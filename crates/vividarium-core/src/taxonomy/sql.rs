use std::collections::{BTreeSet, HashMap, HashSet};
use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::{Arc, Mutex, OnceLock};

use rusqlite::ffi;
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::{Connection, OpenFlags, Transaction, TransactionBehavior, params, params_from_iter};
use serde::{Deserialize, Serialize};

use super::formatted::{
    affected_taxon_ids_from_changeset, start_taxonomy_session, validate_taxonomy,
};
#[cfg(test)]
use super::sql_inputs::SqlInputKind;
use super::sql_inputs::{
    self, AddSqlInputRequest, AddSqlInputResult, PersistentSqlInput, RemoveSqlInputRequest,
    RemoveSqlInputResult, SqlInputScope,
};
use super::sql_support::{
    RawStatement, execute_preview_statement_raw, sqlite_error, statement_columns, statement_row,
};
use crate::metadata::{self, MetadataKey};
use crate::operations::{self, NewAuditRow, NewOperation};
use crate::{CoreError, CoreResult, Database};

pub const DEFAULT_SQL_RESULT_ROW_LIMIT: usize = 1000;
static CUSTOM_SQL_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum SqlDataSource {
    Csv { alias: String, path: PathBuf },
    Sqlite { alias: String, path: PathBuf },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomTaxonomySqlRequest {
    pub sql: String,
    pub maximum_result_rows: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomTaxonomySqlExportRequest {
    pub sql: String,
    pub destination_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CustomSqlExecutionResult {
    pub operation_id: Option<i64>,
    pub changeset_size: usize,
    pub result_sets: Vec<SqlResultSet>,
    pub messages: Vec<SqlStatementMessage>,
    pub script_saved: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SqlResultSet {
    pub statement_index: usize,
    pub columns: Vec<SqlColumn>,
    pub rows: Vec<Vec<SqlValue>>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SqlColumn {
    pub name: String,
    pub declared_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum SqlValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(String),
}

impl SqlValue {
    fn csv_value(&self) -> String {
        match self {
            Self::Null => String::new(),
            Self::Integer(value) => value.to_string(),
            Self::Real(value) => value.to_string(),
            Self::Text(value) | Self::Blob(value) => value.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SqlStatementMessage {
    pub statement_index: usize,
    pub affected_rows: Option<u64>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SqlExportResult {
    pub path: String,
    pub row_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SqlSourceSchema {
    pub alias: String,
    pub objects: Vec<SqlSourceObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SqlSourceObject {
    pub name: String,
    pub object_type: SqlObjectType,
    pub columns: Vec<SqlColumn>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SqlObjectType {
    Table,
    View,
    VirtualTable,
}

pub fn execute_custom_taxonomy_sql(
    database: &Database,
    request: &CustomTaxonomySqlRequest,
) -> CoreResult<CustomSqlExecutionResult> {
    let sql_mutex = custom_sql_mutex(database)?;
    let _sql_guard = lock_custom_sql(&sql_mutex)?;
    let _guard = database.try_taxonomy_mutation()?;
    let sql = require_sql(&request.sql)?;
    let maximum_result_rows = request
        .maximum_result_rows
        .unwrap_or(DEFAULT_SQL_RESULT_ROW_LIMIT)
        .min(DEFAULT_SQL_RESULT_ROW_LIMIT);
    let mut connection = database.connect_taxonomy()?;
    let sources = sql_inputs::stored_sources(database, SqlInputScope::CustomSql)?;
    let delimiter = crate::general::get_csv_delimiter_byte(database)?;
    let attached = prepare_sources(&mut connection, &sources, delimiter)?;
    let execution: CoreResult<CustomSqlExecutionResult> = (|| {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut session = start_taxonomy_session(&transaction)?;
        let mut result = execute_custom_script(
            &transaction,
            sql,
            maximum_result_rows,
            custom_sql_authorizer,
        )?;
        let mut changeset_blob = Vec::new();
        session.changeset_strm(&mut changeset_blob)?;
        drop(session);
        result.changeset_size = changeset_blob.len();
        if changeset_blob.is_empty() {
            transaction.commit()?;
            return Ok(result);
        }
        validate_taxonomy(&transaction)?;
        let affected_taxon_ids = affected_taxon_ids_from_changeset(&transaction, &changeset_blob)?;
        let full_remap_required = affected_taxon_ids.len() > 5000;
        let operation_id =
            insert_custom_sql_operation(&transaction, &changeset_blob, &affected_taxon_ids)?;
        super::sync::record_event(
            &transaction,
            Some(operation_id),
            affected_taxon_ids,
            full_remap_required,
        )?;
        transaction.commit()?;
        result.operation_id = Some(operation_id);
        Ok(result)
    })();
    let detach = detach_sources(&connection, &attached);
    let mut result = match (execution, detach) {
        (Ok(result), Ok(())) => Ok(result),
        (Err(error), _) => Err(error),
        (Ok(mut result), Err(error)) => {
            result.warnings.push(format!(
                "custom taxonomy SQL committed, but source cleanup failed: {error}"
            ));
            Ok(result)
        }
    }?;
    match database.connect_metadata().and_then(|connection| {
        metadata::set_raw(&connection, MetadataKey::CustomTaxonomySql, &request.sql)
    }) {
        Ok(()) => result.script_saved = true,
        Err(error) => result.warnings.push(format!(
            "custom taxonomy SQL committed, but the script could not be saved: {error}"
        )),
    }
    Ok(result)
}

pub fn export_custom_taxonomy_query(
    database: &Database,
    request: &CustomTaxonomySqlExportRequest,
) -> CoreResult<SqlExportResult> {
    let sql_mutex = custom_sql_mutex(database)?;
    let _sql_guard = lock_custom_sql(&sql_mutex)?;
    let sql = require_sql(&request.sql)?;
    if !request.destination_path.is_absolute() {
        return Err(CoreError::InvalidArgument(
            "sql export destination must be an absolute path".into(),
        ));
    }
    let mut connection = database.connect_taxonomy()?;
    let sources = sql_inputs::stored_sources(database, SqlInputScope::CustomSql)?;
    let delimiter = crate::general::get_csv_delimiter_byte(database)?;
    let attached = prepare_sources(&mut connection, &sources, delimiter)?;
    let export = export_single_query(&connection, sql, &request.destination_path, delimiter);
    let detach = detach_sources(&connection, &attached);
    match (export, detach) {
        (Ok(result), Ok(())) => Ok(result),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

pub(super) fn inspect_sql_data_source(
    source: &SqlDataSource,
    delimiter: u8,
) -> CoreResult<SqlSourceSchema> {
    validate_sources(std::slice::from_ref(source))?;
    match source {
        SqlDataSource::Csv { alias, path } => inspect_csv_source(alias, path, delimiter),
        SqlDataSource::Sqlite { alias, path } => inspect_sqlite_source(alias, path),
    }
}

pub fn get_custom_taxonomy_sql(database: &Database) -> CoreResult<String> {
    Ok(metadata::get_raw(
        &database.connect_metadata()?,
        MetadataKey::CustomTaxonomySql,
    )?
    .unwrap_or_else(|| {
        "SELECT taxon_id, rank, geological_range\nFROM taxa\nORDER BY taxon_id\nLIMIT 100;".into()
    }))
}

pub fn list_custom_sql_inputs(database: &Database) -> CoreResult<Vec<PersistentSqlInput>> {
    sql_inputs::list_inputs(database, SqlInputScope::CustomSql)
}

pub fn list_custom_sql_database_schemas(database: &Database) -> CoreResult<Vec<SqlSourceSchema>> {
    Ok(vec![inspect_sqlite_source(
        "main",
        &database.taxonomy_path()?,
    )?])
}

pub fn add_custom_sql_input(
    database: &Database,
    request: &AddSqlInputRequest,
) -> CoreResult<AddSqlInputResult> {
    let sql_mutex = custom_sql_mutex(database)?;
    let _sql_guard = lock_custom_sql(&sql_mutex)?;
    sql_inputs::add_input(database, SqlInputScope::CustomSql, request)
}

pub fn remove_custom_sql_input(
    database: &Database,
    request: &RemoveSqlInputRequest,
) -> CoreResult<RemoveSqlInputResult> {
    let sql_mutex = custom_sql_mutex(database)?;
    let _sql_guard = lock_custom_sql(&sql_mutex)?;
    sql_inputs::remove_input(database, SqlInputScope::CustomSql, request)
}

fn custom_sql_mutex(database: &Database) -> CoreResult<Arc<Mutex<()>>> {
    let mut locks = CUSTOM_SQL_LOCKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| CoreError::Consistency("Custom SQL lock registry is poisoned".into()))?;
    Ok(locks
        .entry(database.metadata_path())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone())
}

fn lock_custom_sql(mutex: &Mutex<()>) -> CoreResult<std::sync::MutexGuard<'_, ()>> {
    mutex
        .lock()
        .map_err(|_| CoreError::Consistency("Custom SQL workspace lock is poisoned".into()))
}

fn execute_custom_script<F, A>(
    connection: &Connection,
    sql: &str,
    maximum_result_rows: usize,
    mut authorizer_factory: F,
) -> CoreResult<CustomSqlExecutionResult>
where
    F: FnMut() -> A,
    A: for<'a> FnMut(AuthContext<'a>) -> Authorization + Send + 'static,
{
    let mut offset = 0;
    let mut statement_index = 0;
    let mut result_sets = Vec::new();
    let mut messages = Vec::new();
    while offset < sql.len() {
        connection.authorizer(Some(authorizer_factory()));
        let execution = unsafe {
            execute_preview_statement_raw(connection, &sql[offset..], maximum_result_rows)
        };
        connection.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
        let execution = execution?;
        offset += execution.tail_offset;
        let Some(execution) = execution.statement else {
            continue;
        };
        statement_index += 1;
        if !execution.columns.is_empty() {
            result_sets.push(SqlResultSet {
                statement_index,
                columns: execution.columns,
                rows: execution.rows,
                truncated: execution.truncated,
            });
        }
        let affected_rows = if execution.read_only {
            None
        } else {
            Some(execution.affected_rows)
        };
        let message = match affected_rows {
            Some(count) => format!("statement affected {count} rows"),
            None if execution.truncated => {
                format!("statement returned more than {maximum_result_rows} rows")
            }
            None => format!("statement returned {} rows", execution.returned_rows),
        };
        messages.push(SqlStatementMessage {
            statement_index,
            affected_rows,
            message,
        });
    }
    if statement_index != 1 {
        return Err(CoreError::InvalidArgument(
            "custom taxonomy SQL requires exactly one statement".into(),
        ));
    }
    Ok(CustomSqlExecutionResult {
        operation_id: None,
        changeset_size: 0,
        result_sets,
        messages,
        script_saved: false,
        warnings: Vec::new(),
    })
}

fn export_single_query(
    connection: &Connection,
    sql: &str,
    destination_path: &Path,
    delimiter: u8,
) -> CoreResult<SqlExportResult> {
    connection.authorizer(Some(custom_sql_authorizer()));
    let result = unsafe { export_single_query_raw(connection, sql, destination_path, delimiter) };
    connection.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
    result
}

unsafe fn export_single_query_raw(
    connection: &Connection,
    sql: &str,
    destination_path: &Path,
    delimiter: u8,
) -> CoreResult<SqlExportResult> {
    let database = unsafe { connection.handle() };
    let sql = CString::new(sql)
        .map_err(|error| CoreError::InvalidArgument(format!("invalid sql: {error}")))?;
    let mut statement = ptr::null_mut();
    let mut tail = ptr::null();
    let code =
        unsafe { ffi::sqlite3_prepare_v2(database, sql.as_ptr(), -1, &mut statement, &mut tail) };
    if code != ffi::SQLITE_OK {
        return Err(sqlite_error(database, code));
    }
    if statement.is_null() {
        return Err(CoreError::InvalidArgument(
            "sql export requires one query statement".into(),
        ));
    }
    let statement = RawStatement(statement);
    if unsafe { ffi::sqlite3_stmt_readonly(statement.0) } == 0 {
        return Err(CoreError::InvalidArgument(
            "sql export only accepts a read-only query".into(),
        ));
    }
    let column_count = unsafe { ffi::sqlite3_column_count(statement.0) as usize };
    if column_count == 0 {
        return Err(CoreError::InvalidArgument(
            "sql export query has no result columns".into(),
        ));
    }
    let tail = if tail.is_null() {
        ""
    } else {
        unsafe { CStr::from_ptr(tail) }.to_str().map_err(|error| {
            CoreError::InvalidArgument(format!("invalid sql after query: {error}"))
        })?
    };
    if !tail.trim().is_empty() {
        return Err(CoreError::InvalidArgument(
            "sql export accepts exactly one query statement".into(),
        ));
    }
    let file = File::create(destination_path)?;
    let mut writer = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(BufWriter::new(file));
    let columns = unsafe { statement_columns(statement.0, column_count) };
    writer.write_record(columns.iter().map(|column| column.name.as_str()))?;
    let mut row_count = 0_u64;
    loop {
        let step = unsafe { ffi::sqlite3_step(statement.0) };
        match step {
            ffi::SQLITE_ROW => {
                let row = unsafe { statement_row(statement.0, column_count) };
                writer.write_record(row.iter().map(SqlValue::csv_value))?;
                row_count += 1;
            }
            ffi::SQLITE_DONE => break,
            code => return Err(sqlite_error(database, code)),
        }
    }
    writer.flush()?;
    Ok(SqlExportResult {
        path: destination_path.to_string_lossy().into_owned(),
        row_count,
    })
}

pub(super) fn prepare_sources(
    connection: &mut Connection,
    sources: &[SqlDataSource],
    delimiter: u8,
) -> CoreResult<Vec<String>> {
    validate_sources(sources)?;
    let mut attached = Vec::new();
    for source in sources {
        match source {
            SqlDataSource::Csv { alias, path } => {
                load_csv_table(connection, alias, path, delimiter)?;
            }
            SqlDataSource::Sqlite { alias, path } => {
                attach_read_only_sqlite(connection, alias, path)?;
                attached.push(alias.clone());
            }
        }
    }
    Ok(attached)
}

pub(super) fn attach_read_only_sqlite(
    connection: &Connection,
    alias: &str,
    path: &Path,
) -> CoreResult<()> {
    let path = std::fs::canonicalize(path)?;
    let uri = sqlite_read_only_uri(&path);
    connection.execute(
        &format!("ATTACH DATABASE ? AS {}", quote_identifier(alias)),
        [uri],
    )?;
    Ok(())
}

pub(super) fn detach_sources(connection: &Connection, aliases: &[String]) -> CoreResult<()> {
    for alias in aliases.iter().rev() {
        connection.execute_batch(&format!("DETACH DATABASE {}", quote_identifier(alias)))?;
    }
    Ok(())
}

fn load_csv_table(
    connection: &mut Connection,
    alias: &str,
    path: &Path,
    delimiter: u8,
) -> CoreResult<()> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
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
        "CREATE TEMP TABLE {} ({definitions})",
        quote_identifier(alias)
    ))?;
    let column_list = columns
        .iter()
        .map(|column| quote_identifier(column))
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = std::iter::repeat_n("?", columns.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "INSERT INTO temp.{} ({column_list}) VALUES ({placeholders})",
        quote_identifier(alias)
    );
    let mut insert = transaction.prepare(&sql)?;
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

fn validate_sources(sources: &[SqlDataSource]) -> CoreResult<()> {
    let mut aliases = HashSet::new();
    for source in sources {
        let alias = match source {
            SqlDataSource::Csv { alias, path } | SqlDataSource::Sqlite { alias, path } => {
                if !path.is_file() {
                    return Err(CoreError::NotFound(format!(
                        "sql data source {}",
                        path.display()
                    )));
                }
                alias
            }
        };
        if !is_safe_identifier(alias) {
            return Err(CoreError::InvalidArgument(format!(
                "invalid sql data source alias: {alias}"
            )));
        }
        let normalized = alias.to_ascii_lowercase();
        if matches!(
            normalized.as_str(),
            "main"
                | "temp"
                | "base"
                | "metadata"
                | "taxonomy"
                | "taxonomy_base"
                | "active_photo_library"
        ) {
            return Err(CoreError::InvalidArgument(format!(
                "reserved sql data source alias: {alias}"
            )));
        }
        if !aliases.insert(normalized) {
            return Err(CoreError::InvalidArgument(format!(
                "duplicate sql data source alias: {alias}"
            )));
        }
    }
    Ok(())
}

fn inspect_csv_source(alias: &str, path: &Path, delimiter: u8) -> CoreResult<SqlSourceSchema> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .flexible(false)
        .from_path(path)?;
    let columns = validated_columns(reader.headers()?.iter())?
        .into_iter()
        .map(|name| SqlColumn {
            name,
            declared_type: Some("TEXT".into()),
        })
        .collect();
    Ok(SqlSourceSchema {
        alias: alias.into(),
        objects: vec![SqlSourceObject {
            name: alias.into(),
            object_type: SqlObjectType::Table,
            columns,
        }],
    })
}

pub(super) fn inspect_sqlite_source(alias: &str, path: &Path) -> CoreResult<SqlSourceSchema> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI,
    )?;
    let mut statement = connection.prepare(
        r#"
        SELECT name, type, sql
        FROM sqlite_schema
        WHERE type IN ('table', 'view')
          AND name NOT LIKE 'sqlite_%'
        ORDER BY name
        "#,
    )?;
    let objects = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|(name, object_type, sql)| {
            let object_type = if sql.as_deref().is_some_and(|sql| {
                sql.trim_start()
                    .to_ascii_uppercase()
                    .starts_with("CREATE VIRTUAL TABLE")
            }) {
                SqlObjectType::VirtualTable
            } else if object_type == "view" {
                SqlObjectType::View
            } else {
                SqlObjectType::Table
            };
            let columns = inspect_object_columns(&connection, &name)?;
            Ok(SqlSourceObject {
                name,
                object_type,
                columns,
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    Ok(SqlSourceSchema {
        alias: alias.into(),
        objects,
    })
}

fn inspect_object_columns(connection: &Connection, object: &str) -> CoreResult<Vec<SqlColumn>> {
    let mut statement =
        connection.prepare(&format!("PRAGMA table_xinfo({})", quote_identifier(object)))?;
    statement
        .query_map([], |row| {
            let declared_type = row.get::<_, String>(2)?;
            Ok(SqlColumn {
                name: row.get(1)?,
                declared_type: (!declared_type.is_empty()).then_some(declared_type),
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}

pub(super) fn validated_columns<'a>(
    columns: impl Iterator<Item = &'a str>,
) -> CoreResult<Vec<String>> {
    let mut seen = BTreeSet::new();
    let mut output = Vec::new();
    for column in columns {
        if column.trim().is_empty() || column.contains('\0') {
            return Err(CoreError::InvalidArgument(format!(
                "invalid sql source column: {column}"
            )));
        }
        if !seen.insert(column.to_ascii_lowercase()) {
            return Err(CoreError::InvalidArgument(format!(
                "duplicate sql source column: {column}"
            )));
        }
        output.push(column.to_string());
    }
    if output.is_empty() {
        return Err(CoreError::InvalidArgument(
            "sql source requires at least one column".into(),
        ));
    }
    Ok(output)
}

pub(super) fn is_safe_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

pub(super) fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn sqlite_read_only_uri(path: &Path) -> String {
    let path = path.to_string_lossy();
    let encoded = path
        .bytes()
        .flat_map(|byte| match byte {
            b'%' | b'?' | b'#' => {
                let hex = b"0123456789ABCDEF";
                vec![b'%', hex[(byte >> 4) as usize], hex[(byte & 0x0f) as usize]]
            }
            _ => vec![byte],
        })
        .collect::<Vec<_>>();
    format!("file:{}?mode=ro", String::from_utf8_lossy(&encoded))
}

fn require_sql(sql: &str) -> CoreResult<&str> {
    let sql = sql.trim();
    if sql.is_empty() {
        Err(CoreError::InvalidArgument("sql is required".into()))
    } else {
        Ok(sql)
    }
}

fn custom_sql_authorizer() -> impl for<'a> FnMut(AuthContext<'a>) -> Authorization + Send + 'static
{
    let mut business_write = false;
    move |context| match context.action {
        AuthAction::Select | AuthAction::Recursive => Authorization::Allow,
        AuthAction::Function { function_name } => {
            if function_name.eq_ignore_ascii_case("load_extension") {
                Authorization::Deny
            } else {
                Authorization::Allow
            }
        }
        AuthAction::Pragma {
            pragma_name,
            pragma_value,
        } => {
            let pragma_name = pragma_name.to_ascii_lowercase();
            let read_only = matches!(
                pragma_name.as_str(),
                "foreign_key_list"
                    | "index_info"
                    | "index_list"
                    | "index_xinfo"
                    | "table_info"
                    | "table_xinfo"
            ) || (pragma_value.is_none()
                && matches!(
                    pragma_name.as_str(),
                    "data_version"
                        | "database_list"
                        | "function_list"
                        | "module_list"
                        | "table_list"
                ));
            if read_only {
                Authorization::Allow
            } else {
                Authorization::Deny
            }
        }
        AuthAction::Read { .. } => Authorization::Allow,
        AuthAction::Insert { table_name }
        | AuthAction::Update { table_name, .. }
        | AuthAction::Delete { table_name } => {
            let direct_business_write = context.database_name == Some("main")
                && context.accessor.is_none()
                && matches!(table_name, "taxa" | "taxon_names");
            let derived_write = context.database_name == Some("main")
                && (context.accessor.is_some() || business_write)
                && table_name.starts_with("taxon_names_fts");
            if direct_business_write {
                business_write = true;
                Authorization::Allow
            } else if derived_write {
                Authorization::Allow
            } else {
                Authorization::Deny
            }
        }
        _ => Authorization::Deny,
    }
}

fn insert_custom_sql_operation(
    transaction: &Transaction<'_>,
    changeset_blob: &[u8],
    affected_taxon_ids: &BTreeSet<i64>,
) -> CoreResult<i64> {
    let total_items = affected_taxon_ids.len().max(1);
    let operation_id = operations::insert_operation(
        transaction,
        NewOperation {
            kind: "taxonomy_custom_sql",
            source: "custom_sql",
            total_items,
            succeeded_items: total_items,
            failed_items: 0,
            rollbackable: true,
            has_formatted_input: false,
        },
    )?;
    transaction.execute(
        r#"
        INSERT INTO operation_changesets (operation_id, changeset_blob)
        VALUES (?, ?)
        "#,
        params![operation_id, changeset_blob],
    )?;
    if affected_taxon_ids.is_empty() {
        operations::insert_audit_row(
            transaction,
            operation_id,
            NewAuditRow {
                sequence: 1,
                entity_type: "taxonomy",
                entity_id: None,
                action: "custom_sql",
                before_json: None,
                after_json: Some(serde_json::json!({
                    "changeset_size": changeset_blob.len(),
                })),
                succeeded: true,
                message: "custom SQL changed taxonomy data",
            },
        )?;
    } else {
        for (index, taxon_id) in affected_taxon_ids.iter().enumerate() {
            operations::insert_audit_row(
                transaction,
                operation_id,
                NewAuditRow {
                    sequence: index + 1,
                    entity_type: "taxon",
                    entity_id: Some(taxon_id.to_string()),
                    action: "custom_sql",
                    before_json: None,
                    after_json: Some(serde_json::json!({
                        "changeset_size": changeset_blob.len(),
                    })),
                    succeeded: true,
                    message: "custom SQL changed taxonomy data",
                },
            )?;
        }
    }
    Ok(operation_id)
}

#[cfg(test)]
#[path = "sql/tests.rs"]
mod tests;
