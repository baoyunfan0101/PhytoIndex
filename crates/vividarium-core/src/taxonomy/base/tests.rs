use super::*;
use crate::taxonomy::{TaxonInputRow, apply_rows, get_taxon_detail, list_operations};

#[test]
fn replaces_taxonomy_preserves_base_ids_and_queues_all_photos() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open_test(directory.path().join("vividarium.db")).unwrap();
    let old_taxon_ids = seed_old_taxonomy_tree(&database);
    let old_taxon_id = old_taxon_ids[2];
    let connection = database.connect().unwrap();
    connection
        .execute(
            "UPDATE photo_library SET root_path = '/photos' WHERE library_id = 1",
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
    let sync = sync::synchronize_pending_photo_libraries(&database).unwrap();

    assert_eq!(result.metadata.taxa_count, 2);
    assert_eq!(result.metadata.taxon_names_count, 2);
    assert_eq!(sync.synchronized[0].queued_photo_count, 1);
    for taxon_id in old_taxon_ids {
        assert!(get_taxon_detail(&database, taxon_id).unwrap().is_none());
    }
    assert!(get_taxon_detail(&database, 101).unwrap().is_some());
    assert!(get_taxon_detail(&database, 102).unwrap().is_some());
    assert!(
        list_operations(&database, None, 10)
            .unwrap()
            .items
            .is_empty()
    );
    let rebased = apply_rows(
        &database,
        &[TaxonInputRow {
            order: Some("New order".into()),
            en_name: Some("new order".into()),
            ..TaxonInputRow::default()
        }],
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
    assert_eq!(list_operations(&database, None, 10).unwrap().items.len(), 1);
}

#[test]
fn rejects_a_base_database_with_text_name_types() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("vividarium.db")).unwrap();
    let source_path = directory.path().join("base.db");
    create_base_database_with_name_type(&source_path, "TEXT");

    let error = replace_taxonomy_base_database(&database, &source_path).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("taxon_names column name_type must use INTEGER")
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
    create_base_database_with_name_type(path, "INTEGER");
}

fn create_base_database_with_name_type(path: &Path, name_type: &str) {
    let connection = Connection::open(path).unwrap();
    connection
        .execute_batch(&format!(
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
                    name_type {name_type} NOT NULL,
                    name TEXT NOT NULL,
                    normalized_name TEXT GENERATED ALWAYS AS (lower(name)) STORED,
                    authority_year TEXT,
                    source TEXT,
                    FOREIGN KEY (taxon_id) REFERENCES taxa(taxon_id)
                );
                INSERT INTO taxa (
                    taxon_id, parent_taxon_id, rank, geological_range
                ) VALUES
                    (101, NULL, 1, NULL),
                    (102, 101, 2, 'Recent');
                INSERT INTO taxon_names (
                    name_id, taxon_id, name_type, name
                ) VALUES
                    (1001, 101, 1, 'New_kingdom'),
                    (1002, 102, 1, '  New   order  ');
                "#
        ))
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
                    name_type INTEGER NOT NULL,
                    name TEXT NOT NULL,
                    normalized_name TEXT GENERATED ALWAYS AS (lower(name)) STORED,
                    authority_year TEXT,
                    source TEXT,
                    FOREIGN KEY (taxon_id) REFERENCES taxa(taxon_id)
                );
                INSERT INTO taxa (
                    taxon_id, parent_taxon_id, rank, geological_range
                ) VALUES
                    (201, NULL, 1, NULL),
                    (202, 201, 3, NULL);
                INSERT INTO taxon_names (
                    name_id, taxon_id, name_type, name
                ) VALUES
                    (2001, 201, 1, 'Invalid kingdom'),
                    (2002, 202, 1, 'Invalid family');
                "#,
        )
        .unwrap();
}

fn taxon_id_by_name(database: &Database, name: &str) -> i64 {
    database
        .connect_taxonomy_metadata_context()
        .unwrap()
        .query_row(
            "SELECT taxon_id FROM taxon_names WHERE name_type = 1 AND name = ?",
            [name],
            |row| row.get(0),
        )
        .unwrap()
}

fn parent_taxon_id(database: &Database, taxon_id: i64) -> Option<i64> {
    database
        .connect_taxonomy_metadata_context()
        .unwrap()
        .query_row(
            "SELECT parent_taxon_id FROM taxa WHERE taxon_id = ?",
            [taxon_id],
            |row| row.get(0),
        )
        .unwrap()
}
