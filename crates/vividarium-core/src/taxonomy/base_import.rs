use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, TryLockError};

use base64::Engine;
use rusqlite::ffi;
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::base::{TaxonomyBaseMetadata, TaxonomyBaseReplaceResult};
use super::formatted::{TaxonomyNameType, validate_taxonomy};
use super::sql::{SqlStatementMessage, detach_sources, prepare_sources, quote_identifier};
use super::sql_inputs::{
    self, AddSqlInputRequest, AddSqlInputResult, PersistentSqlInput, RemoveSqlInputRequest,
    RemoveSqlInputResult, SqlInputScope,
};
use super::sql_support::execute_statement_to_completion_raw;
use crate::db::{
    LOCAL_TAXON_ID_FLOOR, TaxonomyReplacementGuard, initialize_taxonomy_database_file,
};
use crate::metadata::{self, MetadataKey};
use crate::naming::normalize_taxonomy_name;
use crate::{CoreError, CoreResult, Database};

const STAGING_DATABASE: &str = "vividarium_base.db";
const CANDIDATE_DATABASE: &str = "candidate-taxonomy.db";
const CANDIDATE_BUILD_DATABASE: &str = ".candidate-building.db";
const VALIDATION_STATE: &str = "validation.json";
const IMPORT_BATCH_SIZE: i64 = 10_000;
const MAX_ISSUE_SAMPLES: usize = 100;
const INITIAL_BASE_IMPORT_SQL: &str = include_str!("templates/initial_base_import.sql");
static WORKSPACE_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecuteBaseImportSqlRequest {
    pub sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BaseImportExecutionResult {
    pub statements_executed: usize,
    pub messages: Vec<SqlStatementMessage>,
    pub script_saved: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NameTypeCount {
    pub name_type: TaxonomyNameType,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BaseImportIssue {
    pub code: String,
    pub message: String,
    pub table: Option<String>,
    pub row_identifier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BaseImportValidationResult {
    pub can_apply: bool,
    pub taxa_count: u64,
    pub name_counts: Vec<NameTypeCount>,
    pub normalization_changes: u64,
    pub total_warning_count: u64,
    pub total_error_count: u64,
    pub warnings: Vec<BaseImportIssue>,
    pub errors: Vec<BaseImportIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ValidatedBaseImportCandidate {
    staging_fingerprint: String,
    validation_result: BaseImportValidationResult,
}

pub fn list_base_import_inputs(database: &Database) -> CoreResult<Vec<PersistentSqlInput>> {
    sql_inputs::list_inputs(database, SqlInputScope::BaseImport)
}

pub fn add_base_import_input(
    database: &Database,
    request: &AddSqlInputRequest,
) -> CoreResult<AddSqlInputResult> {
    let workspace_mutex = workspace_mutex(database)?;
    let _guard = lock_workspace(&workspace_mutex)?;
    let workspace = workspace(database)?;
    let invalidation = ArtifactInvalidation::stage(&workspace)?;
    match sql_inputs::add_input(database, SqlInputScope::BaseImport, request) {
        Ok(mut result) => {
            result.warnings.extend(invalidation.commit(database));
            Ok(result)
        }
        Err(error) => match invalidation.rollback() {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(CoreError::Consistency(format!(
                "{error}; failed base import artifact restore: {rollback_error}"
            ))),
        },
    }
}

pub fn remove_base_import_input(
    database: &Database,
    request: &RemoveSqlInputRequest,
) -> CoreResult<RemoveSqlInputResult> {
    let workspace_mutex = workspace_mutex(database)?;
    let _guard = try_lock_workspace(&workspace_mutex)?;
    let workspace = workspace(database)?;
    let invalidation = ArtifactInvalidation::stage(&workspace)?;
    match sql_inputs::remove_input(database, SqlInputScope::BaseImport, request) {
        Ok(mut result) => {
            result.warnings.extend(invalidation.commit(database));
            Ok(result)
        }
        Err(error) => match invalidation.rollback() {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(CoreError::Consistency(format!(
                "{error}; failed base import artifact restore: {rollback_error}"
            ))),
        },
    }
}

pub fn execute_base_import_sql(
    database: &Database,
    request: &ExecuteBaseImportSqlRequest,
) -> CoreResult<BaseImportExecutionResult> {
    let sql = request.sql.trim();
    if sql.is_empty() {
        return Err(CoreError::InvalidArgument(
            "base import sql is required".into(),
        ));
    }
    let workspace_mutex = workspace_mutex(database)?;
    let _guard = lock_workspace(&workspace_mutex)?;
    let workspace = workspace(database)?;
    let invalidation = ArtifactInvalidation::stage(&workspace)?;
    let staging = workspace.join(STAGING_DATABASE);
    let staging_path = staging.to_string_lossy().into_owned();
    let sql = replace_staging_literal(sql, &staging_path);
    let execution: CoreResult<Vec<SqlStatementMessage>> = (|| {
        let mut connection = Connection::open_in_memory()?;
        connection.execute_batch("PRAGMA foreign_keys = ON")?;
        let sources = sql_inputs::stored_sources(database, SqlInputScope::BaseImport)?;
        let attached = prepare_sources(&mut connection, &sources)?;
        let execution = execute_base_import_script(&connection, &sql, &staging_path);
        let attachments = validate_base_import_attachments(&connection, &attached);
        let autocommit = unsafe { ffi::sqlite3_get_autocommit(connection.handle()) != 0 };
        if !autocommit {
            let _ = connection.execute_batch("ROLLBACK");
        }
        let detach = detach_sources(&connection, &attached);
        match (execution, attachments, detach) {
            (Ok(messages), Ok(()), Ok(())) if autocommit => Ok(messages),
            (Ok(_), Err(error), _) | (Ok(_), Ok(()), Err(error)) => Err(error),
            (Ok(_), Ok(()), Ok(())) => Err(CoreError::InvalidArgument(
                "base import sql left an unfinished transaction".into(),
            )),
            (Err(error), _, _) => Err(error),
        }
    })();
    match execution {
        Ok(messages) => {
            let mut result = BaseImportExecutionResult {
                statements_executed: messages.len(),
                messages,
                script_saved: false,
                warnings: invalidation.commit(database),
            };
            match database.connect_metadata().and_then(|connection| {
                metadata::set_raw(&connection, MetadataKey::BaseImportSql, &request.sql)
            }) {
                Ok(()) => result.script_saved = true,
                Err(error) => result.warnings.push(format!(
                    "base import SQL committed, but the script could not be saved: {error}"
                )),
            }
            Ok(result)
        }
        Err(error) => restore_invalidated_artifacts(invalidation, &staging, error),
    }
}

pub fn validate_base_import(database: &Database) -> CoreResult<BaseImportValidationResult> {
    let workspace_mutex = workspace_mutex(database)?;
    let _guard = lock_workspace(&workspace_mutex)?;
    let workspace = workspace(database)?;
    let staging = workspace.join(STAGING_DATABASE);
    let mut validation = BaseImportValidationResult {
        can_apply: false,
        taxa_count: 0,
        name_counts: Vec::new(),
        normalization_changes: 0,
        total_warning_count: 0,
        total_error_count: 0,
        warnings: Vec::new(),
        errors: Vec::new(),
    };
    if !staging.is_file() {
        clear_validation_artifacts(&workspace)?;
        record_error(
            &mut validation,
            "staging_missing",
            "base import staging database does not exist",
            None,
            None,
        );
        return Ok(validation);
    }
    let staging_fingerprint = workspace_fingerprint(&workspace)?;
    if let Some(candidate) = read_validation_state(&workspace)?
        && candidate.staging_fingerprint == staging_fingerprint
        && workspace.join(CANDIDATE_DATABASE).is_file()
        && validate_candidate_database(&workspace.join(CANDIDATE_DATABASE)).is_ok()
    {
        return Ok(candidate.validation_result);
    }
    clear_validation_artifacts(&workspace)?;
    let connection = match Connection::open_with_flags(
        &staging,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(connection) => connection,
        Err(error) => {
            record_error(
                &mut validation,
                "staging_open_failed",
                &error.to_string(),
                None,
                None,
            );
            return Ok(validation);
        }
    };
    validate_integrity(&connection, &mut validation)?;
    validate_staging_schema(&connection, &mut validation)?;
    if validation.total_error_count == 0 {
        validation.taxa_count =
            connection.query_row("SELECT COUNT(*) FROM taxa", [], |row| row.get::<_, u64>(0))?;
        let mut counts = connection.prepare(
            "SELECT name_type, COUNT(*) FROM taxon_names GROUP BY name_type ORDER BY name_type",
        )?;
        validation.name_counts = counts
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, u64>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter_map(|(code, count)| {
                TaxonomyNameType::from_code(code)
                    .ok()
                    .map(|name_type| NameTypeCount { name_type, count })
            })
            .collect();
        let mut names = connection.prepare("SELECT name_id, name FROM taxon_names")?;
        for row in names.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })? {
            let (name_id, raw_name) = row?;
            match normalize_taxonomy_name(&raw_name) {
                Some(name) => {
                    if name != raw_name {
                        validation.normalization_changes += 1;
                    }
                }
                None => record_error(
                    &mut validation,
                    "empty_canonical_name",
                    "name is empty after canonical normalization",
                    Some("taxon_names"),
                    Some(name_id.to_string()),
                ),
            }
        }
    }
    drop(connection);
    if validation.total_error_count == 0 {
        let candidate_build = workspace.join(CANDIDATE_BUILD_DATABASE);
        remove_file_if_exists(&candidate_build)?;
        if let Err(error) = build_official_taxonomy(&staging, &candidate_build, "base-import")
            .and_then(|_| validate_candidate_database(&candidate_build))
        {
            remove_file_if_exists(&candidate_build)?;
            record_error(
                &mut validation,
                "taxonomy_validation_failed",
                &error.to_string(),
                None,
                None,
            );
        } else {
            let candidate_path = workspace.join(CANDIDATE_DATABASE);
            remove_file_if_exists(&candidate_path)?;
            fs::rename(candidate_build, &candidate_path)?;
        }
    }
    if validation.normalization_changes > 0 {
        validation.total_warning_count += 1;
        validation.warnings.push(BaseImportIssue {
            code: "canonical_normalization".into(),
            message: format!(
                "{} names will change during canonical normalization",
                validation.normalization_changes
            ),
            table: Some("taxon_names".into()),
            row_identifier: None,
        });
    }
    validation.can_apply = validation.total_error_count == 0;
    if validation.can_apply && workspace.join(CANDIDATE_DATABASE).is_file() {
        write_validation_state(
            &workspace,
            &ValidatedBaseImportCandidate {
                staging_fingerprint,
                validation_result: validation.clone(),
            },
        )?;
    }
    Ok(validation)
}

pub fn apply_base_import(database: &Database) -> CoreResult<TaxonomyBaseReplaceResult> {
    let workspace_mutex = workspace_mutex(database)?;
    let _guard = lock_workspace(&workspace_mutex)?;
    let replacement_guard = database.try_taxonomy_replacement()?;
    apply_base_import_with_guard(database, &replacement_guard)
}

fn apply_base_import_with_guard(
    database: &Database,
    replacement_guard: &TaxonomyReplacementGuard<'_>,
) -> CoreResult<TaxonomyBaseReplaceResult> {
    let workspace = workspace(database)?;
    let candidate = read_validation_state(&workspace)?.ok_or_else(|| {
        CoreError::InvalidArgument("base import must be validated before apply".into())
    })?;
    if !candidate.validation_result.can_apply {
        return Err(CoreError::InvalidArgument(format!(
            "base import validation failed with {} errors",
            candidate.validation_result.total_error_count
        )));
    }
    let fingerprint = workspace_fingerprint(&workspace)?;
    if fingerprint != candidate.staging_fingerprint {
        clear_validation_artifacts(&workspace)?;
        return Err(CoreError::InvalidArgument(
            "base import candidate fingerprint is stale".into(),
        ));
    }
    let candidate_path = workspace.join(CANDIDATE_DATABASE);
    if let Err(error) = validate_candidate_database(&candidate_path) {
        clear_validation_artifacts(&workspace)?;
        return Err(error);
    }
    let metadata = match candidate_metadata(&candidate_path) {
        Ok(metadata) => metadata,
        Err(error) => {
            clear_validation_artifacts(&workspace)?;
            return Err(error);
        }
    };
    database.replace_taxonomy_database_file(replacement_guard, &candidate_path)?;
    let warnings = cleanup_build_artifacts(database, &workspace);
    Ok(TaxonomyBaseReplaceResult { metadata, warnings })
}

pub fn get_base_import_sql(database: &Database) -> CoreResult<String> {
    Ok(
        metadata::get_raw(&database.connect_metadata()?, MetadataKey::BaseImportSql)?
            .unwrap_or_else(|| INITIAL_BASE_IMPORT_SQL.to_string()),
    )
}

fn workspace_mutex(database: &Database) -> CoreResult<Arc<Mutex<()>>> {
    let path = workspace(database)?;
    let mut locks = WORKSPACE_LOCKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| CoreError::Consistency("base import lock registry is poisoned".into()))?;
    Ok(locks
        .entry(path)
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone())
}

fn lock_workspace(mutex: &Mutex<()>) -> CoreResult<std::sync::MutexGuard<'_, ()>> {
    mutex
        .lock()
        .map_err(|_| CoreError::Consistency("base import workspace lock is poisoned".into()))
}

fn try_lock_workspace(mutex: &Mutex<()>) -> CoreResult<std::sync::MutexGuard<'_, ()>> {
    match mutex.try_lock() {
        Ok(guard) => Ok(guard),
        Err(TryLockError::WouldBlock) => Err(CoreError::InvalidArgument(
            "base import workspace is busy".into(),
        )),
        Err(TryLockError::Poisoned(_)) => Err(CoreError::Consistency(
            "base import workspace lock is poisoned".into(),
        )),
    }
}

