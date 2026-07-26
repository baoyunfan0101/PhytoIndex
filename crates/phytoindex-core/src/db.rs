use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, Row};

use crate::error::{CoreError, CoreResult};
use crate::models::Photo;

const SCHEMA_VERSION: i64 = 2;
pub(crate) const LOCAL_TAXON_ID_FLOOR: i64 = 8_000_000_000_000_000;

#[derive(Debug, Clone)]
pub struct Database {
    path: PathBuf,
}

impl Database {
    pub fn open(path: impl Into<PathBuf>) -> CoreResult<Self> {
        let database = Self { path: path.into() };
        if let Some(parent) = database.path.parent() {
            fs::create_dir_all(parent)?;
        }
        database.initialize()?;
        Ok(database)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn connect(&self) -> CoreResult<Connection> {
        let connection = Connection::open(&self.path)?;
        connection.busy_timeout(Duration::from_secs(30))?;
        connection.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            "#,
        )?;
        Ok(connection)
    }

    fn initialize(&self) -> CoreResult<()> {
        let connection = self.connect()?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        match version {
            0 => {
                connection.execute_batch(SCHEMA)?;
                crate::metadata::insert_raw_if_missing(
                    &connection,
                    crate::metadata::MetadataKey::TaxonomyNameSeparator,
                    ";",
                )?;
                crate::naming::seed_default_test_cases(&connection)?;
            }
            SCHEMA_VERSION => {}
            _ => {
                return Err(CoreError::InvalidArgument(format!(
                    "unsupported database schema version: {version}; expected {SCHEMA_VERSION}"
                )));
            }
        }
        Ok(())
    }
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS photo_library (
    library_id INTEGER PRIMARY KEY CHECK (library_id = 1),
    root_path TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS photo_directories (
    directory_id INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_directory_id INTEGER,
    name TEXT NOT NULL,
    relative_path TEXT NOT NULL UNIQUE,
    UNIQUE (parent_directory_id, name),
    FOREIGN KEY (parent_directory_id) REFERENCES photo_directories(directory_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS photos (
    photo_id INTEGER PRIMARY KEY AUTOINCREMENT,
    directory_id INTEGER NOT NULL,
    filename TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    modified_at_ns INTEGER NOT NULL,
    thumbnail_path TEXT,
    UNIQUE (directory_id, filename),
    CHECK (length(filename) > 0),
    CHECK (file_size >= 0),
    FOREIGN KEY (directory_id) REFERENCES photo_directories(directory_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS photo_metadata (
    photo_id INTEGER PRIMARY KEY,
    captured_at TEXT,
    camera TEXT,
    width INTEGER,
    height INTEGER,
    longitude REAL,
    latitude REAL,
    exif_json TEXT,
    FOREIGN KEY (photo_id) REFERENCES photos(photo_id) ON DELETE CASCADE
);

CREATE VIRTUAL TABLE IF NOT EXISTS photo_filenames_fts USING fts5(
    filename,
    content = 'photos',
    content_rowid = 'photo_id',
    tokenize = 'trigram'
);

CREATE TRIGGER IF NOT EXISTS photos_ai AFTER INSERT ON photos BEGIN
    INSERT INTO photo_filenames_fts(rowid, filename) VALUES (new.photo_id, new.filename);
END;

CREATE TRIGGER IF NOT EXISTS photos_ad AFTER DELETE ON photos BEGIN
    INSERT INTO photo_filenames_fts(photo_filenames_fts, rowid, filename)
    VALUES ('delete', old.photo_id, old.filename);
END;

CREATE TRIGGER IF NOT EXISTS photos_au AFTER UPDATE OF filename ON photos BEGIN
    INSERT INTO photo_filenames_fts(photo_filenames_fts, rowid, filename)
    VALUES ('delete', old.photo_id, old.filename);
    INSERT INTO photo_filenames_fts(rowid, filename) VALUES (new.photo_id, new.filename);
END;

CREATE TABLE IF NOT EXISTS photo_taxon_mapping (
    photo_id INTEGER PRIMARY KEY,
    taxon_id INTEGER,
    status TEXT NOT NULL,
    CHECK (status IN ('matched', 'unmatched', 'ambiguous', 'processing', 'stale')),
    CHECK ((status = 'matched' AND taxon_id IS NOT NULL)
        OR (status != 'matched' AND taxon_id IS NULL)),
    FOREIGN KEY (photo_id) REFERENCES photos(photo_id) ON DELETE CASCADE,
    FOREIGN KEY (taxon_id) REFERENCES taxa(taxon_id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS photo_taxon_usage (
    taxon_id INTEGER PRIMARY KEY,
    direct_photo_count INTEGER NOT NULL,
    subtree_photo_count INTEGER NOT NULL,
    CHECK (direct_photo_count >= 0),
    CHECK (subtree_photo_count >= direct_photo_count),
    FOREIGN KEY (taxon_id) REFERENCES taxa(taxon_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS photo_mapping_queue (
    photo_id INTEGER PRIMARY KEY,
    reason TEXT NOT NULL,
    CHECK (reason IN ('refresh', 'taxonomy', 'hook', 'settings')),
    FOREIGN KEY (photo_id) REFERENCES photos(photo_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS photo_operations (
    operation_id INTEGER PRIMARY KEY AUTOINCREMENT,
    source TEXT NOT NULL,
    root_path TEXT NOT NULL,
    input_json TEXT NOT NULL,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (source IN ('manual_rename', 'taxon_rename', 'taxon_selection_rename'))
);

CREATE TABLE IF NOT EXISTS photo_operation_items (
    operation_id INTEGER NOT NULL,
    row_number INTEGER NOT NULL,
    photo_id INTEGER NOT NULL,
    directory_relative_path TEXT NOT NULL,
    old_filename TEXT NOT NULL,
    new_filename TEXT NOT NULL,
    PRIMARY KEY (operation_id, row_number),
    CHECK (row_number > 0),
    CHECK (old_filename <> new_filename),
    FOREIGN KEY (operation_id) REFERENCES photo_operations(operation_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS taxa (
    taxon_id INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_taxon_id INTEGER,
    rank INTEGER NOT NULL,
    geological_range TEXT,
    CHECK (rank IN (1, 2, 3, 4, 5)),
    FOREIGN KEY (parent_taxon_id) REFERENCES taxa(taxon_id) ON DELETE RESTRICT
);

INSERT INTO sqlite_sequence(name, seq)
SELECT 'taxa', 8000000000000000
WHERE NOT EXISTS (SELECT 1 FROM sqlite_sequence WHERE name = 'taxa');
UPDATE sqlite_sequence
SET seq = max(seq, 8000000000000000)
WHERE name = 'taxa';

CREATE TRIGGER IF NOT EXISTS taxa_bd_photo_mapping
BEFORE DELETE ON taxa BEGIN
    INSERT INTO photo_mapping_queue (photo_id, reason)
    SELECT photo_id, 'taxonomy'
    FROM photo_taxon_mapping
    WHERE taxon_id = old.taxon_id
    ON CONFLICT(photo_id) DO UPDATE SET reason = excluded.reason;
    UPDATE photo_taxon_mapping
    SET taxon_id = NULL, status = 'stale'
    WHERE taxon_id = old.taxon_id;
END;

CREATE TABLE IF NOT EXISTS taxon_names (
    name_id INTEGER PRIMARY KEY AUTOINCREMENT,
    taxon_id INTEGER NOT NULL,
    name_type INTEGER NOT NULL,
    name TEXT NOT NULL,
    normalized_name TEXT GENERATED ALWAYS AS (lower(name)) STORED,
    authority_year TEXT,
    source TEXT,
    UNIQUE (taxon_id, name_type, name),
    CHECK (name_type BETWEEN 1 AND 6),
    CHECK (length(trim(name)) > 0),
    FOREIGN KEY (taxon_id) REFERENCES taxa(taxon_id) ON DELETE CASCADE
);

CREATE VIRTUAL TABLE IF NOT EXISTS taxon_names_fts USING fts5(
    name,
    content = 'taxon_names',
    content_rowid = 'name_id',
    tokenize = 'trigram'
);

CREATE TRIGGER IF NOT EXISTS taxon_names_ai AFTER INSERT ON taxon_names BEGIN
    INSERT INTO taxon_names_fts(rowid, name) VALUES (new.name_id, new.name);
END;

CREATE TRIGGER IF NOT EXISTS taxon_names_ad AFTER DELETE ON taxon_names BEGIN
    INSERT INTO taxon_names_fts(taxon_names_fts, rowid, name)
    VALUES ('delete', old.name_id, old.name);
END;

CREATE TRIGGER IF NOT EXISTS taxon_names_au AFTER UPDATE OF name ON taxon_names BEGIN
    INSERT INTO taxon_names_fts(taxon_names_fts, rowid, name)
    VALUES ('delete', old.name_id, old.name);
    INSERT INTO taxon_names_fts(rowid, name) VALUES (new.name_id, new.name);
END;

CREATE TABLE IF NOT EXISTS taxonomy_operations (
    operation_id INTEGER PRIMARY KEY AUTOINCREMENT,
    source TEXT NOT NULL,
    input_json TEXT NOT NULL,
    result_json TEXT NOT NULL,
    changeset_blob BLOB NOT NULL,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (source = 'formatted_update')
);

CREATE TABLE IF NOT EXISTS taxonomy_base_metadata (
    metadata_id INTEGER PRIMARY KEY CHECK (metadata_id = 1),
    source_path TEXT NOT NULL,
    taxa_count INTEGER NOT NULL,
    taxon_names_count INTEGER NOT NULL,
    imported_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (taxa_count >= 0),
    CHECK (taxon_names_count >= 0)
);

CREATE TABLE IF NOT EXISTS app_metadata (
    metadata_key TEXT PRIMARY KEY,
    metadata_value TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_taxon_names_one_sci_name
    ON taxon_names(taxon_id) WHERE name_type = 1;
CREATE UNIQUE INDEX IF NOT EXISTS idx_taxon_names_one_zh_name
    ON taxon_names(taxon_id) WHERE name_type = 3;
CREATE UNIQUE INDEX IF NOT EXISTS idx_taxon_names_one_en_name
    ON taxon_names(taxon_id) WHERE name_type = 5;
CREATE INDEX IF NOT EXISTS idx_taxa_parent ON taxa(parent_taxon_id);
CREATE INDEX IF NOT EXISTS idx_taxa_parent_rank_id ON taxa(parent_taxon_id, rank, taxon_id);
CREATE INDEX IF NOT EXISTS idx_taxa_rank ON taxa(rank);
CREATE INDEX IF NOT EXISTS idx_taxon_names_type_name ON taxon_names(name_type, name);
CREATE INDEX IF NOT EXISTS idx_taxon_names_type_taxon ON taxon_names(name_type, taxon_id);
CREATE INDEX IF NOT EXISTS idx_taxon_names_name ON taxon_names(name);
CREATE INDEX IF NOT EXISTS idx_taxon_names_name_search
    ON taxon_names(normalized_name, taxon_id);
CREATE INDEX IF NOT EXISTS idx_taxonomy_operations_applied
    ON taxonomy_operations(applied_at DESC, operation_id DESC);
CREATE INDEX IF NOT EXISTS idx_photo_directories_parent_name
    ON photo_directories(parent_directory_id, name, directory_id);
CREATE INDEX IF NOT EXISTS idx_photos_directory_filename
    ON photos(directory_id, filename, photo_id);
CREATE INDEX IF NOT EXISTS idx_photo_metadata_coordinates
    ON photo_metadata(latitude, longitude, photo_id);
CREATE INDEX IF NOT EXISTS idx_photo_taxon_mapping_taxon
    ON photo_taxon_mapping(taxon_id, photo_id);
CREATE INDEX IF NOT EXISTS idx_photo_taxon_mapping_status
    ON photo_taxon_mapping(status, photo_id);
CREATE INDEX IF NOT EXISTS idx_photo_taxon_usage_subtree
    ON photo_taxon_usage(subtree_photo_count, taxon_id);
CREATE INDEX IF NOT EXISTS idx_photo_mapping_queue_reason
    ON photo_mapping_queue(reason, photo_id);
CREATE INDEX IF NOT EXISTS idx_photo_operation_items_photo
    ON photo_operation_items(photo_id, operation_id);
CREATE INDEX IF NOT EXISTS idx_photo_operations_applied
    ON photo_operations(applied_at DESC, operation_id DESC);

PRAGMA user_version = 2;
"#;

pub(crate) fn photo_from_row(row: &Row<'_>) -> rusqlite::Result<Photo> {
    Ok(Photo {
        photo_id: row.get("photo_id")?,
        directory_id: row.get("directory_id")?,
        relative_path: row.get("relative_path")?,
        filename: row.get("filename")?,
        file_size: row.get("file_size")?,
        modified_at_ns: row.get("modified_at_ns")?,
        thumbnail_path: row.get("thumbnail_path")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_the_current_schema() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("vividarium.db")).unwrap();
        let connection = database.connect().unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        for table in [
            "photo_library",
            "photo_directories",
            "photos",
            "photo_metadata",
            "photo_filenames_fts",
            "photo_taxon_mapping",
            "photo_taxon_usage",
            "photo_mapping_queue",
            "photo_operations",
            "photo_operation_items",
            "taxa",
            "taxon_names",
            "taxon_names_fts",
            "taxonomy_operations",
            "taxonomy_base_metadata",
            "app_metadata",
        ] {
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?)",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "missing table {table}");
        }
        for key in ["photo_filename_hook_tests", "synonym_authority_hook_tests"] {
            let value: String = connection
                .query_row(
                    "SELECT metadata_value FROM app_metadata WHERE metadata_key = ?",
                    [key],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(value.starts_with('['), "missing metadata {key}");
        }
        let triggers = connection
            .prepare(
                r#"
                SELECT name
                FROM sqlite_master
                WHERE type = 'trigger' AND name LIKE 'taxa%photo_mapping%'
                ORDER BY name
                "#,
            )
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(triggers, ["taxa_bd_photo_mapping"]);
        let name_columns = table_columns(&connection, "taxon_names");
        assert_eq!(
            name_columns,
            [
                "name_id",
                "taxon_id",
                "name_type",
                "name",
                "authority_year",
                "source"
            ]
        );
        let name_columns = table_xcolumns(&connection, "taxon_names");
        assert!(name_columns.contains(&"normalized_name".to_string()));
        let name_type_storage: String = connection
            .query_row(
                "SELECT type FROM pragma_table_info('taxon_names') WHERE name = 'name_type'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(name_type_storage, "INTEGER");
        let taxon_names_schema: String = connection
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'taxon_names'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(taxon_names_schema.contains("name_type INTEGER NOT NULL"));
        assert!(taxon_names_schema.contains("CHECK (name_type BETWEEN 1 AND 6)"));
        assert!(!taxon_names_schema.contains("name_type TEXT"));
        for (index_name, name_type) in [
            ("idx_taxon_names_one_sci_name", 1),
            ("idx_taxon_names_one_zh_name", 3),
            ("idx_taxon_names_one_en_name", 5),
        ] {
            let index_schema: String = connection
                .query_row(
                    "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?",
                    [index_name],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(
                index_schema.contains(&format!("WHERE name_type = {name_type}")),
                "index {index_name} does not use integer name_type {name_type}"
            );
        }
        let photo_columns = table_columns(&connection, "photos");
        assert_eq!(
            photo_columns,
            [
                "photo_id",
                "directory_id",
                "filename",
                "file_size",
                "modified_at_ns",
                "thumbnail_path",
            ]
        );
        let operation_columns = table_columns(&connection, "taxonomy_operations");
        assert_eq!(
            operation_columns,
            [
                "operation_id",
                "source",
                "input_json",
                "result_json",
                "changeset_blob",
                "applied_at",
            ]
        );
        let operation_columns = table_columns(&connection, "photo_operations");
        assert_eq!(
            operation_columns,
            [
                "operation_id",
                "source",
                "root_path",
                "input_json",
                "applied_at",
            ]
        );
        let item_columns = table_columns(&connection, "photo_operation_items");
        assert_eq!(
            item_columns,
            [
                "operation_id",
                "row_number",
                "photo_id",
                "directory_relative_path",
                "old_filename",
                "new_filename",
            ]
        );
    }

    #[test]
    fn rejects_a_second_sci_name() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("vividarium.db")).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute("INSERT INTO taxa (rank) VALUES (5)", [])
            .unwrap();
        let taxon_id = connection.last_insert_rowid();
        connection
            .execute(
                "INSERT INTO taxon_names (taxon_id, name_type, name) VALUES (?, 1, 'A a')",
                [taxon_id],
            )
            .unwrap();
        let result = connection.execute(
            "INSERT INTO taxon_names (taxon_id, name_type, name) VALUES (?, 1, 'A b')",
            [taxon_id],
        );
        assert!(result.is_err());
    }

    #[test]
    fn refuses_to_open_different_schema_versions() {
        for version in [1, 3] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("phytoindex.db");
            let connection = Connection::open(&path).unwrap();
            connection
                .execute_batch(&format!("PRAGMA user_version = {version};"))
                .unwrap();
            drop(connection);
            let error = Database::open(path).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("unsupported database schema version")
            );
        }
    }

    fn table_columns(connection: &Connection, table: &str) -> Vec<String> {
        let mut statement = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap();
        statement
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    fn table_xcolumns(connection: &Connection, table: &str) -> Vec<String> {
        let mut statement = connection
            .prepare(&format!("PRAGMA table_xinfo({table})"))
            .unwrap();
        statement
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }
}
