use std::fs;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

use base64::Engine;
use rusqlite::backup::Backup;
use rusqlite::ffi;
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params, params_from_iter};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::base::{TaxonomyBaseMetadata, TaxonomyBaseReplaceResult};
use super::formatted::{TaxonomyNameType, validate_taxonomy};
use super::sql::{
    SqlSourceSchema, inspect_sqlite_source, is_safe_identifier, quote_identifier, validated_columns,
};
use super::sql_support::execute_statement_to_completion_raw;
use crate::db::{
    LOCAL_TAXON_ID_FLOOR, TaxonomyReplacementGuard, initialize_taxonomy_database_file,
};
use crate::metadata::{self, MetadataKey};
use crate::naming::normalize_taxonomy_name;
use crate::{CoreError, CoreResult, Database};

const SESSION_MARKER: &str = ".vividarium-base-import";
const SESSION_STATE: &str = "session-state.json";
const SOURCE_DATABASE: &str = "source.db";
const STAGING_DATABASE: &str = "vividarium_base.db";
const CANDIDATE_DATABASE: &str = "candidate-taxonomy.db";
const CANDIDATE_BUILD_DATABASE: &str = ".candidate-building.db";
const IMPORT_BATCH_SIZE: i64 = 10_000;
const MAX_ISSUE_SAMPLES: usize = 100;
const BUILTIN_DEFAULT_BASE_IMPORT_SQL: &str = include_str!("templates/default_base_import.sql");
static SESSION_LOCKS: OnceLock<Mutex<std::collections::HashMap<String, Weak<Mutex<()>>>>> =
    OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BaseImportSession {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddBaseImportCsvSourceRequest {
    pub session_id: String,
    pub table_name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddBaseImportSqliteSourceRequest {
    pub session_id: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecuteBaseImportSqlRequest {
    pub session_id: String,
    pub sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BaseImportExecutionResult {
    pub statements_executed: usize,
    pub session_revision: u64,
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
struct BaseImportSessionState {
    revision: u64,
    candidate: Option<ValidatedBaseImportCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ValidatedBaseImportCandidate {
    candidate_path: PathBuf,
    session_revision: u64,
    staging_fingerprint: String,
    validation_result: BaseImportValidationResult,
}

pub fn create_base_import_session(database: &Database) -> CoreResult<BaseImportSession> {
    let session_id = Uuid::new_v4().to_string();
    let workspace = workspace_root(database).join(&session_id);
    fs::create_dir_all(&workspace)?;
    fs::write(workspace.join(SESSION_MARKER), &session_id)?;
    drop(Connection::open(workspace.join(SOURCE_DATABASE))?);
    write_session_state(
        &workspace,
        &BaseImportSessionState {
            revision: 0,
            candidate: None,
        },
    )?;
    Ok(BaseImportSession { session_id })
}

pub fn add_base_import_csv_source(
    database: &Database,
    request: &AddBaseImportCsvSourceRequest,
) -> CoreResult<SqlSourceSchema> {
    if !is_safe_identifier(&request.table_name)
        || request
            .table_name
            .to_ascii_lowercase()
            .starts_with("sqlite_")
    {
        return Err(CoreError::InvalidArgument(format!(
            "invalid base import source table: {}",
            request.table_name
        )));
    }
    if !request.path.is_file() {
        return Err(CoreError::NotFound(format!(
            "base import CSV source {}",
            request.path.display()
        )));
    }
    let session_lock = session_lock(&request.session_id)?;
    let _session_guard = session_lock
        .lock()
        .map_err(|_| CoreError::Consistency("base import session lock is poisoned".into()))?;
    let workspace = session_workspace(database, &request.session_id)?;
    let mut connection = Connection::open(workspace.join(SOURCE_DATABASE))?;
    let exists = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name = ?)",
        [&request.table_name],
        |row| row.get::<_, bool>(0),
    )?;
    if exists {
        return Err(CoreError::InvalidArgument(format!(
            "base import source object already exists: {}",
            request.table_name
        )));
    }
    load_csv_source_table(&mut connection, &request.table_name, &request.path)?;
    drop(connection);
    invalidate_session(&workspace, true)?;
    inspect_base_import_sources_unlocked(database, &request.session_id)?
        .into_iter()
        .next()
        .ok_or_else(|| CoreError::Consistency("base import source schema was not created".into()))
}

pub fn add_base_import_sqlite_source(
    database: &Database,
    request: &AddBaseImportSqliteSourceRequest,
) -> CoreResult<SqlSourceSchema> {
    if !request.path.is_file() {
        return Err(CoreError::NotFound(format!(
            "base import SQLite source {}",
            request.path.display()
        )));
    }
    let session_lock = session_lock(&request.session_id)?;
    let _session_guard = session_lock
        .lock()
        .map_err(|_| CoreError::Consistency("base import session lock is poisoned".into()))?;
    let workspace = session_workspace(database, &request.session_id)?;
    let source_path = workspace.join(SOURCE_DATABASE);
    let existing = Connection::open(&source_path)?.query_row(
        "SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    if existing != 0 {
        return Err(CoreError::InvalidArgument(
            "SQLite source must be added before CSV source tables".into(),
        ));
    }
    let temporary = workspace.join(".source-import.db");
    remove_file_if_exists(&temporary)?;
    let source = Connection::open_with_flags(
        &request.path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let mut destination = Connection::open(&temporary)?;
    let backup = Backup::new(&source, &mut destination)?;
    backup.run_to_completion(256, Duration::from_millis(10), None)?;
    drop(backup);
    drop(destination);
    drop(source);
    fs::remove_file(&source_path)?;
    fs::rename(&temporary, &source_path)?;
    invalidate_session(&workspace, true)?;
    inspect_base_import_sources_unlocked(database, &request.session_id)?
        .into_iter()
        .next()
        .ok_or_else(|| CoreError::Consistency("base import source schema was not copied".into()))
}

pub fn inspect_base_import_sources(
    database: &Database,
    session_id: &str,
) -> CoreResult<Vec<SqlSourceSchema>> {
    let session_lock = session_lock(session_id)?;
    let _session_guard = session_lock
        .lock()
        .map_err(|_| CoreError::Consistency("base import session lock is poisoned".into()))?;
    inspect_base_import_sources_unlocked(database, session_id)
}

fn inspect_base_import_sources_unlocked(
    database: &Database,
    session_id: &str,
) -> CoreResult<Vec<SqlSourceSchema>> {
    let workspace = session_workspace(database, session_id)?;
    Ok(vec![inspect_sqlite_source(
        "main",
        &workspace.join(SOURCE_DATABASE),
    )?])
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
    let session_lock = session_lock(&request.session_id)?;
    let _session_guard = session_lock
        .lock()
        .map_err(|_| CoreError::Consistency("base import session lock is poisoned".into()))?;
    let workspace = session_workspace(database, &request.session_id)?;
    let revision = invalidate_session(&workspace, true)?;
    let staging = workspace.join(STAGING_DATABASE);
    let staging_path = staging.to_string_lossy().into_owned();
    let sql = replace_staging_literal(sql, &staging_path);
    let connection = Connection::open(workspace.join(SOURCE_DATABASE))?;
    connection.execute_batch("PRAGMA foreign_keys = ON")?;
    let execution = execute_base_import_script(&connection, &sql, &staging_path);
    let attachments = validate_base_import_attachments(&connection);
    let autocommit = unsafe { ffi::sqlite3_get_autocommit(connection.handle()) != 0 };
    if !autocommit {
        let _ = connection.execute_batch("ROLLBACK");
    }
    match (execution, attachments) {
        (Ok(statements_executed), Ok(())) if autocommit => Ok(BaseImportExecutionResult {
            statements_executed,
            session_revision: revision,
        }),
        (Ok(_), Err(error)) => {
            remove_file_if_exists(&staging)?;
            Err(error)
        }
        (Ok(_), Ok(())) => {
            remove_file_if_exists(&staging)?;
            Err(CoreError::InvalidArgument(
                "base import sql left an unfinished transaction".into(),
            ))
        }
        (Err(error), _) => {
            remove_file_if_exists(&staging)?;
            Err(error)
        }
    }
}

pub fn validate_base_import(
    database: &Database,
    session_id: &str,
) -> CoreResult<BaseImportValidationResult> {
    let session_lock = session_lock(session_id)?;
    let _session_guard = session_lock
        .lock()
        .map_err(|_| CoreError::Consistency("base import session lock is poisoned".into()))?;
    let workspace = session_workspace(database, session_id)?;
    let staging = workspace.join(STAGING_DATABASE);
    let mut session_state = read_session_state(&workspace)?;
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
        clear_candidate(&workspace, &mut session_state)?;
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
    if let Some(candidate) = session_state.candidate.as_ref()
        && candidate.session_revision == session_state.revision
        && candidate.staging_fingerprint == staging_fingerprint
        && candidate.candidate_path.is_file()
        && validate_candidate_database(&candidate.candidate_path).is_ok()
    {
        return Ok(candidate.validation_result.clone());
    }
    clear_candidate(&workspace, &mut session_state)?;
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
        if let Err(error) = build_official_taxonomy(
            &staging,
            &candidate_build,
            &format!("base-import:{session_id}"),
        )
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
            session_state.candidate = Some(ValidatedBaseImportCandidate {
                candidate_path,
                session_revision: session_state.revision,
                staging_fingerprint,
                validation_result: validation.clone(),
            });
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
    if let Some(candidate) = session_state.candidate.as_mut() {
        candidate.validation_result = validation.clone();
    }
    write_session_state(&workspace, &session_state)?;
    Ok(validation)
}

pub fn apply_base_import(
    database: &Database,
    session_id: &str,
) -> CoreResult<TaxonomyBaseReplaceResult> {
    let session_lock = session_lock(session_id)?;
    let _session_guard = session_lock
        .lock()
        .map_err(|_| CoreError::Consistency("base import session lock is poisoned".into()))?;
    let replacement_guard = database.try_taxonomy_replacement()?;
    apply_base_import_with_guard(database, session_id, &replacement_guard)
}

fn apply_base_import_with_guard(
    database: &Database,
    session_id: &str,
    replacement_guard: &TaxonomyReplacementGuard<'_>,
) -> CoreResult<TaxonomyBaseReplaceResult> {
    let workspace = session_workspace(database, session_id)?;
    let mut session_state = read_session_state(&workspace)?;
    let candidate = session_state.candidate.clone().ok_or_else(|| {
        CoreError::InvalidArgument("base import must be validated before apply".into())
    })?;
    if !candidate.validation_result.can_apply {
        return Err(CoreError::InvalidArgument(format!(
            "base import validation failed with {} errors",
            candidate.validation_result.total_error_count
        )));
    }
    if candidate.session_revision != session_state.revision {
        return Err(CoreError::InvalidArgument(
            "base import candidate revision is stale".into(),
        ));
    }
    let fingerprint = workspace_fingerprint(&workspace)?;
    if fingerprint != candidate.staging_fingerprint {
        clear_candidate(&workspace, &mut session_state)?;
        return Err(CoreError::InvalidArgument(
            "base import candidate fingerprint is stale".into(),
        ));
    }
    if let Err(error) = validate_candidate_database(&candidate.candidate_path) {
        clear_candidate(&workspace, &mut session_state)?;
        return Err(error);
    }
    let metadata = match candidate_metadata(&candidate.candidate_path) {
        Ok(metadata) => metadata,
        Err(error) => {
            clear_candidate(&workspace, &mut session_state)?;
            return Err(error);
        }
    };
    database.replace_taxonomy_database_file(replacement_guard, &candidate.candidate_path)?;
    session_state.candidate = None;
    let _ = remove_file_if_exists(&candidate.candidate_path);
    let _ = write_session_state(&workspace, &session_state);
    Ok(TaxonomyBaseReplaceResult { metadata })
}

pub fn discard_base_import_session(database: &Database, session_id: &str) -> CoreResult<()> {
    let session_lock = session_lock(session_id)?;
    let _session_guard = session_lock
        .lock()
        .map_err(|_| CoreError::Consistency("base import session lock is poisoned".into()))?;
    let workspace = session_workspace(database, session_id)?;
    fs::remove_dir_all(workspace)?;
    Ok(())
}

pub fn get_default_base_import_sql(database: &Database) -> CoreResult<String> {
    Ok(metadata::get_raw(
        &database.connect_metadata()?,
        MetadataKey::DefaultBaseImportSql,
    )?
    .unwrap_or_else(|| BUILTIN_DEFAULT_BASE_IMPORT_SQL.to_string()))
}

pub fn save_default_base_import_sql(database: &Database, sql: &str) -> CoreResult<()> {
    metadata::set_raw(
        &database.connect_metadata()?,
        MetadataKey::DefaultBaseImportSql,
        sql,
    )
}

pub fn reset_default_base_import_sql(database: &Database) -> CoreResult<String> {
    metadata::remove(
        &database.connect_metadata()?,
        MetadataKey::DefaultBaseImportSql,
    )?;
    Ok(BUILTIN_DEFAULT_BASE_IMPORT_SQL.to_string())
}

fn workspace_root(database: &Database) -> PathBuf {
    database
        .metadata_path()
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("base-import-workspaces")
}

fn session_lock(session_id: &str) -> CoreResult<Arc<Mutex<()>>> {
    let session_id = Uuid::parse_str(session_id)
        .map_err(|_| CoreError::InvalidArgument("invalid base import session id".into()))?
        .to_string();
    let mut locks = SESSION_LOCKS
        .get_or_init(|| Mutex::new(std::collections::HashMap::new()))
        .lock()
        .map_err(|_| CoreError::Consistency("base import lock registry is poisoned".into()))?;
    locks.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = locks.get(&session_id).and_then(Weak::upgrade) {
        return Ok(lock);
    }
    let lock = Arc::new(Mutex::new(()));
    locks.insert(session_id, Arc::downgrade(&lock));
    Ok(lock)
}

fn session_workspace(database: &Database, session_id: &str) -> CoreResult<PathBuf> {
    let parsed = Uuid::parse_str(session_id)
        .map_err(|_| CoreError::InvalidArgument("invalid base import session id".into()))?;
    let workspace = workspace_root(database).join(parsed.to_string());
    let marker = workspace.join(SESSION_MARKER);
    if !workspace.is_dir() || fs::read_to_string(marker).ok().as_deref() != Some(session_id) {
        return Err(CoreError::NotFound(format!(
            "base import session {session_id}"
        )));
    }
    Ok(workspace)
}

fn read_session_state(workspace: &Path) -> CoreResult<BaseImportSessionState> {
    let bytes = fs::read(workspace.join(SESSION_STATE))?;
    serde_json::from_slice(&bytes).map_err(|error| {
        CoreError::Consistency(format!("invalid base import session state: {error}"))
    })
}

fn write_session_state(workspace: &Path, state: &BaseImportSessionState) -> CoreResult<()> {
    let path = workspace.join(SESSION_STATE);
    let temporary = workspace.join(".session-state.json.tmp");
    let bytes = serde_json::to_vec(state).map_err(|error| {
        CoreError::Consistency(format!(
            "could not serialize base import session state: {error}"
        ))
    })?;
    fs::write(&temporary, bytes)?;
    if path.exists() {
        fs::remove_file(&path)?;
    }
    fs::rename(temporary, path)?;
    Ok(())
}

fn invalidate_session(workspace: &Path, remove_staging: bool) -> CoreResult<u64> {
    let mut state = read_session_state(workspace)?;
    state.revision = state
        .revision
        .checked_add(1)
        .ok_or_else(|| CoreError::Consistency("base import session revision overflow".into()))?;
    if let Some(candidate) = state.candidate.take() {
        remove_file_if_exists(&candidate.candidate_path)?;
    }
    remove_file_if_exists(&workspace.join(CANDIDATE_BUILD_DATABASE))?;
    if remove_staging {
        remove_file_if_exists(&workspace.join(STAGING_DATABASE))?;
    }
    write_session_state(workspace, &state)?;
    Ok(state.revision)
}

fn clear_candidate(workspace: &Path, state: &mut BaseImportSessionState) -> CoreResult<()> {
    if let Some(candidate) = state.candidate.take() {
        remove_file_if_exists(&candidate.candidate_path)?;
    }
    remove_file_if_exists(&workspace.join(CANDIDATE_BUILD_DATABASE))?;
    write_session_state(workspace, state)
}

fn workspace_fingerprint(workspace: &Path) -> CoreResult<String> {
    let mut hasher = Sha256::new();
    for filename in [SOURCE_DATABASE, STAGING_DATABASE] {
        let path = workspace.join(filename);
        if !path.is_file() {
            return Err(CoreError::NotFound(format!(
                "base import file {}",
                path.display()
            )));
        }
        hasher.update(filename.as_bytes());
        let mut reader = BufReader::new(File::open(path)?);
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
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

fn load_csv_source_table(
    connection: &mut Connection,
    table_name: &str,
    path: &Path,
) -> CoreResult<()> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(false)
        .from_path(path)?;
    let columns = validated_columns(reader.headers()?.iter())?;
    let definitions = columns
        .iter()
        .map(|column| format!("{} TEXT", quote_identifier(column)))
        .collect::<Vec<_>>()
        .join(", ");
    let transaction = connection.transaction()?;
    transaction.execute_batch(&format!(
        "CREATE TABLE {} ({definitions})",
        quote_identifier(table_name)
    ))?;
    let column_list = columns
        .iter()
        .map(|column| quote_identifier(column))
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = std::iter::repeat_n("?", columns.len())
        .collect::<Vec<_>>()
        .join(", ");
    let mut insert = transaction.prepare(&format!(
        "INSERT INTO {} ({column_list}) VALUES ({placeholders})",
        quote_identifier(table_name)
    ))?;
    for (index, record) in reader.records().enumerate() {
        let row_number = index + 2;
        let record = record.map_err(|error| {
            CoreError::InvalidArgument(format!("CSV row {row_number} could not be read: {error}"))
        })?;
        insert
            .execute(params_from_iter(record.iter()))
            .map_err(|error| {
                CoreError::InvalidArgument(format!(
                    "CSV row {row_number} could not be inserted: {error}"
                ))
            })?;
    }
    drop(insert);
    transaction.commit()?;
    Ok(())
}

fn execute_base_import_script(
    connection: &Connection,
    sql: &str,
    staging_path: &str,
) -> CoreResult<usize> {
    let mut offset = 0;
    let mut statements_executed = 0;
    while offset < sql.len() {
        connection.authorizer(Some(base_import_authorizer(staging_path.to_string())));
        let execution = unsafe { execute_statement_to_completion_raw(connection, &sql[offset..]) };
        connection.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
        let execution = execution?;
        offset += execution.tail_offset;
        if execution.statement.is_some() {
            statements_executed += 1;
        }
    }
    Ok(statements_executed)
}

fn validate_base_import_attachments(connection: &Connection) -> CoreResult<()> {
    let mut statement = connection.prepare("PRAGMA database_list")?;
    let attached = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(alias) = attached
        .iter()
        .find(|alias| !matches!(alias.as_str(), "main" | "temp" | "base"))
    {
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