fn workspace(database: &Database) -> CoreResult<PathBuf> {
    let workspace = database
        .metadata_path()
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("base-import-workspace");
    fs::create_dir_all(&workspace)?;
    Ok(workspace)
}

fn read_validation_state(workspace: &Path) -> CoreResult<Option<ValidatedBaseImportCandidate>> {
    let path = workspace.join(VALIDATION_STATE);
    if !path.is_file() {
        return Ok(None);
    }
    serde_json::from_slice(&fs::read(path)?)
        .map(Some)
        .map_err(|error| CoreError::Consistency(format!("invalid validation state: {error}")))
}

fn write_validation_state(
    workspace: &Path,
    state: &ValidatedBaseImportCandidate,
) -> CoreResult<()> {
    let path = workspace.join(VALIDATION_STATE);
    let temporary = workspace.join(".validation.json.tmp");
    let bytes = serde_json::to_vec(state).map_err(|error| {
        CoreError::Consistency(format!("could not serialize validation state: {error}"))
    })?;
    fs::write(&temporary, bytes)?;
    if path.exists() {
        fs::remove_file(&path)?;
    }
    fs::rename(temporary, path)?;
    Ok(())
}

fn clear_validation_artifacts(workspace: &Path) -> CoreResult<()> {
    for filename in [
        CANDIDATE_DATABASE,
        CANDIDATE_BUILD_DATABASE,
        VALIDATION_STATE,
    ] {
        remove_file_if_exists(&workspace.join(filename))?;
    }
    Ok(())
}

