use std::fs;
use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

use super::formatted::validate_taxonomy;
use super::sync;
use crate::db::LOCAL_TAXON_ID_FLOOR;
use crate::naming::normalize_taxonomy_name;
use crate::{CoreError, CoreResult, Database};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonomyBaseMetadata {
    pub source_path: String,
    pub taxa_count: i64,
    pub taxon_names_count: i64,
    pub imported_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonomyBaseReplaceResult {
    pub metadata: TaxonomyBaseMetadata,
    pub warnings: Vec<String>,
}

pub fn get_taxonomy_base_metadata(database: &Database) -> CoreResult<Option<TaxonomyBaseMetadata>> {
    let connection = database.connect_taxonomy_metadata_context()?;
    connection
        .query_row(
            r#"
            SELECT source_path, taxa_count, taxon_names_count, imported_at
            FROM taxonomy_base_metadata
            WHERE metadata_id = 1
            "#,
            [],
            taxonomy_base_metadata_row,
        )
        .optional()
        .map_err(Into::into)
}

pub fn replace_taxonomy_base_database(
    database: &Database,
    source_path: &Path,
) -> CoreResult<TaxonomyBaseReplaceResult> {
    let _guard = database.try_taxonomy_replacement()?;
    let source_path = fs::canonicalize(source_path)?;
    let target_path = fs::canonicalize(database.taxonomy_path()?)?;
    if source_path == target_path {
        return Err(CoreError::InvalidArgument(
            "taxonomy base database must differ from the application database".into(),
        ));
    }
    validate_base_database(&source_path)?;
    let source_path = source_path
        .to_str()
        .ok_or_else(|| CoreError::InvalidArgument("taxonomy base path is not valid UTF-8".into()))?
        .to_string();
    let mut connection = database.connect_taxonomy_metadata_context()?;
    connection.execute("ATTACH DATABASE ? AS taxonomy_base", [&source_path])?;
    let result = replace_from_attached_database(&mut connection, &source_path);
    let detach_result = connection.execute_batch("DETACH DATABASE taxonomy_base");
    match (result, detach_result) {
        (Ok(result), Ok(())) => Ok(result),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error.into()),
    }
}

fn replace_from_attached_database(
    connection: &mut Connection,
    source_path: &str,
) -> CoreResult<TaxonomyBaseReplaceResult> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        r#"
        DELETE FROM operations;
        DELETE FROM taxonomy_base_metadata;
        DELETE FROM taxonomy_sync_events;
        UPDATE taxa SET parent_taxon_id = NULL;
        DELETE FROM taxon_names;
        DELETE FROM taxa;
        DELETE FROM sqlite_sequence
        WHERE name IN ('operations', 'taxon_names', 'taxa');

        INSERT INTO taxa (taxon_id, parent_taxon_id, rank, geological_range)
        SELECT taxon_id, parent_taxon_id, rank, geological_range
        FROM taxonomy_base.taxa
        ORDER BY rank, taxon_id;
        "#,
    )?;
    import_normalized_names(&transaction)?;
    validate_taxonomy(&transaction)?;
    set_local_taxon_id_floor(&transaction)?;
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
        params![source_path, taxa_count, taxon_names_count],
    )?;
    transaction.execute(
        "UPDATE taxonomy_identity SET taxonomy_identity = ? WHERE identity_id = 1",
        [uuid::Uuid::new_v4().to_string()],
    )?;
    sync::record_event(&transaction, None, [], true)?;
    let metadata = transaction.query_row(
        r#"
        SELECT source_path, taxa_count, taxon_names_count, imported_at
        FROM taxonomy_base_metadata
        WHERE metadata_id = 1
        "#,
        [],
        taxonomy_base_metadata_row,
    )?;
    transaction.commit()?;
    Ok(TaxonomyBaseReplaceResult {
        metadata,
        warnings: Vec::new(),
    })
}

fn import_normalized_names(transaction: &Transaction<'_>) -> CoreResult<()> {
    let names = {
        let mut statement = transaction.prepare(
            r#"
            SELECT name_id, taxon_id, name_type, name, authority_year, source
            FROM taxonomy_base.taxon_names
            ORDER BY name_id
            "#,
        )?;
        statement
            .query_map([], |row| {
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
    let mut insert = transaction.prepare_cached(
        r#"
        INSERT INTO taxon_names (
            name_id, taxon_id, name_type, name, authority_year, source
        ) VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )?;
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
    }
    Ok(())
}

fn set_local_taxon_id_floor(transaction: &Transaction<'_>) -> CoreResult<()> {
    transaction.execute(
        r#"
        INSERT INTO sqlite_sequence(name, seq)
        SELECT 'taxa', ?
        WHERE NOT EXISTS (SELECT 1 FROM sqlite_sequence WHERE name = 'taxa')
        "#,
        [LOCAL_TAXON_ID_FLOOR],
    )?;
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
    Ok(())
}

fn validate_base_database(path: &Path) -> CoreResult<()> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    require_table_columns(
        &connection,
        "taxa",
        &["taxon_id", "parent_taxon_id", "rank", "geological_range"],
        false,
    )?;
    require_table_columns(
        &connection,
        "taxon_names",
        &[
            "name_id",
            "taxon_id",
            "name_type",
            "name",
            "normalized_name",
            "authority_year",
            "source",
        ],
        true,
    )?;
    require_column_declared_type(&connection, "taxon_names", "name_type", "INTEGER")?;
    Ok(())
}

fn require_table_columns(
    connection: &Connection,
    table: &str,
    expected: &[&str],
    include_hidden: bool,
) -> CoreResult<()> {
    let object_type = connection
        .query_row(
            "SELECT type FROM sqlite_master WHERE name = ?",
            [table],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if object_type.as_deref() != Some("table") {
        return Err(CoreError::InvalidArgument(format!(
            "taxonomy base database is missing table {table}"
        )));
    }
    let pragma = if include_hidden {
        format!("PRAGMA table_xinfo({table})")
    } else {
        format!("PRAGMA table_info({table})")
    };
    let mut statement = connection.prepare(&pragma)?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if columns != expected {
        return Err(CoreError::InvalidArgument(format!(
            "taxonomy base table {table} has invalid columns"
        )));
    }
    Ok(())
}

fn require_column_declared_type(
    connection: &Connection,
    table: &str,
    column: &str,
    expected: &str,
) -> CoreResult<()> {
    let mut statement = connection.prepare(&format!("PRAGMA table_xinfo({table})"))?;
    let columns = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let declared_type = columns
        .iter()
        .find_map(|(name, declared_type)| (name == column).then_some(declared_type.as_str()));
    if !declared_type.is_some_and(|declared_type| declared_type.eq_ignore_ascii_case(expected)) {
        return Err(CoreError::InvalidArgument(format!(
            "taxonomy base table {table} column {column} must use {expected}"
        )));
    }
    Ok(())
}

fn taxonomy_base_metadata_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaxonomyBaseMetadata> {
    Ok(TaxonomyBaseMetadata {
        source_path: row.get(0)?,
        taxa_count: row.get(1)?,
        taxon_names_count: row.get(2)?,
        imported_at: row.get(3)?,
    })
}

#[cfg(test)]
#[path = "base/tests.rs"]
mod tests;
