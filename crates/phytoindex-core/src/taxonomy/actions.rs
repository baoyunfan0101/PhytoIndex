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
use crate::mapping;
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
    pub changeset_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonomyCustomSqlTempTable {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

pub fn update_taxon(
    database: &Database,
    input: TaxonUpdateInput,
) -> CoreResult<TaxonomyOperationResult> {
    let connection = database.connect()?;
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
    let mut connection = database.connect()?;
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
    let accepted_name_id = transaction
        .query_row(
            "SELECT name_id FROM taxon_names WHERE taxon_id = ? AND name_type = ?",
            params![input.taxon_id, accepted_type.code()],
            |row| row.get::<_, i64>(0),
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
    transaction.execute(
        "UPDATE taxon_names SET name_type = ? WHERE taxon_id = ? AND name_id = ?",
        params![current_type.code(), input.taxon_id, accepted_name_id],
    )?;
    transaction.execute(
        "UPDATE taxon_names SET name_type = ? WHERE taxon_id = ? AND name_id = ?",
        params![accepted_type.code(), input.taxon_id, input.name_id],
    )?;
    validate_taxonomy(&transaction)?;
    transaction.commit()?;
    mapping::refresh_after_taxonomy_changes(database, [input.taxon_id])?;
    Ok(())
}

pub fn delete_taxon_name(database: &Database, input: DeleteTaxonNameInput) -> CoreResult<()> {
    let mut connection = database.connect()?;
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
    transaction.commit()?;
    mapping::refresh_after_taxonomy_changes(database, [input.taxon_id])?;
    Ok(())
}

pub fn delete_taxon(database: &Database, taxon_id: i64) -> CoreResult<()> {
    let mut connection = database.connect()?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let exists: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM taxa WHERE taxon_id = ?)",
        [taxon_id],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(CoreError::NotFound(format!("taxon {taxon_id}")));
    }
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
    transaction.execute("DELETE FROM taxa WHERE taxon_id = ?", [taxon_id])?;
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
    let mut connection = database.connect()?;
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
    if changeset_blob.is_empty() {
        transaction.commit()?;
        return Ok(TaxonomyCustomSqlResult { changeset_size });
    }
    validate_taxonomy(&transaction)?;
    let affected_taxon_ids = affected_taxon_ids_from_changeset(&transaction, &changeset_blob)?;
    transaction.commit()?;
    mapping::refresh_after_taxonomy_changes(database, affected_taxon_ids)?;
    Ok(TaxonomyCustomSqlResult { changeset_size })
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

#[derive(Default)]
struct CustomSqlAuthorizer {
    deletes_taxa: bool,
}

fn custom_sql_authorizer() -> impl for<'a> FnMut(AuthContext<'a>) -> Authorization + Send + 'static
{
    let mut authorizer = CustomSqlAuthorizer::default();
    move |context| authorizer.authorize(context)
}

impl CustomSqlAuthorizer {
    fn authorize(&mut self, context: AuthContext<'_>) -> Authorization {
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
                if is_allowed_custom_sql_read(
                    self.deletes_taxa,
                    context.database_name,
                    context.accessor,
                    table_name,
                ) {
                    Authorization::Allow
                } else {
                    Authorization::Deny
                }
            }
            AuthAction::Insert { table_name }
            | AuthAction::Update { table_name, .. }
            | AuthAction::Delete { table_name } => {
                if context.accessor.is_none() && table_name == "taxa" {
                    self.deletes_taxa = true;
                }
                if is_allowed_custom_sql_write(self.deletes_taxa, context.accessor, table_name) {
                    Authorization::Allow
                } else {
                    Authorization::Deny
                }
            }
            _ => Authorization::Deny,
        }
    }
}

fn is_allowed_custom_sql_read(
    deletes_taxa: bool,
    database_name: Option<&str>,
    accessor: Option<&str>,
    table_name: &str,
) -> bool {
    is_taxonomy_session_table(table_name)
        || table_name.starts_with("taxon_names_fts")
        || (database_name == Some("temp") && table_name == "input")
        || (accessor == Some("taxa_bd_photo_mapping")
            && matches!(
                table_name,
                "photo_mapping_queue" | "photo_taxon_mapping" | "photos"
            ))
        || is_taxa_delete_foreign_key_access(deletes_taxa, accessor, table_name)
}

fn is_allowed_custom_sql_write(
    deletes_taxa: bool,
    accessor: Option<&str>,
    table_name: &str,
) -> bool {
    is_taxonomy_session_table(table_name)
        || (accessor.is_some() && table_name.starts_with("taxon_names_fts"))
        || (accessor == Some("taxa_bd_photo_mapping")
            && matches!(table_name, "photo_mapping_queue" | "photo_taxon_mapping"))
        || is_taxa_delete_foreign_key_access(deletes_taxa, accessor, table_name)
}

fn is_taxa_delete_foreign_key_access(
    deletes_taxa: bool,
    accessor: Option<&str>,
    table_name: &str,
) -> bool {
    deletes_taxa
        && accessor.is_none()
        && matches!(table_name, "photo_taxon_mapping" | "photo_taxon_usage")
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn database() -> (TempDir, Database) {
        let directory = TempDir::new().unwrap();
        let database = Database::open(directory.path().join("test.db")).unwrap();
        (directory, database)
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
        let connection = database.connect().unwrap();
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
        let connection = database.connect().unwrap();
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

        let error = promote_taxon_name(
            &database,
            PromoteTaxonNameInput {
                taxon_id,
                name_id: invalid_name_id,
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("parent genus"));
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
        let connection = database.connect().unwrap();
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
        assert!(
            database
                .connect()
                .unwrap()
                .query_row(
                    "SELECT NOT EXISTS(SELECT 1 FROM taxon_names WHERE name_id = ?)",
                    [synonym_id],
                    |row| row.get::<_, bool>(0),
                )
                .unwrap()
        );
    }
}