struct ArtifactInvalidation {
    paths: Vec<(PathBuf, PathBuf)>,
}

impl ArtifactInvalidation {
    fn stage(workspace: &Path) -> CoreResult<Self> {
        let mut paths = Vec::new();
        for filename in [
            STAGING_DATABASE,
            CANDIDATE_DATABASE,
            CANDIDATE_BUILD_DATABASE,
            VALIDATION_STATE,
        ] {
            let original = workspace.join(filename);
            if !original.exists() {
                continue;
            }
            let staged = workspace.join(format!(".invalidated-{filename}"));
            remove_file_if_exists(&staged)?;
            if let Err(error) = fs::rename(&original, &staged) {
                let invalidation = Self { paths };
                return match invalidation.rollback() {
                    Ok(()) => Err(error.into()),
                    Err(rollback_error) => Err(CoreError::Consistency(format!(
                        "{error}; failed base import artifact restore: {rollback_error}"
                    ))),
                };
            }
            paths.push((original, staged));
        }
        Ok(Self { paths })
    }

    fn rollback(self) -> CoreResult<()> {
        for (original, staged) in self.paths.into_iter().rev() {
            if staged.exists() {
                fs::rename(staged, original)?;
            }
        }
        Ok(())
    }

    fn commit(self, database: &Database) -> Vec<String> {
        let mut warnings = Vec::new();
        for (_, staged) in self.paths {
            if let Some(warning) =
                super::cleanup::remove_or_defer(database, &staged, "base import artifact")
            {
                warnings.push(warning);
            }
        }
        warnings
    }
}

