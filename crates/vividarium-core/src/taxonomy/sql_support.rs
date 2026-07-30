use std::ffi::{CStr, CString};
use std::ptr;

use base64::Engine;
use rusqlite::{Connection, ffi};

use super::sql::{SqlColumn, SqlValue};
use crate::{CoreError, CoreResult};

pub(super) struct RawScriptStep {
    pub(super) tail_offset: usize,
    pub(super) statement: Option<RawStatementExecution>,
}

pub(super) struct RawStatementExecution {
    pub(super) columns: Vec<SqlColumn>,
    pub(super) rows: Vec<Vec<SqlValue>>,
    pub(super) returned_rows: u64,
    pub(super) truncated: bool,
    pub(super) read_only: bool,
    pub(super) affected_rows: u64,
}

pub(super) unsafe fn execute_preview_statement_raw(
    connection: &Connection,
    sql_tail: &str,
    maximum_result_rows: usize,
) -> CoreResult<RawScriptStep> {
    unsafe { execute_one_statement_raw(connection, sql_tail, maximum_result_rows, true) }
}

pub(super) unsafe fn execute_statement_to_completion_raw(
    connection: &Connection,
    sql_tail: &str,
) -> CoreResult<RawScriptStep> {
    unsafe { execute_one_statement_raw(connection, sql_tail, 0, false) }
}

unsafe fn execute_one_statement_raw(
    connection: &Connection,
    sql_tail: &str,
    maximum_result_rows: usize,
    stop_read_only_after_preview: bool,
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
    let mut returned_rows = 0_u64;
    let mut truncated = false;
    loop {
        let step = unsafe { ffi::sqlite3_step(statement.0) };
        match step {
            ffi::SQLITE_ROW => {
                returned_rows += 1;
                if rows.len() < maximum_result_rows {
                    rows.push(unsafe { statement_row(statement.0, column_count) });
                } else {
                    truncated = true;
                    if read_only && stop_read_only_after_preview {
                        break;
                    }
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
            returned_rows,
            truncated,
            read_only,
            affected_rows: unsafe { ffi::sqlite3_changes64(database) }.max(0) as u64,
        }),
    })
}

pub(super) unsafe fn statement_columns(
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

pub(super) unsafe fn statement_row(
    statement: *mut ffi::sqlite3_stmt,
    column_count: usize,
) -> Vec<SqlValue> {
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

pub(super) struct RawStatement(pub(super) *mut ffi::sqlite3_stmt);

impl Drop for RawStatement {
    fn drop(&mut self) {
        unsafe {
            ffi::sqlite3_finalize(self.0);
        }
    }
}

pub(super) fn sqlite_error(database: *mut ffi::sqlite3, code: i32) -> CoreError {
    let message = unsafe { CStr::from_ptr(ffi::sqlite3_errmsg(database)) }
        .to_string_lossy()
        .into_owned();
    CoreError::Database(rusqlite::Error::SqliteFailure(
        ffi::Error::new(code),
        Some(message),
    ))
}
