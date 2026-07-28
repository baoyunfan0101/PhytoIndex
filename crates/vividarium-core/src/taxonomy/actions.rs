use std::collections::BTreeSet;
use std::ffi::{CStr, CString};
use std::ptr;

use rusqlite::ffi;
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params, params_from_iter};
use serde::{Deserialize, Serialize};

use super::formatted::{
    affected_taxon_ids_from_changeset, is_taxonomy_session_table, start_taxonomy_session,
    validate_taxonomy,
};
use super::view::load_taxon_summary;
use super::{
    TaxonInputRow, TaxonRank, TaxonRowStatus, TaxonomyNameType, TaxonomyOperationResult,
    apply_rows, preview_rows,
};
use crate::operations::{self, NewAuditRow, NewOperation};
use crate::{CoreError, CoreResult, Database};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonUpdateInput {
    pub taxon_id: i64,
    pub authority_year: Option<String>,
    pub synonyms: Vec<String>,
    pub zh_name: Option<String>,
    pub zh_alias: Vec<String>,
    pub en_name: Option<String>,
    pub en_alias: Vec<String>,
    pub geological_range: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteTaxonNameInput {
    pub taxon_id: i64,
    pub name_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromoteTaxonNameInput {
    pub taxon_id: i64,
    pub name_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonomyCustomSqlResult {
    pub operation_id: i64,
    pub changeset_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonomyCustomSqlTempTable {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

pub fn parse_custom_taxonomy_input_csv(input: &str) -> CoreResult<TaxonomyCustomSqlTempTable> {
    let input = input.trim_start_matches('\u{feff}');
    if input.trim().is_empty() {
        return Err(CoreError::InvalidArgument(
            "custom sql input csv is empty".into(),
        ));
    }
    let delimiter = detect_csv_delimiter(input);
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .flexible(false)
        .from_reader(input.as_bytes());
    let columns = reader
        .headers()
        .map_err(|error| {
            CoreError::InvalidArgument(format!("invalid custom sql input csv: {error}"))
        })?
        .iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if columns.is_empty() {
        return Err(CoreError::InvalidArgument(
            "custom sql input csv requires a header".into(),
        ));
    }
    let rows = reader
        .records()
        .map(|record| {
            record
                .map(|record| record.iter().map(str::to_string).collect::<Vec<_>>())
                .map_err(|error| {
                    CoreError::InvalidArgument(format!("invalid custom sql input csv: {error}"))
                })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    Ok(TaxonomyCustomSqlTempTable { columns, rows })
}

fn detect_csv_delimiter(input: &str) -> u8 {
    let candidates = *b",|\t;";
    let mut counts = [0_usize; 4];
    let bytes = input.as_bytes();
    let mut in_quotes = false;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'"' {
            if in_quotes && bytes.get(index + 1) == Some(&b'"') {
                index += 2;
                continue;
            }
            in_quotes = !in_quotes;
        } else if !in_quotes && matches!(byte, b'\r' | b'\n') {
            break;
        } else if !in_quotes
            && let Some(candidate) = candidates.iter().position(|value| *value == byte)
        {
            counts[candidate] += 1;
        }
        index += 1;
    }
    counts
        .iter()
        .enumerate()
        .max_by_key(|(_, count)| *count)
        .filter(|(_, count)| **count > 0)
        .map(|(index, _)| candidates[index])
        .unwrap_or(b',')
}

pub fn update_taxon(
    database: &Database,
    input: TaxonUpdateInput,
) -> CoreResult<TaxonomyOperationResult> {
    let connection = database.connect_taxonomy_context()?;
    let summary = load_taxon_summary(&connection, input.taxon_id)?
        .ok_or_else(|| CoreError::NotFound(format!("taxon {}", input.taxon_id)))?;
    drop(connection);
    let mut row = TaxonInputRow {
        selected_taxon_id: Some(input.taxon_id),
        authority_year: input.authority_year,
        synonyms: input.synonyms,
        zh_name: input.zh_name,
        zh_alias: input.zh_alias,
        en_name: input.en_name,
        en_alias: input.en_alias,
        geological_range: input.geological_range,
        source: input.source,
        ..TaxonInputRow::default()
    };
    for item in &summary.breadcrumb {
        set_rank_locator(&mut row, item.rank, item.names.sci_name.clone())?;
    }
    set_rank_locator(&mut row, summary.rank, summary.names.sci_name)?;
    let preview = preview_rows(database, std::slice::from_ref(&row))?;
    if preview.rows[0].operation_types.iter().any(|value| {
        matches!(
            value,
            TaxonRowStatus::Invalid
                | TaxonRowStatus::NotMatched
                | TaxonRowStatus::MultipleCandidates
        )
    }) {
        return Err(CoreError::InvalidArgument(preview.rows[0].message.clone()));
    }
    apply_rows(database, &[row])
}

pub fn promote_taxon_name(database: &Database, input: PromoteTaxonNameInput) -> CoreResult<()> {
    let mut connection = database.connect_taxonomy_context()?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (rank, current_type, name) = transaction
        .query_row(
            r#"
            SELECT taxa.rank, taxon_names.name_type, taxon_names.name
            FROM taxa JOIN taxon_names USING (taxon_id)
            WHERE taxa.taxon_id = ? AND taxon_names.name_id = ?
            "#,
            params![input.taxon_id, input.name_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::NotFound(format!(
                "name {} for taxon {}",
                input.name_id, input.taxon_id
            ))
        })?;
    let current_type = TaxonomyNameType::from_code(current_type)?;
    if current_type.is_primary() {
        return Err(CoreError::InvalidArgument(
            "the selected name is already accepted".into(),
        ));
    }
    let accepted_type = current_type.accepted_type();
    let (accepted_name_id, accepted_name) = transaction
        .query_row(
            "SELECT name_id, name FROM taxon_names WHERE taxon_id = ? AND name_type = ?",
            params![input.taxon_id, accepted_type.code()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::InvalidArgument(format!(
                "taxon {} has no {} to exchange",
                input.taxon_id,
                accepted_type.as_str()
            ))
        })?;
    if current_type == TaxonomyNameType::Synonym
        && TaxonRank::from_code(rank)? == TaxonRank::Species
    {
        let genus_name: Option<String> = transaction
            .query_row(
                r#"
                SELECT name
                FROM taxa AS species
                JOIN taxon_names
                  ON taxon_names.taxon_id = species.parent_taxon_id
                 AND taxon_names.name_type = 1
                WHERE species.taxon_id = ?
                "#,
                [input.taxon_id],
                |row| row.get(0),
            )
            .optional()?;
        let first_word = name.split_whitespace().next();
        if genus_name.as_deref() != first_word {
            return Err(CoreError::InvalidArgument(format!(
                "species scientific name '{}' does not start with parent genus '{}'",
                name,
                genus_name.unwrap_or_default()
            )));
        }
    }
    let mut session = start_taxonomy_session(&transaction)?;
    transaction.execute(
        "UPDATE taxon_names SET name_type = ? WHERE taxon_id = ? AND name_id = ?",
        params![current_type.code(), input.taxon_id, accepted_name_id],
    )?;
    transaction.execute(
        "UPDATE taxon_names SET name_type = ? WHERE taxon_id = ? AND name_id = ?",
        params![accepted_type.code(), input.taxon_id, input.name_id],
    )?;
    validate_taxonomy(&transaction)?;
    let mut changeset_blob = Vec::new();
    session.changeset_strm(&mut changeset_blob)?;
    drop(session);
    let operation_id = operations::insert_operation(
        &transaction,
        NewOperation {
            kind: "taxonomy_name_promote",
            source: "ui_promote",
            total_items: 1,
            succeeded_items: 1,
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
    operations::insert_audit_row(
        &transaction,
        operation_id,
        NewAuditRow {
            sequence: 1,
            entity_type: "taxon_name",
            entity_id: Some(input.name_id.to_string()),
            action: "promote",
            before_json: Some(serde_json::json!({
                "taxon_id": input.taxon_id,
                "accepted_name_id": accepted_name_id,
                "accepted_name": accepted_name.clone(),
                "promoted_name_id": input.name_id,
                "promoted_name": name.clone(),
                "accepted_name_type": accepted_type.code(),
                "alias_name_type": current_type.code(),
            })),
            after_json: Some(serde_json::json!({
                "taxon_id": input.taxon_id,
                "accepted_name_id": input.name_id,
                "accepted_name": name,
                "alias_name_id": accepted_name_id,
                "alias_name": accepted_name,
                "accepted_name_type": accepted_type.code(),
                "alias_name_type": current_type.code(),
            })),
            succeeded: true,
            message: "exchanged accepted and alias taxonomy names",
        },
    )?;
    super::sync::record_event(&transaction, Some(operation_id), [input.taxon_id], false)?;
    transaction.commit()?;
    Ok(())
}

pub fn delete_taxon_name(database: &Database, input: DeleteTaxonNameInput) -> CoreResult<()> {
    let mut connection = database.connect_taxonomy_context()?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (name_type, name) = transaction
        .query_row(
            "SELECT name_type, name FROM taxon_names WHERE taxon_id = ? AND name_id = ?",
            params![input.taxon_id, input.name_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::NotFound(format!(
                "name {} for taxon {}",
                input.name_id, input.taxon_id
            ))
        })?;
    if TaxonomyNameType::from_code(name_type)? == TaxonomyNameType::SciName {
        return Err(CoreError::InvalidArgument(
            "the unique sci_name cannot be deleted".into(),
        ));
    }
    let mut session = start_taxonomy_session(&transaction)?;
    let deleted = transaction.execute(
        "DELETE FROM taxon_names WHERE taxon_id = ? AND name_id = ?",
        params![input.taxon_id, input.name_id],
    )?;
    if deleted == 0 {
        return Err(CoreError::NotFound(format!(
            "name {} ('{}') for taxon {}",
            input.name_id, name, input.taxon_id
        )));
    }
    validate_taxonomy(&transaction)?;
    let mut changeset_blob = Vec::new();
    session.changeset_strm(&mut changeset_blob)?;
    drop(session);
    let operation_id = operations::insert_operation(
        &transaction,
        NewOperation {
            kind: "taxonomy_delete",
            source: "ui_delete",
            total_items: 1,
            succeeded_items: 1,
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
    operations::insert_audit_row(
        &transaction,
        operation_id,
        NewAuditRow {
            sequence: 1,
            entity_type: "taxon_name",
            entity_id: Some(input.name_id.to_string()),
            action: "delete",
            before_json: Some(serde_json::json!({
                "taxon_id": input.taxon_id,
                "name_id": input.name_id,
                "name_type": name_type,
                "name": name,
            })),
            after_json: None,
            succeeded: true,
            message: "deleted taxonomy name",
        },
    )?;
    super::sync::record_event(&transaction, Some(operation_id), [input.taxon_id], false)?;
    transaction.commit()?;
    Ok(())
}

pub fn delete_taxon(database: &Database, taxon_id: i64) -> CoreResult<()> {
    let mut connection = database.connect_taxonomy_context()?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let taxon = transaction
        .query_row(
            r#"
        SELECT parent_taxon_id, rank, geological_range
        FROM taxa
        WHERE taxon_id = ?
        "#,
            [taxon_id],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound(format!("taxon {taxon_id}")))?;
    let child_count: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM taxa WHERE parent_taxon_id = ?",
        [taxon_id],
        |row| row.get(0),
    )?;
    if child_count > 0 {
        return Err(CoreError::InvalidArgument(format!(
            "taxon {taxon_id} cannot be deleted because it has child taxa"
        )));
    }
    let mut session = start_taxonomy_session(&transaction)?;
    transaction.execute("DELETE FROM taxa WHERE taxon_id = ?", [taxon_id])?;
    let mut changeset_blob = Vec::new();
    session.changeset_strm(&mut changeset_blob)?;
    drop(session);
    let operation_id = operations::insert_operation(
        &transaction,
        NewOperation {
            kind: "taxonomy_delete",
            source: "ui_delete",
            total_items: 1,
            succeeded_items: 1,
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
    operations::insert_audit_row(
        &transaction,
        operation_id,
        NewAuditRow {
            sequence: 1,
            entity_type: "taxon",
            entity_id: Some(taxon_id.to_string()),
            action: "delete",
            before_json: Some(serde_json::json!({
                "taxon_id": taxon_id,
                "parent_taxon_id": taxon.0,
                "rank": taxon.1,
                "geological_range": taxon.2,
            })),
            after_json: None,
            succeeded: true,
            message: "deleted taxon",
        },
    )?;
    super::sync::record_event(&transaction, Some(operation_id), [taxon_id], false)?;
    transaction.commit()?;
    Ok(())
}

fn set_rank_locator(
    row: &mut TaxonInputRow,
    rank: TaxonRank,
    scientific_name: Option<String>,
) -> CoreResult<()> {
    let name = scientific_name.ok_or_else(|| {
        CoreError::InvalidArgument(format!("{} taxon has no sci_name", rank.as_str()))
    })?;
    match rank {
        TaxonRank::Kingdom => row.kingdom = Some(name),
        TaxonRank::Order => row.order = Some(name),
        TaxonRank::Family => row.family = Some(name),
        TaxonRank::Genus => row.genus = Some(name),
        TaxonRank::Species => row.species = Some(name),
    }
    Ok(())
}

pub fn execute_custom_taxonomy_sql(
    database: &Database,
    sql: &str,
    input: Option<TaxonomyCustomSqlTempTable>,
) -> CoreResult<TaxonomyCustomSqlResult> {
    let sql = sql.trim();
    if sql.is_empty() {
        return Err(CoreError::InvalidArgument("sql is required".into()));
    }
    let mut connection = database.connect_taxonomy_context()?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some(input) = input.as_ref() {
        create_temp_input_table(&transaction, input)?;
    }
    authorize_custom_sql(&transaction, sql)?;
    let mut session = start_taxonomy_session(&transaction)?;
    transaction.execute_batch(sql)?;
    let mut changeset_blob = Vec::new();
    session.changeset_strm(&mut changeset_blob)?;
    let changeset_size = changeset_blob.len();
    drop(session);
    let affected_taxon_ids = if changeset_blob.is_empty() {
        BTreeSet::new()
    } else {
        validate_taxonomy(&transaction)?;
        affected_taxon_ids_from_changeset(&transaction, &changeset_blob)?
    };
    let operation_id =
        insert_custom_sql_operation(&transaction, &changeset_blob, &affected_taxon_ids)?;
    if !changeset_blob.is_empty() {
        super::sync::record_event(&transaction, Some(operation_id), affected_taxon_ids, false)?;
    }
    transaction.commit()?;
    Ok(TaxonomyCustomSqlResult {
        operation_id,
        changeset_size,
    })
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
            rollbackable: !changeset_blob.is_empty(),
            has_formatted_input: false,
        },
    )?;
    if !changeset_blob.is_empty() {
        transaction.execute(
            r#"
            INSERT INTO operation_changesets (operation_id, changeset_blob)
            VALUES (?, ?)
            "#,
            params![operation_id, changeset_blob],
        )?;
    }
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
                after_json: None,
                succeeded: true,
                message: "custom SQL made no taxonomy changes",
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

fn authorize_custom_sql(transaction: &Transaction<'_>, sql: &str) -> CoreResult<()> {
    if sql.to_ascii_lowercase().contains("taxon_names_fts") {
        return Err(CoreError::InvalidArgument(
            "custom sql cannot access taxonomy search index tables directly".into(),
        ));
    }
    prepare_custom_sql_batch(transaction, sql)
}

fn prepare_custom_sql_batch(connection: &rusqlite::Connection, sql: &str) -> CoreResult<()> {
    let database = unsafe { connection.handle() };
    let mut offset = 0;
    while offset < sql.len() {
        let sql_tail = &sql[offset..];
        let sql_tail = CString::new(sql_tail)
            .map_err(|error| CoreError::InvalidArgument(format!("invalid sql: {error}")))?;
        let mut statement = ptr::null_mut();
        let mut next_sql = ptr::null();
        connection.authorizer(Some(custom_sql_authorizer()));
        let code = unsafe {
            ffi::sqlite3_prepare_v2(
                database,
                sql_tail.as_ptr(),
                -1,
                &mut statement,
                &mut next_sql,
            )
        };
        connection.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
        if !statement.is_null() {
            unsafe {
                ffi::sqlite3_finalize(statement);
            }
        }
        if code != ffi::SQLITE_OK {
            return Err(sqlite_error(database, code));
        }
        if next_sql.is_null() {
            break;
        }
        let tail_offset = unsafe { next_sql.offset_from(sql_tail.as_ptr()) as usize };
        if tail_offset == 0 || tail_offset >= sql_tail.as_bytes().len() {
            break;
        }
        offset += tail_offset;
    }
    Ok(())
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

fn create_temp_input_table(
    transaction: &Transaction<'_>,
    input: &TaxonomyCustomSqlTempTable,
) -> CoreResult<()> {
    if input.columns.is_empty() {
        return Err(CoreError::InvalidArgument(
            "custom sql input requires at least one column".into(),
        ));
    }
    let mut seen = BTreeSet::new();
    let mut columns = Vec::with_capacity(input.columns.len());
    for column in &input.columns {
        let column = column.trim();
        if !is_safe_identifier(column) {
            return Err(CoreError::InvalidArgument(format!(
                "invalid custom sql input column: {column}"
            )));
        }
        if !seen.insert(column.to_ascii_lowercase()) {
            return Err(CoreError::InvalidArgument(format!(
                "duplicate custom sql input column: {column}"
            )));
        }
        columns.push(column.to_string());
    }
    for (index, row) in input.rows.iter().enumerate() {
        if row.len() != columns.len() {
            return Err(CoreError::InvalidArgument(format!(
                "custom sql input row {} has {} values but {} columns were declared",
                index + 1,
                row.len(),
                columns.len()
            )));
        }
    }
    let definitions = columns
        .iter()
        .map(|column| format!("{} TEXT", quote_identifier(column)))
        .collect::<Vec<_>>()
        .join(", ");
    transaction.execute_batch(&format!("CREATE TEMP TABLE input ({definitions})"))?;
    if !input.rows.is_empty() {
        let column_list = columns
            .iter()
            .map(|column| quote_identifier(column))
            .collect::<Vec<_>>()
            .join(", ");
        let placeholders = std::iter::repeat_n("?", columns.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("INSERT INTO temp.input ({column_list}) VALUES ({placeholders})");
        let mut statement = transaction.prepare(&sql)?;
        for row in &input.rows {
            statement.execute(params_from_iter(row.iter()))?;
        }
    }
    Ok(())
}

fn is_safe_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn custom_sql_authorizer() -> impl for<'a> FnMut(AuthContext<'a>) -> Authorization + Send + 'static
{
    move |context| authorize_custom_sql_action(context)
}

fn authorize_custom_sql_action(context: AuthContext<'_>) -> Authorization {
    match context.action {
        AuthAction::Select | AuthAction::Recursive => Authorization::Allow,
        AuthAction::Function { function_name } => {
            if function_name.eq_ignore_ascii_case("load_extension") {
                Authorization::Deny
            } else {
                Authorization::Allow
            }
        }
        AuthAction::Pragma { pragma_name, .. } => {
            if pragma_name.eq_ignore_ascii_case("data_version") {
                Authorization::Allow
            } else {
                Authorization::Deny
            }
        }
        AuthAction::Read { table_name, .. } => {
            if is_allowed_custom_sql_read(context.database_name, table_name) {
                Authorization::Allow
            } else {
                Authorization::Deny
            }
        }
        AuthAction::Insert { table_name }
        | AuthAction::Update { table_name, .. }
        | AuthAction::Delete { table_name } => {
            if is_allowed_custom_sql_write(context.accessor, table_name) {
                Authorization::Allow
            } else {
                Authorization::Deny
            }
        }
        _ => Authorization::Deny,
    }
}

fn is_allowed_custom_sql_read(database_name: Option<&str>, table_name: &str) -> bool {
    is_taxonomy_session_table(table_name)
        || table_name.starts_with("taxon_names_fts")
        || (database_name == Some("temp") && table_name == "input")
}

fn is_allowed_custom_sql_write(accessor: Option<&str>, table_name: &str) -> bool {
    is_taxonomy_session_table(table_name)
        || (accessor.is_some() && table_name.starts_with("taxon_names_fts"))
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn database() -> (TempDir, Database) {
        let directory = TempDir::new().unwrap();
        let database = Database::open_test(directory.path().join("test.db")).unwrap();
        (directory, database)
    }

    #[test]
    fn custom_sql_csv_parser_handles_quoted_delimiters_and_newlines() {
        let comma = parse_custom_taxonomy_input_csv(
            "name,notes\n\"Felis, catus\",\"line one\nline two\"\n",
        )
        .unwrap();
        assert_eq!(comma.columns, ["name", "notes"]);
        assert_eq!(
            comma.rows,
            [["Felis, catus".to_string(), "line one\nline two".to_string()]]
        );

        let pipe =
            parse_custom_taxonomy_input_csv("name|notes\n\"A|B\"|\"quoted \"\"value\"\"\"\n")
                .unwrap();
        assert_eq!(pipe.columns, ["name", "notes"]);
        assert_eq!(
            pipe.rows,
            [["A|B".to_string(), "quoted \"value\"".to_string()]]
        );
    }

    #[test]
    fn promoting_a_species_synonym_exchanges_name_types() {
        let (_directory, database) = database();
        apply_rows(
            &database,
            &[
                TaxonInputRow {
                    kingdom: Some("Animalia".into()),
                    ..TaxonInputRow::default()
                },
                TaxonInputRow {
                    kingdom: Some("Animalia".into()),
                    order: Some("Carnivora".into()),
                    ..TaxonInputRow::default()
                },
                TaxonInputRow {
                    kingdom: Some("Animalia".into()),
                    order: Some("Carnivora".into()),
                    family: Some("Canidae".into()),
                    ..TaxonInputRow::default()
                },
                TaxonInputRow {
                    kingdom: Some("Animalia".into()),
                    order: Some("Carnivora".into()),
                    family: Some("Canidae".into()),
                    genus: Some("Canis".into()),
                    ..TaxonInputRow::default()
                },
                TaxonInputRow {
                    kingdom: Some("Animalia".into()),
                    order: Some("Carnivora".into()),
                    family: Some("Canidae".into()),
                    genus: Some("Canis".into()),
                    species: Some("Canis lupus".into()),
                    synonyms: vec!["Canis lycaon".into(), "Felis lupus".into()],
                    ..TaxonInputRow::default()
                },
            ],
        )
        .unwrap();
        let connection = database.connect_taxonomy_context().unwrap();
        let (taxon_id, promoted_name_id, invalid_name_id): (i64, i64, i64) = connection
            .query_row(
                r#"
                SELECT accepted.taxon_id, promoted.name_id, invalid.name_id
                FROM taxon_names AS accepted
                JOIN taxon_names AS promoted
                  ON promoted.taxon_id = accepted.taxon_id
                 AND promoted.name = 'Canis lycaon'
                JOIN taxon_names AS invalid
                  ON invalid.taxon_id = accepted.taxon_id
                 AND invalid.name = 'Felis lupus'
                WHERE accepted.name = 'Canis lupus'
                "#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        drop(connection);

        promote_taxon_name(
            &database,
            PromoteTaxonNameInput {
                taxon_id,
                name_id: promoted_name_id,
            },
        )
        .unwrap();
        let connection = database.connect_taxonomy_context().unwrap();
        let promoted: i64 = connection
            .query_row(
                "SELECT name_type FROM taxon_names WHERE name = 'Canis lycaon'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let previous: i64 = connection
            .query_row(
                "SELECT name_type FROM taxon_names WHERE name = 'Canis lupus'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(promoted, TaxonomyNameType::SciName.code());
        assert_eq!(previous, TaxonomyNameType::Synonym.code());
        drop(connection);
        let operation = crate::taxonomy::list_operations(&database, None, 1)
            .unwrap()
            .items
            .remove(0);
        assert_eq!(operation.kind, "taxonomy_name_promote");
        assert!(operation.rollbackable);
        assert!(!operation.has_formatted_input);

        let error = promote_taxon_name(
            &database,
            PromoteTaxonNameInput {
                taxon_id,
                name_id: invalid_name_id,
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("parent genus"));

        crate::taxonomy::rollback_operation(&database, operation.operation_id).unwrap();
        let connection = database.connect_taxonomy_context().unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT name_type FROM taxon_names WHERE name = 'Canis lupus'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            TaxonomyNameType::SciName.code()
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT name_type FROM taxon_names WHERE name = 'Canis lycaon'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            TaxonomyNameType::Synonym.code()
        );
    }

    #[test]
    fn name_actions_require_matching_taxon_and_name_ids() {
        let (_directory, database) = database();
        apply_rows(
            &database,
            &[
                TaxonInputRow {
                    kingdom: Some("Animalia".into()),
                    synonyms: vec!["Metazoa".into()],
                    ..TaxonInputRow::default()
                },
                TaxonInputRow {
                    kingdom: Some("Plantae".into()),
                    ..TaxonInputRow::default()
                },
            ],
        )
        .unwrap();
        let connection = database.connect_taxonomy_context().unwrap();
        let (animalia_id, sci_name_id, synonym_id): (i64, i64, i64) = connection
            .query_row(
                r#"
                SELECT sci.taxon_id, sci.name_id, synonym.name_id
                FROM taxon_names AS sci
                JOIN taxon_names AS synonym USING (taxon_id)
                WHERE sci.name = 'Animalia' AND synonym.name = 'Metazoa'
                "#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let plantae_id: i64 = connection
            .query_row(
                "SELECT taxon_id FROM taxon_names WHERE name = 'Plantae'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        drop(connection);
        let detail = crate::taxonomy::get_taxon_detail(&database, animalia_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            detail.names.sci_name.as_ref().map(|name| name.name_id),
            Some(sci_name_id)
        );
        assert_eq!(detail.names.synonyms[0].name_id, synonym_id);

        let mismatch = delete_taxon_name(
            &database,
            DeleteTaxonNameInput {
                taxon_id: plantae_id,
                name_id: synonym_id,
            },
        )
        .unwrap_err();
        assert!(mismatch.to_string().contains("not found"));
        let sci_error = delete_taxon_name(
            &database,
            DeleteTaxonNameInput {
                taxon_id: animalia_id,
                name_id: sci_name_id,
            },
        )
        .unwrap_err();
        assert!(sci_error.to_string().contains("cannot be deleted"));

        delete_taxon_name(
            &database,
            DeleteTaxonNameInput {
                taxon_id: animalia_id,
                name_id: synonym_id,
            },
        )
        .unwrap();
        crate::taxonomy::synchronize_pending_photo_libraries(&database).unwrap();
        assert!(
            database
                .connect_taxonomy_context()
                .unwrap()
                .query_row(
                    "SELECT NOT EXISTS(SELECT 1 FROM taxon_names WHERE name_id = ?)",
                    [synonym_id],
                    |row| row.get::<_, bool>(0),
                )
                .unwrap()
        );
        let operation = crate::taxonomy::list_operations(&database, None, 1)
            .unwrap()
            .items
            .remove(0);
        assert_eq!(operation.kind, "taxonomy_delete");
        assert!(!operation.has_formatted_input);
        crate::taxonomy::rollback_operation(&database, operation.operation_id).unwrap();
        assert!(
            database
                .connect_taxonomy_context()
                .unwrap()
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM taxon_names WHERE name_id = ?)",
                    [synonym_id],
                    |row| row.get::<_, bool>(0),
                )
                .unwrap()
        );
    }

    #[test]
    fn custom_taxon_delete_queues_old_matches() {
        let (_directory, database) = database();
        let connection = database.connect_taxonomy_context().unwrap();
        connection
            .execute("INSERT INTO taxa (rank) VALUES (5)", [])
            .unwrap();
        let taxon_id = connection.last_insert_rowid();
        connection
            .execute(
                r#"
                INSERT INTO taxon_names (taxon_id, name_type, name)
                VALUES (?, 1, 'Felis catus')
                "#,
                [taxon_id],
            )
            .unwrap();
        let name_id = connection.last_insert_rowid();
        connection
            .execute_batch(
                r#"
                INSERT INTO photo_directories (
                    directory_id, parent_directory_id, name, relative_path
                ) VALUES (1, NULL, '', '');
                INSERT INTO photos (
                    photo_id, directory_id, filename, file_size, modified_at_ns
                ) VALUES
                    (1, 1, 'Felis catus.jpg', 1, 1),
                    (2, 1, 'Ambiguous cat.jpg', 1, 1);
                "#,
            )
            .unwrap();
        connection
            .execute(
                r#"
                INSERT INTO photo_taxon_mapping (photo_id, taxon_id, status)
                VALUES (1, ?, 'matched')
                "#,
                [taxon_id],
            )
            .unwrap();
        connection
            .execute(
                r#"
                INSERT INTO photo_taxon_mapping (photo_id, taxon_id, status)
                VALUES (2, NULL, 'ambiguous')
                "#,
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO photo_taxon_candidates (photo_id, taxon_id) VALUES (2, ?)",
                [taxon_id],
            )
            .unwrap();
        connection
            .execute(
                r#"
                INSERT INTO photo_taxon_candidate_names (
                    photo_id, taxon_id, name_id, name_type, name
                ) VALUES (2, ?, ?, 1, 'Felis catus')
                "#,
                params![taxon_id, name_id],
            )
            .unwrap();
        connection
            .execute(
                r#"
                INSERT INTO photo_taxon_usage (
                    taxon_id, direct_photo_count, subtree_photo_count
                ) VALUES (?, 1, 1)
                "#,
                [taxon_id],
            )
            .unwrap();
        drop(connection);

        let result = execute_custom_taxonomy_sql(
            &database,
            &format!("DELETE FROM taxa WHERE taxon_id = {taxon_id}"),
            None,
        )
        .unwrap();
        crate::taxonomy::synchronize_pending_photo_libraries(&database).unwrap();
        assert!(
            crate::taxonomy::export_operation_input(&database, result.operation_id)
                .unwrap_err()
                .to_string()
                .contains("formatted input")
        );

        let connection = database.connect_taxonomy_context().unwrap();
        let queued: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM photo_mapping_queue WHERE photo_id IN (1, 2)",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(queued, 2);
    }
}