fn restore_invalidated_artifacts<T>(
    invalidation: ArtifactInvalidation,
    staging: &Path,
    error: CoreError,
) -> CoreResult<T> {
    if let Err(cleanup_error) = remove_file_if_exists(staging) {
        return Err(CoreError::Consistency(format!(
            "{error}; failed base import staging cleanup: {cleanup_error}"
        )));
    }
    if let Err(rollback_error) = invalidation.rollback() {
        return Err(CoreError::Consistency(format!(
            "{error}; failed base import artifact restore: {rollback_error}"
        )));
    }
    Err(error)
}

fn cleanup_build_artifacts(database: &Database, workspace: &Path) -> Vec<String> {
    let mut warnings = super::cleanup::retry_pending(database);
    for filename in [
        CANDIDATE_DATABASE,
        CANDIDATE_BUILD_DATABASE,
        VALIDATION_STATE,
        STAGING_DATABASE,
    ] {
        if let Some(warning) = super::cleanup::remove_or_defer(
            database,
            &workspace.join(filename),
            "base import artifact",
        ) {
            warnings.push(warning);
        }
    }
    warnings
}

fn workspace_fingerprint(workspace: &Path) -> CoreResult<String> {
    let mut hasher = Sha256::new();
    let path = workspace.join(STAGING_DATABASE);
    if !path.is_file() {
        return Err(CoreError::NotFound(format!(
            "base import file {}",
            path.display()
        )));
    }
    let mut reader = BufReader::new(File::open(path)?);
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(base64::engine::general_purpose::STANDARD_NO_PAD.encode(hasher.finalize()))
}

