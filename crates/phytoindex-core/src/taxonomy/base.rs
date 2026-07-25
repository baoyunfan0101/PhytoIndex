use std::fs;
use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

use super::update::validate_taxonomy;
use crate::db::LOCAL_TAXON_ID_FLOOR;
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
    pub queued_photo_count: i64,
}

pub fn get_taxonomy_base_metadata(database: &Database) -> CoreResult<Option<TaxonomyBaseMetadata>> {
    let connection = database.connect()?;
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
    let source_path = fs::canonicalize(source_path)?;
    let target_path = fs::canonicalize(database.path())?;
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
    let mut connection = database.connect()?;
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
        DELETE FROM photo_taxon_usage;
        DELETE FROM photo_mapping_queue;
        DELETE FROM photo_taxon_mapping;
        DELETE FROM taxonomy_operations;
        DELETE FROM taxonomy_base_metadata;
        UPDATE taxa SET parent_taxon_id = NULL;
        DELETE FROM taxon_names;
        DELETE FROM taxa;
        DELETE FROM sqlite_sequence
        WHERE name IN ('taxonomy_operations', 'taxon_names', 'taxa');

        INSERT INTO taxa (taxon_id, parent_taxon_id, rank, geological_range)
        SELECT taxon_id, parent_taxon_id, rank, geological_range
        FROM taxonomy_base.taxa
        ORDER BY rank, taxon_id;

        INSERT INTO taxon_names (
            name_id, taxon_id, name_kind, name, is_accepted,
            authority_year, category, source
        )
        SELECT
            name_id, taxon_id, name_kind, name, is_accepted,
            authority_year, category, source
        FROM taxonomy_base.taxon_names
        ORDER BY name_id;
        "#,
    )?;
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
        r#"
        INSERT INTO photo_mapping_queue (photo_id, reason)
        SELECT photo_id, 'taxonomy' FROM photos
        "#,
        [],
    )?;
    let queued_photo_count =
        transaction.query_row("SELECT COUNT(*) FROM photo_mapping_queue", [], |row| {
            row.get::<_, i64>(0)
        })?;
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
        queued_photo_count,
    })
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
            "name_kind",
            "name",
            "normalized_name",
            "is_accepted",
            "authority_year",
            "category",
            "source",
        ],
        true,
    )?;
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

