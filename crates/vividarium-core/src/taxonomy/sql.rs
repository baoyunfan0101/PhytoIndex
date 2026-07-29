use std::collections::{BTreeSet, HashSet};
use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::ptr;

use base64::Engine;
use rusqlite::ffi;
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::{Connection, OpenFlags, Transaction, TransactionBehavior, params, params_from_iter};
use serde::{Deserialize, Serialize};

use super::formatted::{
    affected_taxon_ids_from_changeset, start_taxonomy_session, validate_taxonomy,
};
use crate::operations::{self, NewAuditRow, NewOperation};
use crate::{CoreError, CoreResult, Database};

pub const DEFAULT_SQL_RESULT_ROW_LIMIT: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SqlDataSource {
    Csv { alias: String, path: PathBuf },
    Sqlite { alias: String, path: PathBuf },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomTaxonomySqlRequest {
    pub sql: String,
    #[serde(default)]
    pub sources: Vec<SqlDataSource>,
    pub maximum_result_rows: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomTaxonomySqlExportRequest {
    pub sql: String,
    #[serde(default)]
    pub sources: Vec<SqlDataSource>,
    pub destination_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SqlExecutionResult {
    pub operation_id: Option<i64>,
    pub changeset_size: usize,
    pub result_sets: Vec<SqlResultSet>,
    pub messages: Vec<SqlStatementMessage>,
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
) -> CoreResult<SqlExecutionResult> {
    let sql = require_sql(&request.sql)?;
    let maximum_result_rows = request
        .maximum_result_rows
        .unwrap_or(DEFAULT_SQL_RESULT_ROW_LIMIT)
        .min(DEFAULT_SQL_RESULT_ROW_LIMIT);
    let mut connection = database.connect_taxonomy()?;
    let attached = prepare_sources(&connection, &request.sources)?;
    let execution = (|| {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut session = start_taxonomy_session(&transaction)?;
        let mut result = execute_script(
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
    match (execution, detach) {
        (Ok(result), Ok(())) => Ok(result),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

pub fn export_custom_taxonomy_query(
    database: &Database,
    request: &CustomTaxonomySqlExportRequest,
) -> CoreResult<SqlExportResult> {
    let sql = require_sql(&request.sql)?;
    if !request.destination_path.is_absolute() {
        return Err(CoreError::InvalidArgument(
            "sql export destination must be an absolute path".into(),
        ));
    }
    let connection = database.connect_taxonomy()?;
    let attached = prepare_sources(&connection, &request.sources)?;
    let export = export_single_query(&connection, sql, &request.destination_path);
    let detach = detach_sources(&connection, &attached);
    match (export, detach) {
        (Ok(result), Ok(())) => Ok(result),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

pub fn inspect_sql_data_source(source: &SqlDataSource) -> CoreResult<SqlSourceSchema> {
    validate_sources(std::slice::from_ref(source))?;
    match source {
        SqlDataSource::Csv { alias, path } => inspect_csv_source(alias, path),
        SqlDataSource::Sqlite { alias, path } => inspect_sqlite_source(alias, path),
    }
}

pub(super) fn execute_script<F, A>(
    connection: &Connection,
    sql: &str,
    maximum_result_rows: usize,
    mut authorizer_factory: F,
) -> CoreResult<SqlExecutionResult>
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
        let execution =
            unsafe { execute_one_statement_raw(connection, &sql[offset..], maximum_result_rows) };
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
                truncated: execution.total_rows > maximum_result_rows as u64,
            });
        }
        let affected_rows = if execution.read_only {
            None
        } else {
            Some(execution.affected_rows)
        };
        let message = match affected_rows {
            Some(count) => format!("statement affected {count} rows"),
            None => format!("statement returned {} rows", execution.total_rows),
        };
        messages.push(SqlStatementMessage {
            statement_index,
            affected_rows,
            message,
        });
    }
    Ok(SqlExecutionResult {
        operation_id: None,
        changeset_size: 0,
        result_sets,
        messages,
    })
}

struct RawScriptStep {
    tail_offset: usize,
    statement: Option<RawStatementExecution>,
}

struct RawStatementExecution {
    columns: Vec<SqlColumn>,
    rows: Vec<Vec<SqlValue>>,
    total_rows: u64,
    read_only: bool,
    affected_rows: u64,
}

unsafe fn execute_one_statement_raw(
    connection: &Connection,
    sql_tail: &str,
    maximum_result_rows: usize,
) -> CoreResult<RawScriptStep> {
    let database = unsafe { connection.handle() };
    let sql_tail = CString::new(sql_tail)
        .map_err(|error| CoreError::InvalidArgument(format!("invalid sql: {error}")))?;
    let mut raw_statement = ptr::null_mut();
    let mut next_sql = ptr::null();
    let code = unsafe {
        ffi::sqlite3_prepare_v2(
            database,
            sql_tail.as_ptr(),
            -1,
            &mut raw_statement,
            &mut next_sql,
        )
    };
    if code != ffi::SQLITE_OK {
        return Err(sqlite_error(database, code));
    }
    let tail_offset = if next_sql.is_null() {
        sql_tail.as_bytes().len()
    } else {
        unsafe { next_sql.offset_from(sql_tail.as_ptr()) as usize }
    };
    if tail_offset == 0 {
        return Err(CoreError::InvalidArgument(
            "sql parser did not advance".into(),
        ));
    }
    if raw_statement.is_null() {
        return Ok(RawScriptStep {
            tail_offset,
            statement: None,
        });
    }
    let statement = RawStatement(raw_statement);
    let column_count = unsafe { ffi::sqlite3_column_count(statement.0) as usize };
    let columns = unsafe { statement_columns(statement.0, column_count) };
    let read_only = unsafe { ffi::sqlite3_stmt_readonly(statement.0) != 0 };
    let mut rows = Vec::new();
    let mut total_rows = 0_u64;
    loop {
        let step = unsafe { ffi::sqlite3_step(statement.0) };
        match step {
            ffi::SQLITE_ROW => {
                total_rows += 1;
                if rows.len() < maximum_result_rows {
                    rows.push(unsafe { statement_row(statement.0, column_count) });
                }
            }
            ffi::SQLITE_DONE => break,
            code => return Err(sqlite_error(database, code)),
        }
    }
    Ok(RawScriptStep {
        tail_offset,
        statement: Some(RawStatementExecution {
            columns,
            rows,
            total_rows,
            read_only,
            affected_rows: unsafe { ffi::sqlite3_changes64(database) }.max(0) as u64,
        }),
    })
}

unsafe fn statement_columns(
    statement: *mut ffi::sqlite3_stmt,
    column_count: usize,
) -> Vec<SqlColumn> {
    (0..column_count)
        .map(|index| {
            let index = index as i32;
            SqlColumn {
                name: unsafe { sqlite_text(ffi::sqlite3_column_name(statement, index)) }
                    .unwrap_or_default(),
                declared_type: unsafe {
                    sqlite_text(ffi::sqlite3_column_decltype(statement, index))
                },
            }
        })
        .collect()
}

unsafe fn statement_row(statement: *mut ffi::sqlite3_stmt, column_count: usize) -> Vec<SqlValue> {
    (0..column_count)
        .map(|index| unsafe { statement_value(statement, index as i32) })
        .collect()
}

unsafe fn statement_value(statement: *mut ffi::sqlite3_stmt, index: i32) -> SqlValue {
    match unsafe { ffi::sqlite3_column_type(statement, index) } {
        ffi::SQLITE_NULL => SqlValue::Null,
        ffi::SQLITE_INTEGER => {
            SqlValue::Integer(unsafe { ffi::sqlite3_column_int64(statement, index) })
        }
        ffi::SQLITE_FLOAT => {
            SqlValue::Real(unsafe { ffi::sqlite3_column_double(statement, index) })
        }
        ffi::SQLITE_TEXT => {
            let pointer = unsafe { ffi::sqlite3_column_text(statement, index) };
            let length = unsafe { ffi::sqlite3_column_bytes(statement, index) }.max(0) as usize;
            let bytes = if pointer.is_null() {
                &[]
            } else {
                unsafe { std::slice::from_raw_parts(pointer, length) }
            };
            SqlValue::Text(String::from_utf8_lossy(bytes).into_owned())
        }
        ffi::SQLITE_BLOB => {
            let pointer = unsafe { ffi::sqlite3_column_blob(statement, index) };
            let length = unsafe { ffi::sqlite3_column_bytes(statement, index) }.max(0) as usize;
            let bytes = if pointer.is_null() {
                &[]
            } else {
                unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), length) }
            };
            SqlValue::Blob(base64::engine::general_purpose::STANDARD.encode(bytes))
        }
        _ => SqlValue::Null,
    }
}

unsafe fn sqlite_text(pointer: *const std::ffi::c_char) -> Option<String> {
    (!pointer.is_null()).then(|| {
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    })
}

struct RawStatement(*mut ffi::sqlite3_stmt);

impl Drop for RawStatement {
    fn drop(&mut self) {
        unsafe {
            ffi::sqlite3_finalize(self.0);
        }
    }
}

fn export_single_query(
    connection: &Connection,
    sql: &str,
    destination_path: &Path,
) -> CoreResult<SqlExportResult> {
    connection.authorizer(Some(custom_sql_authorizer()));
    let result = unsafe { export_single_query_raw(connection, sql, destination_path) };
    connection.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
    result
}

unsafe fn export_single_query_raw(
    connection: &Connection,
    sql: &str,
    destination_path: &Path,
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
    let mut writer = csv::Writer::from_writer(BufWriter::new(file));
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

fn prepare_sources(connection: &Connection, sources: &[SqlDataSource]) -> CoreResult<Vec<String>> {
    validate_sources(sources)?;
    let mut attached = Vec::new();
    for source in sources {
        match source {
            SqlDataSource::Csv { alias, path } => {
                load_csv_table(connection, alias, path)?;
            }
            SqlDataSource::Sqlite { alias, path } => {
                let path = std::fs::canonicalize(path)?;
                let uri = sqlite_read_only_uri(&path);
                connection.execute(
                    &format!("ATTACH DATABASE ? AS {}", quote_identifier(alias)),
                    [uri],
                )?;
                attached.push(alias.clone());
            }
        }
    }
    Ok(attached)
}

fn detach_sources(connection: &Connection, aliases: &[String]) -> CoreResult<()> {
    for alias in aliases.iter().rev() {
        connection.execute_batch(&format!("DETACH DATABASE {}", quote_identifier(alias)))?;
    }
    Ok(())
}

fn load_csv_table(connection: &Connection, alias: &str, path: &Path) -> CoreResult<()> {
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
    connection.execute_batch(&format!(
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
    let mut insert = connection.prepare(&sql)?;
    for record in reader.records() {
        let record = record?;
        insert.execute(params_from_iter(record.iter()))?;
    }
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

fn inspect_csv_source(alias: &str, path: &Path) -> CoreResult<SqlSourceSchema> {
    let mut reader = csv::ReaderBuilder::new()
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

fn sqlite_error(database: *mut ffi::sqlite3, code: i32) -> CoreError {
    let message = unsafe { CStr::from_ptr(ffi::sqlite3_errmsg(database)) }
        .to_string_lossy()
        .into_owned();
    CoreError::Database(rusqlite::Error::SqliteFailure(
        ffi::Error::new(code),
        Some(message),
    ))
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::*;
    use crate::taxonomy::{TaxonInputRow, apply_rows, list_operations};

    fn database() -> (tempfile::TempDir, Database) {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("metadata.db")).unwrap();
        apply_rows(
            &database,
            &[TaxonInputRow {
                kingdom: Some("Animalia".into()),
                ..TaxonInputRow::default()
            }],
        )
        .unwrap();
        (directory, database)
    }

    fn request(sql: &str) -> CustomTaxonomySqlRequest {
        CustomTaxonomySqlRequest {
            sql: sql.into(),
            sources: Vec::new(),
            maximum_result_rows: None,
        }
    }

    #[test]
    fn returns_typed_results_and_only_logs_actual_mutations() {
        let (_directory, database) = database();
        let before = list_operations(&database, None, 20).unwrap().items.len();
        let query = execute_custom_taxonomy_sql(
            &database,
            &request(
                "SELECT taxon_id, rank, geological_range, CAST(NULL AS TEXT) AS missing FROM taxa",
            ),
        )
        .unwrap();
        assert_eq!(query.operation_id, None);
        assert_eq!(query.changeset_size, 0);
        assert_eq!(query.result_sets.len(), 1);
        assert_eq!(query.result_sets[0].rows[0][1], SqlValue::Integer(1));
        assert_eq!(query.result_sets[0].rows[0][2], SqlValue::Null);
        assert_eq!(
            list_operations(&database, None, 20).unwrap().items.len(),
            before
        );

        let mutation = execute_custom_taxonomy_sql(
            &database,
            &request(
                r#"
                UPDATE taxa SET geological_range = 'Recent';
                SELECT geological_range FROM taxa;
                "#,
            ),
        )
        .unwrap();
        assert!(mutation.operation_id.is_some());
        assert!(mutation.changeset_size > 0);
        assert_eq!(
            mutation.result_sets[0].rows,
            vec![vec![SqlValue::Text("Recent".into())]]
        );
        assert_eq!(mutation.messages[0].affected_rows, Some(1));
        assert_eq!(mutation.messages[1].affected_rows, None);
        assert_eq!(
            list_operations(&database, None, 20).unwrap().items.len(),
            before + 1
        );
    }

    #[test]
    fn truncates_returned_rows_without_stopping_statement_execution() {
        let (_directory, database) = database();
        let mut limited = request(
            r#"
            WITH RECURSIVE values_cte(value) AS (
                VALUES (1)
                UNION ALL
                SELECT value + 1 FROM values_cte WHERE value < 5
            )
            SELECT value FROM values_cte ORDER BY value;
            "#,
        );
        limited.maximum_result_rows = Some(2);

        let result = execute_custom_taxonomy_sql(&database, &limited).unwrap();

        assert_eq!(result.result_sets[0].rows.len(), 2);
        assert!(result.result_sets[0].truncated);
        assert_eq!(result.messages[0].message, "statement returned 5 rows");
    }

    #[test]
    fn csv_and_sqlite_sources_are_read_only() {
        let (directory, database) = database();
        let csv_path = directory.path().join("input.csv");
        std::fs::write(&csv_path, "name,value\nAnimalia,\n").unwrap();
        let sqlite_path = directory.path().join("source.db");
        let source = Connection::open(&sqlite_path).unwrap();
        source
            .execute_batch("CREATE TABLE source_names(name TEXT); INSERT INTO source_names VALUES ('Metazoa');")
            .unwrap();
        drop(source);
        let sources = vec![
            SqlDataSource::Csv {
                alias: "csv_input".into(),
                path: csv_path.clone(),
            },
            SqlDataSource::Sqlite {
                alias: "external".into(),
                path: sqlite_path.clone(),
            },
        ];
        let result = execute_custom_taxonomy_sql(
            &database,
            &CustomTaxonomySqlRequest {
                sql: "SELECT csv_input.name, csv_input.value, external.source_names.name FROM csv_input CROSS JOIN external.source_names".into(),
                sources: sources.clone(),
                maximum_result_rows: None,
            },
        )
        .unwrap();
        assert_eq!(
            result.result_sets[0].rows[0],
            vec![
                SqlValue::Text("Animalia".into()),
                SqlValue::Text(String::new()),
                SqlValue::Text("Metazoa".into())
            ]
        );
        let error = execute_custom_taxonomy_sql(
            &database,
            &CustomTaxonomySqlRequest {
                sql: "DELETE FROM external.source_names".into(),
                sources,
                maximum_result_rows: None,
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("not authorized"));
        assert_eq!(
            Connection::open(&sqlite_path)
                .unwrap()
                .query_row("SELECT COUNT(*) FROM source_names", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        let schema = inspect_sql_data_source(&SqlDataSource::Csv {
            alias: "csv_input".into(),
            path: csv_path,
        })
        .unwrap();
        assert_eq!(schema.objects[0].columns[0].name, "name");
    }

    #[test]
    fn rejects_control_and_internal_write_statements() {
        let (_directory, database) = database();
        for sql in [
            "BEGIN",
            "ATTACH DATABASE 'other.db' AS other",
            "DELETE FROM operations",
            "DROP TABLE taxa",
            "PRAGMA writable_schema = ON",
        ] {
            assert!(
                execute_custom_taxonomy_sql(&database, &request(sql))
                    .unwrap_err()
                    .to_string()
                    .contains("not authorized"),
                "{sql}"
            );
        }
    }

    #[test]
    fn streams_one_query_to_csv() {
        let (directory, database) = database();
        let destination = directory.path().join("query.csv");
        let result = export_custom_taxonomy_query(
            &database,
            &CustomTaxonomySqlExportRequest {
                sql: "SELECT rank, name FROM taxa JOIN taxon_names USING (taxon_id)".into(),
                sources: Vec::new(),
                destination_path: destination.clone(),
            },
        )
        .unwrap();
        assert_eq!(result.row_count, 1);
        assert_eq!(
            std::fs::read_to_string(destination).unwrap(),
            "rank,name\n1,Animalia\n"
        );
    }
}