fn validate_candidate_database(path: &Path) -> CoreResult<()> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let quick_check =
        connection.query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))?;
    if quick_check != "ok" {
        return Err(CoreError::InvalidArgument(format!(
            "candidate quick check failed: {quick_check}"
        )));
    }
    let foreign_key_error = connection
        .query_row("PRAGMA foreign_key_check", [], |_| Ok(()))
        .optional()?;
    if foreign_key_error.is_some() {
        return Err(CoreError::InvalidArgument(
            "candidate foreign key check failed".into(),
        ));
    }
    validate_taxonomy(&connection)?;
    let identity = connection
        .query_row(
            "SELECT taxonomy_identity FROM taxonomy_identity WHERE identity_id = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if identity.as_deref().is_none_or(str::is_empty) {
        return Err(CoreError::InvalidArgument(
            "candidate taxonomy identity is missing".into(),
        ));
    }
    Ok(())
}

fn candidate_metadata(path: &Path) -> CoreResult<TaxonomyBaseMetadata> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?
    .query_row(
        r#"
        SELECT source_path, taxa_count, taxon_names_count, imported_at
        FROM taxonomy_base_metadata
        WHERE metadata_id = 1
        "#,
        [],
        |row| {
            Ok(TaxonomyBaseMetadata {
                source_path: row.get(0)?,
                taxa_count: row.get(1)?,
                taxon_names_count: row.get(2)?,
                imported_at: row.get(3)?,
            })
        },
    )
    .map_err(Into::into)
}

fn execute_base_import_script(
    connection: &Connection,
    sql: &str,
    staging_path: &str,
) -> CoreResult<Vec<SqlStatementMessage>> {
    let mut offset = 0;
    let mut messages = Vec::new();
    while offset < sql.len() {
        connection.authorizer(Some(base_import_authorizer(staging_path.to_string())));
        let execution = unsafe { execute_statement_to_completion_raw(connection, &sql[offset..]) };
        connection.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
        let execution = execution?;
        offset += execution.tail_offset;
        if let Some(statement) = execution.statement {
            let statement_index = messages.len() + 1;
            let affected_rows = (!statement.read_only).then_some(statement.affected_rows);
            let message = match affected_rows {
                Some(count) => format!("statement affected {count} rows"),
                None => format!("statement returned {} rows", statement.returned_rows),
            };
            messages.push(SqlStatementMessage {
                statement_index,
                affected_rows,
                message,
            });
        }
    }
    Ok(messages)
}

fn validate_base_import_attachments(
    connection: &Connection,
    source_aliases: &[String],
) -> CoreResult<()> {
    let mut statement = connection.prepare("PRAGMA database_list")?;
    let attached = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(alias) = attached.iter().find(|alias| {
        !matches!(alias.as_str(), "main" | "temp" | "base")
            && !source_aliases.iter().any(|source| source == *alias)
    }) {
        return Err(CoreError::InvalidArgument(format!(
            "base import staging database must use the base alias, not {alias}"
        )));
    }
    Ok(())
}

fn replace_staging_literal(sql: &str, staging_path: &str) -> String {
    sql.replace(
        "'vividarium_base.db'",
        &format!("'{}'", staging_path.replace('\'', "''")),
    )
}