fn taxonomy_base_metadata_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaxonomyBaseMetadata> {
    Ok(TaxonomyBaseMetadata {
        source_path: row.get(0)?,
        taxa_count: row.get(1)?,
        taxon_names_count: row.get(2)?,
        imported_at: row.get(3)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taxonomy::{
        TaxonInputRow, TaxonUpdateOptions, apply_rows, get_taxon_detail, list_taxonomy_operations,
    };

    #[test]
    fn replaces_taxonomy_preserves_base_ids_and_queues_all_photos() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("vividarium.db")).unwrap();
        let old_taxon_ids = seed_old_taxonomy_tree(&database);
        let old_taxon_id = old_taxon_ids[2];
        let connection = database.connect().unwrap();
        connection
            .execute(
                "INSERT INTO photo_library (library_id, root_path) VALUES (1, '/photos')",
                [],
            )
            .unwrap();
        connection
            .execute(
                r#"
                INSERT INTO photo_directories (
                    parent_directory_id, name, relative_path
                ) VALUES (NULL, '', '')
                "#,
                [],
            )
            .unwrap();
        let directory_id = connection.last_insert_rowid();
        connection
            .execute(
                r#"
                INSERT INTO photos (
                    directory_id, filename, file_size, modified_at_ns
                ) VALUES (?, 'New species.jpg', 1, 1)
                "#,
                [directory_id],
            )
            .unwrap();
        let photo_id = connection.last_insert_rowid();
        connection
            .execute(
                r#"
                INSERT INTO photo_taxon_mapping (photo_id, taxon_id, status)
                VALUES (?, ?, 'matched')
                "#,
                params![photo_id, old_taxon_id],
            )
            .unwrap();
        connection
            .execute(
                r#"
                INSERT INTO photo_taxon_usage (
                    taxon_id, direct_photo_count, subtree_photo_count
                ) VALUES (?, 1, 1)
                "#,
                [old_taxon_id],
            )
            .unwrap();
        drop(connection);

        let source_path = directory.path().join("base.db");
        create_base_database(&source_path);
        let result = replace_taxonomy_base_database(&database, &source_path).unwrap();

        assert_eq!(result.metadata.taxa_count, 2);
        assert_eq!(result.metadata.taxon_names_count, 2);
        assert_eq!(result.queued_photo_count, 1);
        for taxon_id in old_taxon_ids {
            assert!(get_taxon_detail(&database, taxon_id).unwrap().is_none());
        }
        assert!(get_taxon_detail(&database, 101).unwrap().is_some());
        assert!(get_taxon_detail(&database, 102).unwrap().is_some());
        assert!(
            list_taxonomy_operations(&database, None, 10)
                .unwrap()
                .items
                .is_empty()
        );
        let rebased = apply_rows(
            &database,
            &[TaxonInputRow {
                order: Some("New order".into()),
                english: Some(crate::taxonomy::TaxonNameInput {
                    name: "new order".into(),
                    ..crate::taxonomy::TaxonNameInput::default()
                }),
                ..TaxonInputRow::default()
            }],
            TaxonUpdateOptions {
                allow_new_names: true,
                ..TaxonUpdateOptions::default()
            },
        )
        .unwrap();
        assert_eq!(rebased.operation_id, 1);
        let connection = database.connect().unwrap();
        let mapping_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM photo_taxon_mapping", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(mapping_count, 0);
        let queued_reason: String = connection
            .query_row(
                "SELECT reason FROM photo_mapping_queue WHERE photo_id = ?",
                [photo_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(queued_reason, "taxonomy");
        connection
            .execute("INSERT INTO taxa (rank) VALUES (1)", [])
            .unwrap();
        assert!(connection.last_insert_rowid() > LOCAL_TAXON_ID_FLOOR);
    }

    #[test]
    fn rejects_an_invalid_base_without_changing_taxonomy() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("vividarium.db")).unwrap();
        let taxon_ids = seed_old_taxonomy_tree(&database);
        let invalid_path = directory.path().join("invalid.db");
        create_invalid_base_database(&invalid_path);

        let error = replace_taxonomy_base_database(&database, &invalid_path).unwrap_err();

        assert!(error.to_string().contains("invalid parentage"));
        for taxon_id in taxon_ids {
            assert!(get_taxon_detail(&database, taxon_id).unwrap().is_some());
        }
        assert_eq!(parent_taxon_id(&database, taxon_ids[1]), Some(taxon_ids[0]));
        assert_eq!(parent_taxon_id(&database, taxon_ids[2]), Some(taxon_ids[1]));
        assert_eq!(
            list_taxonomy_operations(&database, None, 10)
                .unwrap()
                .items
                .len(),
            1
        );
    }

    fn seed_old_taxonomy_tree(database: &Database) -> [i64; 3] {
        let result = apply_rows(
            database,
            &[
                TaxonInputRow {
                    kingdom: Some("Old kingdom".into()),
                    ..TaxonInputRow::default()
                },
                TaxonInputRow {
                    kingdom: Some("Old kingdom".into()),
                    order: Some("Old order".into()),
                    ..TaxonInputRow::default()
                },
                TaxonInputRow {
                    kingdom: Some("Old kingdom".into()),
                    order: Some("Old order".into()),
                    family: Some("Old family".into()),
                    ..TaxonInputRow::default()
                },
            ],
            TaxonUpdateOptions {
                allow_new_taxa: true,
                ..TaxonUpdateOptions::default()
            },
        )
        .unwrap();
        assert_eq!(result.succeeded_rows, 3);
        [
            taxon_id_by_name(database, "Old kingdom"),
            taxon_id_by_name(database, "Old order"),
            taxon_id_by_name(database, "Old family"),
        ]
    }

    fn create_base_database(path: &Path) {
        let connection = Connection::open(path).unwrap();
        connection
            .execute_batch(
                r#"
                PRAGMA foreign_keys = ON;
                CREATE TABLE taxa (
                    taxon_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    parent_taxon_id INTEGER,
                    rank INTEGER NOT NULL,
                    geological_range TEXT,
                    FOREIGN KEY (parent_taxon_id) REFERENCES taxa(taxon_id)
                );
                CREATE TABLE taxon_names (
                    name_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    taxon_id INTEGER NOT NULL,
                    name_kind INTEGER NOT NULL,
                    name TEXT NOT NULL,
                    normalized_name TEXT GENERATED ALWAYS AS (lower(name)) STORED,
                    is_accepted INTEGER NOT NULL DEFAULT 0,
                    authority_year TEXT,
                    category TEXT,
                    source TEXT,
                    FOREIGN KEY (taxon_id) REFERENCES taxa(taxon_id)
                );
                INSERT INTO taxa (
                    taxon_id, parent_taxon_id, rank, geological_range
                ) VALUES
                    (101, NULL, 1, NULL),
                    (102, 101, 2, 'Recent');
                INSERT INTO taxon_names (
                    name_id, taxon_id, name_kind, name, is_accepted
                ) VALUES
                    (1001, 101, 1, 'New kingdom', 1),
                    (1002, 102, 1, 'New order', 1);
                "#,
            )
            .unwrap();
    }

    fn create_invalid_base_database(path: &Path) {
        let connection = Connection::open(path).unwrap();
        connection
            .execute_batch(
                r#"
                PRAGMA foreign_keys = ON;
                CREATE TABLE taxa (
                    taxon_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    parent_taxon_id INTEGER,
                    rank INTEGER NOT NULL,
                    geological_range TEXT,
                    FOREIGN KEY (parent_taxon_id) REFERENCES taxa(taxon_id)
                );
                CREATE TABLE taxon_names (
                    name_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    taxon_id INTEGER NOT NULL,
                    name_kind INTEGER NOT NULL,
                    name TEXT NOT NULL,
                    normalized_name TEXT GENERATED ALWAYS AS (lower(name)) STORED,
                    is_accepted INTEGER NOT NULL DEFAULT 0,
                    authority_year TEXT,
                    category TEXT,
                    source TEXT,
                    FOREIGN KEY (taxon_id) REFERENCES taxa(taxon_id)
                );
                INSERT INTO taxa (
                    taxon_id, parent_taxon_id, rank, geological_range
                ) VALUES
                    (201, NULL, 1, NULL),
                    (202, 201, 3, NULL);
                INSERT INTO taxon_names (
                    name_id, taxon_id, name_kind, name, is_accepted
                ) VALUES
                    (2001, 201, 1, 'Invalid kingdom', 1),
                    (2002, 202, 1, 'Invalid family', 1);
                "#,
            )
            .unwrap();
    }

    fn taxon_id_by_name(database: &Database, name: &str) -> i64 {
        database
            .connect()
            .unwrap()
            .query_row(
                "SELECT taxon_id FROM taxon_names WHERE name_kind = 1 AND name = ?",
                [name],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn parent_taxon_id(database: &Database, taxon_id: i64) -> Option<i64> {
        database
            .connect()
            .unwrap()
            .query_row(
                "SELECT parent_taxon_id FROM taxa WHERE taxon_id = ?",
                [taxon_id],
                |row| row.get(0),
            )
            .unwrap()
    }
}