fn base_import_authorizer(
    staging_path: String,
) -> impl for<'a> FnMut(AuthContext<'a>) -> Authorization + Send + 'static {
    move |context| match context.action {
        AuthAction::Select | AuthAction::Recursive | AuthAction::Transaction { .. } => {
            Authorization::Allow
        }
        AuthAction::Function { function_name } => {
            if function_name.eq_ignore_ascii_case("load_extension") {
                Authorization::Deny
            } else {
                Authorization::Allow
            }
        }
        AuthAction::Attach { filename } => {
            if filename == staging_path {
                Authorization::Allow
            } else {
                Authorization::Deny
            }
        }
        AuthAction::Detach { database_name } => {
            if database_name.eq_ignore_ascii_case("base") {
                Authorization::Allow
            } else {
                Authorization::Deny
            }
        }
        AuthAction::Pragma {
            pragma_name,
            pragma_value,
        } => {
            if pragma_name.eq_ignore_ascii_case("foreign_keys")
                && pragma_value.is_some_and(|value| value.eq_ignore_ascii_case("ON"))
            {
                Authorization::Allow
            } else {
                Authorization::Deny
            }
        }
        AuthAction::Read { .. } => Authorization::Allow,
        AuthAction::Insert { .. }
        | AuthAction::Update { .. }
        | AuthAction::Delete { .. }
        | AuthAction::CreateIndex { .. }
        | AuthAction::CreateTable { .. }
        | AuthAction::CreateTrigger { .. }
        | AuthAction::CreateView { .. }
        | AuthAction::DropIndex { .. }
        | AuthAction::DropTable { .. }
        | AuthAction::DropTrigger { .. }
        | AuthAction::DropView { .. }
        | AuthAction::AlterTable { .. }
        | AuthAction::CreateVtable { .. }
        | AuthAction::DropVtable { .. }
        | AuthAction::Reindex { .. }
        | AuthAction::Analyze { .. } => {
            if context.database_name == Some("base") {
                Authorization::Allow
            } else {
                Authorization::Deny
            }
        }
        _ => Authorization::Deny,
    }
}

fn validate_integrity(
    connection: &Connection,
    validation: &mut BaseImportValidationResult,
) -> CoreResult<()> {
    let mut integrity = connection.prepare("PRAGMA integrity_check")?;
    for row in integrity.query_map([], |row| row.get::<_, String>(0))? {
        let message = row?;
        if message != "ok" {
            record_error(validation, "integrity_check", &message, None, None);
        }
    }
    let mut foreign_keys = connection.prepare("PRAGMA foreign_key_check")?;
    for row in foreign_keys.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?))
    })? {
        let (table, row_id) = row?;
        record_error(
            validation,
            "foreign_key_check",
            "foreign key violation",
            Some(&table),
            row_id.map(|value| value.to_string()),
        );
    }
    Ok(())
}

fn validate_staging_schema(
    connection: &Connection,
    validation: &mut BaseImportValidationResult,
) -> CoreResult<()> {
    validate_table(
        connection,
        validation,
        "taxa",
        &[
            ("taxon_id", true, true, false),
            ("parent_taxon_id", true, false, false),
            ("rank", true, false, true),
            ("geological_range", false, false, false),
        ],
    )?;
    validate_table(
        connection,
        validation,
        "taxon_names",
        &[
            ("name_id", true, true, false),
            ("taxon_id", true, false, true),
            ("name_type", true, false, true),
            ("name", false, false, true),
            ("normalized_name", false, false, false),
            ("authority_year", false, false, false),
            ("source", false, false, false),
        ],
    )?;
    validate_foreign_key(
        connection,
        validation,
        "taxa",
        "parent_taxon_id",
        "taxa",
        "taxon_id",
        "RESTRICT",
    )?;
    validate_foreign_key(
        connection,
        validation,
        "taxon_names",
        "taxon_id",
        "taxa",
        "taxon_id",
        "CASCADE",
    )?;
    validate_unique_columns(
        connection,
        validation,
        "taxon_names",
        &["taxon_id", "name_type", "name"],
    )?;
    if validation.total_error_count == 0 {
        let invalid_ranks = connection.query_row(
            "SELECT COUNT(*) FROM taxa WHERE rank NOT BETWEEN 1 AND 5",
            [],
            |row| row.get::<_, u64>(0),
        )?;
        if invalid_ranks > 0 {
            record_error(
                validation,
                "invalid_rank",
                &format!("{invalid_ranks} taxa use unsupported ranks"),
                Some("taxa"),
                None,
            );
        }
        let invalid_types = connection.query_row(
            "SELECT COUNT(*) FROM taxon_names WHERE name_type NOT BETWEEN 1 AND 6",
            [],
            |row| row.get::<_, u64>(0),
        )?;
        if invalid_types > 0 {
            record_error(
                validation,
                "invalid_name_type",
                &format!("{invalid_types} names use unsupported name types"),
                Some("taxon_names"),
                None,
            );
        }
    }
    Ok(())
}

fn validate_table(
    connection: &Connection,
    validation: &mut BaseImportValidationResult,
    table: &str,
    required: &[(&str, bool, bool, bool)],
) -> CoreResult<()> {
    let object_type = connection
        .query_row(
            "SELECT type FROM sqlite_schema WHERE name = ?",
            [table],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if object_type.as_deref() != Some("table") {
        record_error(
            validation,
            "required_table_missing",
            &format!("required table {table} is missing"),
            Some(table),
            None,
        );
        return Ok(());
    }
    let mut statement =
        connection.prepare(&format!("PRAGMA table_xinfo({})", quote_identifier(table)))?;
    let columns = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
                row.get::<_, i64>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (name, integer, primary_key, not_null) in required {
        match columns.iter().find(|column| column.0 == *name) {
            None => record_error(
                validation,
                "required_column_missing",
                &format!("required column {table}.{name} is missing"),
                Some(table),
                None,
            ),
            Some((_, declared_type, column_not_null, pk)) => {
                if *integer && !declared_type.to_ascii_uppercase().contains("INT") {
                    record_error(
                        validation,
                        "incompatible_column_type",
                        &format!("{table}.{name} must use INTEGER affinity"),
                        Some(table),
                        None,
                    );
                }
                if *primary_key && *pk == 0 {
                    record_error(
                        validation,
                        "primary_key_missing",
                        &format!("{table}.{name} must be a primary key"),
                        Some(table),
                        None,
                    );
                }
                if *not_null && !column_not_null {
                    record_error(
                        validation,
                        "not_null_constraint_missing",
                        &format!("{table}.{name} must be NOT NULL"),
                        Some(table),
                        None,
                    );
                }
            }
        }
    }
    Ok(())
}

fn validate_foreign_key(
    connection: &Connection,
    validation: &mut BaseImportValidationResult,
    table: &str,
    from_column: &str,
    target_table: &str,
    target_column: &str,
    on_delete: &str,
) -> CoreResult<()> {
    let mut statement = connection.prepare(&format!(
        "PRAGMA foreign_key_list({})",
        quote_identifier(table)
    ))?;
    let foreign_keys = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let present = foreign_keys.iter().any(|foreign_key| {
        foreign_key.0 == target_table
            && foreign_key.1 == from_column
            && foreign_key.2.as_deref() == Some(target_column)
            && foreign_key.3.eq_ignore_ascii_case(on_delete)
    });
    if !present {
        record_error(
            validation,
            "foreign_key_constraint_missing",
            &format!(
                "{table}.{from_column} must reference {target_table}.{target_column} ON DELETE {on_delete}"
            ),
            Some(table),
            None,
        );
    }
    Ok(())
}

fn validate_unique_columns(
    connection: &Connection,
    validation: &mut BaseImportValidationResult,
    table: &str,
    required_columns: &[&str],
) -> CoreResult<()> {
    let mut indexes =
        connection.prepare(&format!("PRAGMA index_list({})", quote_identifier(table)))?;
    let unique_indexes = indexes
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, bool>(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut present = false;
    for (index, unique) in unique_indexes {
        if !unique {
            continue;
        }
        let mut columns =
            connection.prepare(&format!("PRAGMA index_info({})", quote_identifier(&index)))?;
        let columns = columns
            .query_map([], |row| row.get::<_, String>(2))?
            .collect::<Result<Vec<_>, _>>()?;
        if columns
            .iter()
            .map(String::as_str)
            .eq(required_columns.iter().copied())
        {
            present = true;
            break;
        }
    }
    if !present {
        record_error(
            validation,
            "unique_constraint_missing",
            &format!(
                "{table} must enforce UNIQUE ({})",
                required_columns.join(", ")
            ),
            Some(table),
            None,
        );
    }
    Ok(())
}

fn build_official_taxonomy(
    staging: &Path,
    destination: &Path,
    source_label: &str,
) -> CoreResult<TaxonomyBaseMetadata> {
    initialize_taxonomy_database_file(destination)?;
    let mut connection = Connection::open(destination)?;
    connection.execute_batch("PRAGMA foreign_keys = ON")?;
    connection.execute("ATTACH DATABASE ? AS staging", [staging.to_string_lossy()])?;
    let transaction = connection.transaction()?;
    transaction.execute(
        r#"
        INSERT INTO taxa (taxon_id, parent_taxon_id, rank, geological_range)
        SELECT taxon_id, parent_taxon_id, rank, geological_range
        FROM staging.taxa
        ORDER BY rank, taxon_id
        "#,
        [],
    )?;
    let mut insert = transaction.prepare_cached(
        r#"
        INSERT INTO taxon_names (
            name_id, taxon_id, name_type, name, authority_year, source
        ) VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )?;
    let mut last_name_id = None;
    loop {
        let names = {
            let (sql, cursor) = match last_name_id {
                Some(name_id) => (
                    r#"
                    SELECT name_id, taxon_id, name_type, name, authority_year, source
                    FROM staging.taxon_names
                    WHERE name_id > ?
                    ORDER BY name_id
                    LIMIT ?
                    "#,
                    name_id,
                ),
                None => (
                    r#"
                    SELECT name_id, taxon_id, name_type, name, authority_year, source
                    FROM staging.taxon_names
                    WHERE name_id >= ?
                    ORDER BY name_id
                    LIMIT ?
                    "#,
                    i64::MIN,
                ),
            };
            let mut statement = transaction.prepare(sql)?;
            statement
                .query_map(params![cursor, IMPORT_BATCH_SIZE], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        if names.is_empty() {
            break;
        }
        for (name_id, taxon_id, name_type, raw_name, authority_year, source) in names {
            let name = normalize_taxonomy_name(&raw_name).ok_or_else(|| {
                CoreError::InvalidArgument(format!(
                    "taxonomy base name {name_id} is empty after normalization"
                ))
            })?;
            insert.execute(params![
                name_id,
                taxon_id,
                name_type,
                name,
                authority_year,
                source
            ])?;
            last_name_id = Some(name_id);
        }
    }
    drop(insert);
    validate_taxonomy(&transaction)?;
    transaction.execute(
        r#"
        UPDATE sqlite_sequence
        SET seq = max(
            seq,
            ?,
            COALESCE((SELECT MAX(taxon_id) FROM taxa), 0)
        )
        WHERE name = 'taxa'
        "#,
        [LOCAL_TAXON_ID_FLOOR],
    )?;
    let taxa_count =
        transaction.query_row("SELECT COUNT(*) FROM taxa", [], |row| row.get::<_, i64>(0))?;
    let taxon_names_count =
        transaction.query_row("SELECT COUNT(*) FROM taxon_names", [], |row| {
            row.get::<_, i64>(0)
        })?;
    transaction.execute(
        r#"
        INSERT INTO taxonomy_base_metadata (
            metadata_id, source_path, taxa_count, taxon_names_count
        ) VALUES (1, ?, ?, ?)
        "#,
        params![source_label, taxa_count, taxon_names_count],
    )?;
    let metadata = transaction.query_row(
        r#"
        SELECT source_path, taxa_count, taxon_names_count, imported_at
        FROM taxonomy_base_metadata
        WHERE metadata_id = 1
        "#,
        [],
        |row| {
            Ok(TaxonomyBaseMetadata {
                source_path: row.get(0)?,
                taxa_count: row.get(1)?,
                taxon_names_count: row.get(2)?,
                imported_at: row.get(3)?,
            })
        },
    )?;
    transaction.commit()?;
    connection.execute_batch("DETACH DATABASE staging")?;
    Ok(metadata)
}

fn record_error(
    validation: &mut BaseImportValidationResult,
    code: &str,
    message: &str,
    table: Option<&str>,
    row_identifier: Option<String>,
) {
    validation.total_error_count += 1;
    if validation.errors.len() < MAX_ISSUE_SAMPLES {
        validation.errors.push(BaseImportIssue {
            code: code.into(),
            message: message.into(),
            table: table.map(str::to_string),
            row_identifier,
        });
    }
}

fn remove_file_if_exists(path: &Path) -> CoreResult<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "base_import/tests.rs"]
mod tests;
