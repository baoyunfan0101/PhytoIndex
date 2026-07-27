use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, Row, params};
use uuid::Uuid;

use crate::error::{CoreError, CoreResult};
use crate::models::{DatabaseLocations, Photo, PhotoLibraryLocation, PhotoLibraryRegistration};

pub const SCHEMA_VERSION: i64 = 2;
pub(crate) const LOCAL_TAXON_ID_FLOOR: i64 = 8_000_000_000_000_000;

const DEFAULT_TAXONOMY_FILENAME: &str = "taxonomy.db";
const DEFAULT_LIBRARY_DIRECTORY: &str = "photo-libraries";
const DEFAULT_LIBRARY_FILENAME: &str = "default.db";

#[derive(Debug)]
struct DatabasePaths {
    metadata: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Database {
    paths: Arc<RwLock<DatabasePaths>>,
}

impl Database {
    pub fn open(metadata_path: impl Into<PathBuf>) -> CoreResult<Self> {
        let metadata_path = metadata_path.into();
        if let Some(parent) = metadata_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let database = Self {
            paths: Arc::new(RwLock::new(DatabasePaths {
                metadata: metadata_path,
            })),
        };
        database.initialize()?;
        Ok(database)
    }

    pub fn path(&self) -> PathBuf {
        self.taxonomy_path()
            .unwrap_or_else(|_| self.metadata_path())
    }

    pub fn metadata_path(&self) -> PathBuf {
        self.paths
            .read()
            .expect("database path lock poisoned")
            .metadata
            .clone()
    }

    pub fn taxonomy_path(&self) -> CoreResult<PathBuf> {
        let connection = self.connect_metadata()?;
        connection
            .query_row(
                "SELECT taxonomy_db_path FROM storage_settings WHERE settings_id = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .map(PathBuf::from)
            .map_err(Into::into)
    }

    pub fn active_photo_library(&self) -> CoreResult<Option<PhotoLibraryRegistration>> {
        let connection = self.connect_metadata()?;
        connection
            .query_row(
                r#"
                SELECT libraries.library_uuid, libraries.display_name,
                       libraries.root_path, libraries.db_path,
                       libraries.last_opened_at
                FROM active_photo_library
                JOIN photo_libraries AS libraries USING (library_uuid)
                WHERE active_photo_library.active_id = 1
                "#,
                [],
                photo_library_registration_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn locations(&self) -> CoreResult<DatabaseLocations> {
        let connection = self.connect_metadata()?;
        let (taxonomy_database, default_taxonomy_directory, default_photo_library_directory) =
            connection.query_row(
                r#"
            SELECT taxonomy_db_path, default_taxonomy_directory,
                   default_photo_library_directory
            FROM storage_settings
            WHERE settings_id = 1
            "#,
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )?;
        let active_photo_library_uuid = connection
            .query_row(
                "SELECT library_uuid FROM active_photo_library WHERE active_id = 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(DatabaseLocations {
            metadata_database: path_string(&self.metadata_path()),
            taxonomy_database,
            default_taxonomy_directory,
            default_photo_library_directory,
            active_photo_library_uuid,
        })
    }

    pub fn list_photo_libraries(&self) -> CoreResult<Vec<PhotoLibraryRegistration>> {
        let connection = self.connect_metadata()?;
        let mut statement = connection.prepare(
            r#"
            SELECT library_uuid, display_name, root_path, db_path, last_opened_at
            FROM photo_libraries
            ORDER BY display_name COLLATE NOCASE, library_uuid
            "#,
        )?;
        let rows = statement.query_map([], photo_library_registration_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn register_photo_library(
        &self,
        root_path: &Path,
        database_path: &Path,
        display_name: Option<&str>,
    ) -> CoreResult<PhotoLibraryRegistration> {
        let root_path = canonical_or_absolute(root_path)?;
        let database_path = absolute_path(database_path)?;
        let library_uuid = if database_path.exists() {
            initialize_file(&database_path, PHOTO_SCHEMA)?;
            read_photo_library_uuid(&database_path)?.unwrap_or_else(new_uuid)
        } else {
            if let Some(parent) = database_path.parent() {
                fs::create_dir_all(parent)?;
            }
            initialize_file(&database_path, PHOTO_SCHEMA)?;
            new_uuid()
        };
        let display_name = display_name
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| {
                root_path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "Photo Library".into());
        let taxonomy_identity = self.taxonomy_identity()?;
        {
            let connection = open_connection(&database_path)?;
            connection.execute(
                r#"
                INSERT INTO photo_library (
                    library_id, library_uuid, root_path,
                    bound_taxonomy_identity, last_taxonomy_sync_id
                ) VALUES (1, ?, ?, ?, 0)
                ON CONFLICT(library_id) DO UPDATE SET
                    library_uuid = excluded.library_uuid,
                    root_path = excluded.root_path
                "#,
                params![library_uuid, path_string(&root_path), taxonomy_identity],
            )?;
            ensure_photo_root(&connection)?;
        }
        let connection = self.connect_metadata()?;
        connection.execute(
            r#"
            INSERT INTO photo_libraries (
                library_uuid, display_name, root_path, db_path, last_opened_at
            ) VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP)
            ON CONFLICT(library_uuid) DO UPDATE SET
                display_name = excluded.display_name,
                root_path = excluded.root_path,
                db_path = excluded.db_path,
                last_opened_at = excluded.last_opened_at
            "#,
            params![
                library_uuid,
                display_name,
                path_string(&root_path),
                path_string(&database_path)
            ],
        )?;
        self.switch_photo_library(&library_uuid)
    }

    pub fn switch_photo_library(&self, library_uuid: &str) -> CoreResult<PhotoLibraryRegistration> {
        let mut connection = self.connect_metadata()?;
        let transaction = connection.transaction()?;
        let library = transaction
            .query_row(
                r#"
                SELECT library_uuid, display_name, root_path, db_path, last_opened_at
                FROM photo_libraries
                WHERE library_uuid = ?
                "#,
                [library_uuid],
                photo_library_registration_from_row,
            )
            .optional()?
            .ok_or_else(|| CoreError::NotFound(format!("photo library {library_uuid}")))?;
        initialize_file(Path::new(&library.db_path), PHOTO_SCHEMA)?;
        transaction.execute(
            r#"
            INSERT INTO active_photo_library (active_id, library_uuid)
            VALUES (1, ?)
            ON CONFLICT(active_id) DO UPDATE SET library_uuid = excluded.library_uuid
            "#,
            [library_uuid],
        )?;
        transaction.execute(
            "UPDATE photo_libraries SET last_opened_at = CURRENT_TIMESTAMP WHERE library_uuid = ?",
            [library_uuid],
        )?;
        transaction.commit()?;
        crate::taxonomy::sync::synchronize_photo_library(self, &library)?;
        crate::taxonomy::sync::cleanup_consumed_events(self)?;
        self.active_photo_library()?
            .ok_or_else(|| CoreError::Consistency("active photo library was not stored".into()))
    }

    pub fn remove_photo_library(&self, library_uuid: &str) -> CoreResult<()> {
        let mut connection = self.connect_metadata()?;
        let transaction = connection.transaction()?;
        let active = transaction
            .query_row(
                "SELECT library_uuid FROM active_photo_library WHERE active_id = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if active.as_deref() == Some(library_uuid) {
            return Err(CoreError::InvalidArgument(
                "cannot remove the active photo library registration".into(),
            ));
        }
        let removed = transaction.execute(
            "DELETE FROM photo_libraries WHERE library_uuid = ?",
            [library_uuid],
        )?;
        if removed == 0 {
            return Err(CoreError::NotFound(format!("photo library {library_uuid}")));
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn rebind_photo_library_root(
        &self,
        library_uuid: &str,
        root_path: &Path,
    ) -> CoreResult<PhotoLibraryRegistration> {
        let root_path = canonical_or_absolute(root_path)?;
        let library = self.photo_library(library_uuid)?;
        let connection = open_connection(Path::new(&library.db_path))?;
        connection.execute(
            "UPDATE photo_library SET root_path = ? WHERE library_id = 1",
            [path_string(&root_path)],
        )?;
        let metadata = self.connect_metadata()?;
        metadata.execute(
            "UPDATE photo_libraries SET root_path = ? WHERE library_uuid = ?",
            params![path_string(&root_path), library_uuid],
        )?;
        self.photo_library(library_uuid)
    }

    pub fn relocate_photo_library_database(
        &self,
        library_uuid: &str,
        destination: &Path,
    ) -> CoreResult<PhotoLibraryRegistration> {
        let library = self.photo_library(library_uuid)?;
        let source = PathBuf::from(&library.db_path);
        let destination = absolute_path(destination)?;
        move_database_file(&source, &destination, PHOTO_SCHEMA)?;
        let connection = self.connect_metadata()?;
        if let Err(error) = connection.execute(
            "UPDATE photo_libraries SET db_path = ? WHERE library_uuid = ?",
            params![path_string(&destination), library_uuid],
        ) {
            let _ = move_database_file(&destination, &source, PHOTO_SCHEMA);
            return Err(error.into());
        }
        self.photo_library(library_uuid)
    }

    pub fn relocate_taxonomy_database(&self, destination: &Path) -> CoreResult<DatabaseLocations> {
        let source = self.taxonomy_path()?;
        let destination = absolute_path(destination)?;
        move_database_file(&source, &destination, TAXONOMY_SCHEMA)?;
        let connection = self.connect_metadata()?;
        if let Err(error) = connection.execute(
            "UPDATE storage_settings SET taxonomy_db_path = ? WHERE settings_id = 1",
            [path_string(&destination)],
        ) {
            let _ = move_database_file(&destination, &source, TAXONOMY_SCHEMA);
            return Err(error.into());
        }
        self.locations()
    }

    pub fn set_default_taxonomy_directory(
        &self,
        directory: &Path,
    ) -> CoreResult<DatabaseLocations> {
        let directory = absolute_path(directory)?;
        fs::create_dir_all(&directory)?;
        self.connect_metadata()?.execute(
            "UPDATE storage_settings SET default_taxonomy_directory = ? WHERE settings_id = 1",
            [path_string(&directory)],
        )?;
        self.locations()
    }

    pub fn set_default_photo_library_directory(
        &self,
        directory: &Path,
    ) -> CoreResult<DatabaseLocations> {
        let directory = absolute_path(directory)?;
        fs::create_dir_all(&directory)?;
        self.connect_metadata()?.execute(
            r#"
            UPDATE storage_settings
            SET default_photo_library_directory = ?
            WHERE settings_id = 1
            "#,
            [path_string(&directory)],
        )?;
        self.locations()
    }

    pub fn connect(&self) -> CoreResult<Connection> {
        self.connect_photo_library()
    }

    pub fn connect_metadata(&self) -> CoreResult<Connection> {
        open_connection(&self.metadata_path())
    }

    pub fn connect_taxonomy(&self) -> CoreResult<Connection> {
        open_connection(&self.taxonomy_path()?)
    }

    pub(crate) fn connect_taxonomy_context(&self) -> CoreResult<Connection> {
        let connection = self.connect_taxonomy()?;
        attach_database(&connection, &self.metadata_path(), "metadata")?;
        if let Some(library) = self.active_photo_library()? {
            attach_database(
                &connection,
                Path::new(&library.db_path),
                "active_photo_library",
            )?;
            create_cross_database_views(&connection)?;
        }
        Ok(connection)
    }

    pub fn connect_photo_library(&self) -> CoreResult<Connection> {
        let library = self.active_photo_library()?.ok_or_else(|| {
            CoreError::InvalidArgument("no active photo library is registered".into())
        })?;
        self.connect_photo_library_registration(&library)
    }

    pub(crate) fn connect_photo_library_registration(
        &self,
        library: &PhotoLibraryRegistration,
    ) -> CoreResult<Connection> {
        let connection = open_connection(Path::new(&library.db_path))?;
        attach_database(&connection, &self.taxonomy_path()?, "taxonomy")?;
        attach_database(&connection, &self.metadata_path(), "metadata")?;
        create_photo_main_views(&connection)?;
        Ok(connection)
    }

    pub(crate) fn taxonomy_identity(&self) -> CoreResult<String> {
        self.connect_taxonomy()?
            .query_row(
                "SELECT taxonomy_identity FROM taxonomy_identity WHERE identity_id = 1",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub(crate) fn photo_library(&self, library_uuid: &str) -> CoreResult<PhotoLibraryRegistration> {
        self.connect_metadata()?
            .query_row(
                r#"
                SELECT library_uuid, display_name, root_path, db_path, last_opened_at
                FROM photo_libraries
                WHERE library_uuid = ?
                "#,
                [library_uuid],
                photo_library_registration_from_row,
            )
            .optional()?
            .ok_or_else(|| CoreError::NotFound(format!("photo library {library_uuid}")))
    }

    fn initialize(&self) -> CoreResult<()> {
        initialize_file(&self.metadata_path(), METADATA_SCHEMA)?;
        let parent = self
            .metadata_path()
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let taxonomy_directory = parent.clone();
        let photo_directory = parent.join(DEFAULT_LIBRARY_DIRECTORY);
        fs::create_dir_all(&photo_directory)?;
        {
            let connection = self.connect_metadata()?;
            connection.execute(
                r#"
                INSERT INTO storage_settings (
                    settings_id, taxonomy_db_path,
                    default_taxonomy_directory,
                    default_photo_library_directory
                ) VALUES (1, ?, ?, ?)
                ON CONFLICT(settings_id) DO NOTHING
                "#,
                params![
                    path_string(&taxonomy_directory.join(DEFAULT_TAXONOMY_FILENAME)),
                    path_string(&taxonomy_directory),
                    path_string(&photo_directory)
                ],
            )?;
        }
        initialize_file(&self.taxonomy_path()?, TAXONOMY_SCHEMA)?;
        self.ensure_taxonomy_identity()?;
        self.ensure_default_library(&photo_directory)?;
        self.seed_metadata()?;
        crate::taxonomy::sync::synchronize_all_photo_libraries(self)?;
        Ok(())
    }

    fn ensure_taxonomy_identity(&self) -> CoreResult<()> {
        let connection = self.connect_taxonomy()?;
        connection.execute(
            r#"
            INSERT INTO taxonomy_identity (identity_id, taxonomy_identity)
            VALUES (1, ?)
            ON CONFLICT(identity_id) DO NOTHING
            "#,
            [new_uuid()],
        )?;
        Ok(())
    }

    fn ensure_default_library(&self, directory: &Path) -> CoreResult<()> {
        if self.active_photo_library()?.is_some() {
            return Ok(());
        }
        let database_path = directory.join(DEFAULT_LIBRARY_FILENAME);
        initialize_file(&database_path, PHOTO_SCHEMA)?;
        let existing_uuid = read_photo_library_uuid(&database_path)?;
        let library_uuid = existing_uuid.unwrap_or_else(new_uuid);
        let taxonomy_identity = self.taxonomy_identity()?;
        {
            let connection = open_connection(&database_path)?;
            connection.execute(
                r#"
                INSERT INTO photo_library (
                    library_id, library_uuid, root_path,
                    bound_taxonomy_identity, last_taxonomy_sync_id
                ) VALUES (1, ?, '', ?, 0)
                ON CONFLICT(library_id) DO NOTHING
                "#,
                params![library_uuid, taxonomy_identity],
            )?;
        }
        let connection = self.connect_metadata()?;
        connection.execute(
            r#"
            INSERT INTO photo_libraries (
                library_uuid, display_name, root_path, db_path
            ) VALUES (?, 'Default', '', ?)
            ON CONFLICT(library_uuid) DO NOTHING
            "#,
            params![library_uuid, path_string(&database_path)],
        )?;
        connection.execute(
            r#"
            INSERT INTO active_photo_library (active_id, library_uuid)
            VALUES (1, ?)
            ON CONFLICT(active_id) DO UPDATE SET library_uuid = excluded.library_uuid
            "#,
            [library_uuid],
        )?;
        Ok(())
    }

    fn seed_metadata(&self) -> CoreResult<()> {
        let connection = self.connect_metadata()?;
        crate::metadata::insert_raw_if_missing(
            &connection,
            crate::metadata::MetadataKey::TaxonomyNameSeparator,
            ";",
        )?;
        crate::naming::seed_default_test_cases(&connection)
    }
}

fn initialize_file(path: &Path, schema: &str) -> CoreResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let connection = open_connection(path)?;
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    match version {
        0 => connection.execute_batch(schema)?,
        SCHEMA_VERSION => {}
        _ => {
            return Err(CoreError::InvalidArgument(format!(
                "unsupported database schema version: {version}; expected {SCHEMA_VERSION}"
            )));
        }
    }
    Ok(())
}

fn open_connection(path: &Path) -> CoreResult<Connection> {
    let connection = Connection::open(path)?;
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

fn attach_database(connection: &Connection, path: &Path, schema: &str) -> CoreResult<()> {
    if !schema
        .bytes()
        .all(|value| value.is_ascii_alphanumeric() || value == b'_')
    {
        return Err(CoreError::InvalidArgument(
            "invalid attached database schema name".into(),
        ));
    }
    connection.execute(
        &format!("ATTACH DATABASE ? AS {schema}"),
        [path_string(path)],
    )?;
    Ok(())
}

fn create_cross_database_views(connection: &Connection) -> CoreResult<()> {
    connection.execute_batch(
        r#"
        CREATE TEMP VIEW current_photo_taxon_mapping AS
        SELECT mapping.photo_id, mapping.taxon_id
        FROM active_photo_library.photo_taxon_mapping AS mapping
        WHERE mapping.status = 'matched'
          AND NOT EXISTS (
              SELECT 1
              FROM active_photo_library.photo_mapping_queue AS queue
              WHERE queue.photo_id = mapping.photo_id
          );

        CREATE TEMP VIEW current_photo_taxon_usage AS
        WITH RECURSIVE queued_taxon_paths(direct_taxon_id, taxon_id) AS (
            SELECT mapping.taxon_id, mapping.taxon_id
            FROM active_photo_library.photo_taxon_mapping AS mapping
            JOIN active_photo_library.photo_mapping_queue AS queue
              ON queue.photo_id = mapping.photo_id
            WHERE mapping.status = 'matched'
            UNION ALL
            SELECT paths.direct_taxon_id, taxa.parent_taxon_id
            FROM queued_taxon_paths AS paths
            JOIN main.taxa AS taxa ON taxa.taxon_id = paths.taxon_id
            WHERE taxa.parent_taxon_id IS NOT NULL
        ),
        queued_taxon_counts AS (
            SELECT taxon_id,
                   SUM(direct_taxon_id = taxon_id) AS direct_photo_count,
                   COUNT(*) AS subtree_photo_count
            FROM queued_taxon_paths
            GROUP BY taxon_id
        )
        SELECT usage.taxon_id,
               usage.direct_photo_count
                   - COALESCE(counts.direct_photo_count, 0) AS direct_photo_count,
               usage.subtree_photo_count
                   - COALESCE(counts.subtree_photo_count, 0) AS subtree_photo_count
        FROM active_photo_library.photo_taxon_usage AS usage
        LEFT JOIN queued_taxon_counts AS counts USING (taxon_id)
        WHERE usage.subtree_photo_count
              > COALESCE(counts.subtree_photo_count, 0);
        "#,
    )?;
    Ok(())
}

fn create_photo_main_views(connection: &Connection) -> CoreResult<()> {
    connection.execute_batch(
        r#"
        CREATE TEMP VIEW current_photo_taxon_mapping AS
        SELECT mapping.photo_id, mapping.taxon_id
        FROM main.photo_taxon_mapping AS mapping
        WHERE mapping.status = 'matched'
          AND NOT EXISTS (
              SELECT 1 FROM main.photo_mapping_queue AS queue
              WHERE queue.photo_id = mapping.photo_id
          );

        CREATE TEMP VIEW current_photo_taxon_usage AS
        WITH RECURSIVE queued_taxon_paths(direct_taxon_id, taxon_id) AS (
            SELECT mapping.taxon_id, mapping.taxon_id
            FROM main.photo_taxon_mapping AS mapping
            JOIN main.photo_mapping_queue AS queue USING (photo_id)
            WHERE mapping.status = 'matched'
            UNION ALL
            SELECT paths.direct_taxon_id, taxa.parent_taxon_id
            FROM queued_taxon_paths AS paths
            JOIN taxonomy.taxa AS taxa ON taxa.taxon_id = paths.taxon_id
            WHERE taxa.parent_taxon_id IS NOT NULL
        ),
        queued_taxon_counts AS (
            SELECT taxon_id,
                   SUM(direct_taxon_id = taxon_id) AS direct_photo_count,
                   COUNT(*) AS subtree_photo_count
            FROM queued_taxon_paths
            GROUP BY taxon_id
        )
        SELECT usage.taxon_id,
               usage.direct_photo_count
                   - COALESCE(counts.direct_photo_count, 0) AS direct_photo_count,
               usage.subtree_photo_count
                   - COALESCE(counts.subtree_photo_count, 0) AS subtree_photo_count
        FROM main.photo_taxon_usage AS usage
        LEFT JOIN queued_taxon_counts AS counts USING (taxon_id)
        WHERE usage.subtree_photo_count
              > COALESCE(counts.subtree_photo_count, 0);
        "#,
    )?;
    Ok(())
}

fn read_photo_library_uuid(path: &Path) -> CoreResult<Option<String>> {
    let connection = open_connection(path)?;
    connection
        .query_row(
            "SELECT library_uuid FROM photo_library WHERE library_id = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
}

fn ensure_photo_root(connection: &Connection) -> CoreResult<()> {
    connection.execute(
        r#"
        INSERT INTO photo_directories (
            parent_directory_id, name, relative_path
        ) VALUES (NULL, '', '')
        ON CONFLICT(relative_path) DO NOTHING
        "#,
        [],
    )?;
    Ok(())
}

fn move_database_file(source: &Path, destination: &Path, schema: &str) -> CoreResult<()> {
    if source == destination {
        return Ok(());
    }
    if destination.exists() {
        return Err(CoreError::InvalidArgument(format!(
            "database destination already exists: {}",
            destination.display()
        )));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination)?;
    if let Err(error) = initialize_file(destination, schema) {
        let _ = fs::remove_file(destination);
        return Err(error);
    }
    if let Err(error) = fs::remove_file(source) {
        let _ = fs::remove_file(destination);
        return Err(error.into());
    }
    Ok(())
}

fn canonical_or_absolute(path: &Path) -> CoreResult<PathBuf> {
    if path.exists() {
        Ok(fs::canonicalize(path)?)
    } else {
        absolute_path(path)
    }
}

fn absolute_path(path: &Path) -> CoreResult<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn new_uuid() -> String {
    Uuid::new_v4().to_string()
}

fn photo_library_registration_from_row(
    row: &Row<'_>,
) -> rusqlite::Result<PhotoLibraryRegistration> {
    Ok(PhotoLibraryRegistration {
        library_uuid: row.get(0)?,
        display_name: row.get(1)?,
        root_path: row.get(2)?,
        db_path: row.get(3)?,
        last_opened_at: row.get(4)?,
    })
}

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

const METADATA_SCHEMA: &str = r#"
CREATE TABLE storage_settings (
    settings_id INTEGER PRIMARY KEY CHECK (settings_id = 1),
    taxonomy_db_path TEXT NOT NULL,
    default_taxonomy_directory TEXT NOT NULL,
    default_photo_library_directory TEXT NOT NULL
);

CREATE TABLE photo_libraries (
    library_uuid TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    root_path TEXT NOT NULL,
    db_path TEXT NOT NULL UNIQUE,
    last_opened_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (length(library_uuid) > 0),
    CHECK (length(display_name) > 0)
);

CREATE TABLE active_photo_library (
    active_id INTEGER PRIMARY KEY CHECK (active_id = 1),
    library_uuid TEXT NOT NULL UNIQUE,
    FOREIGN KEY (library_uuid)
        REFERENCES photo_libraries(library_uuid) ON DELETE RESTRICT
);

CREATE TABLE app_metadata (
    metadata_key TEXT PRIMARY KEY,
    metadata_value TEXT NOT NULL
);

PRAGMA user_version = 2;
"#;

const TAXONOMY_SCHEMA: &str = r#"
CREATE TABLE taxonomy_identity (
    identity_id INTEGER PRIMARY KEY CHECK (identity_id = 1),
    taxonomy_identity TEXT NOT NULL UNIQUE
);

CREATE TABLE taxa (
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

CREATE TABLE taxon_names (
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

CREATE VIRTUAL TABLE taxon_names_fts USING fts5(
    name,
    content = 'taxon_names',
    content_rowid = 'name_id',
    tokenize = 'trigram'
);

CREATE TRIGGER taxon_names_ai AFTER INSERT ON taxon_names BEGIN
    INSERT INTO taxon_names_fts(rowid, name) VALUES (new.name_id, new.name);
END;

CREATE TRIGGER taxon_names_ad AFTER DELETE ON taxon_names BEGIN
    INSERT INTO taxon_names_fts(taxon_names_fts, rowid, name)
    VALUES ('delete', old.name_id, old.name);
END;

CREATE TRIGGER taxon_names_au AFTER UPDATE OF name ON taxon_names BEGIN
    INSERT INTO taxon_names_fts(taxon_names_fts, rowid, name)
    VALUES ('delete', old.name_id, old.name);
    INSERT INTO taxon_names_fts(rowid, name) VALUES (new.name_id, new.name);
END;

CREATE TABLE operations (
    operation_id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL,
    source TEXT NOT NULL,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    total_items INTEGER NOT NULL,
    succeeded_items INTEGER NOT NULL,
    failed_items INTEGER NOT NULL,
    rollbackable INTEGER NOT NULL,
    has_formatted_input INTEGER NOT NULL,
    CHECK (total_items >= 0),
    CHECK (succeeded_items >= 0),
    CHECK (failed_items >= 0),
    CHECK (succeeded_items + failed_items = total_items),
    CHECK (rollbackable IN (0, 1)),
    CHECK (has_formatted_input IN (0, 1))
);

CREATE TABLE operation_audit_rows (
    operation_id INTEGER NOT NULL,
    sequence INTEGER NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT,
    action TEXT NOT NULL,
    before_json TEXT,
    after_json TEXT,
    succeeded INTEGER NOT NULL,
    message TEXT NOT NULL,
    PRIMARY KEY (operation_id, sequence),
    CHECK (sequence > 0),
    CHECK (succeeded IN (0, 1)),
    FOREIGN KEY (operation_id) REFERENCES operations(operation_id) ON DELETE CASCADE
);

CREATE TABLE operation_changesets (
    operation_id INTEGER PRIMARY KEY,
    changeset_blob BLOB NOT NULL,
    FOREIGN KEY (operation_id) REFERENCES operations(operation_id) ON DELETE CASCADE
);

CREATE TABLE operation_formatted_inputs (
    operation_id INTEGER NOT NULL,
    sequence INTEGER NOT NULL,
    input_json TEXT NOT NULL,
    PRIMARY KEY (operation_id, sequence),
    CHECK (sequence > 0),
    FOREIGN KEY (operation_id) REFERENCES operations(operation_id) ON DELETE CASCADE
);

CREATE TABLE taxonomy_base_metadata (
    metadata_id INTEGER PRIMARY KEY CHECK (metadata_id = 1),
    source_path TEXT NOT NULL,
    taxa_count INTEGER NOT NULL,
    taxon_names_count INTEGER NOT NULL,
    imported_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (taxa_count >= 0),
    CHECK (taxon_names_count >= 0)
);

CREATE TABLE taxonomy_sync_events (
    sync_id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_operation_id INTEGER,
    full_remap_required INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (full_remap_required IN (0, 1))
);

CREATE TABLE taxonomy_sync_event_taxa (
    sync_id INTEGER NOT NULL,
    taxon_id INTEGER NOT NULL,
    PRIMARY KEY (sync_id, taxon_id),
    FOREIGN KEY (sync_id)
        REFERENCES taxonomy_sync_events(sync_id) ON DELETE CASCADE
) WITHOUT ROWID;

CREATE UNIQUE INDEX idx_taxon_names_one_sci_name
    ON taxon_names(taxon_id) WHERE name_type = 1;
CREATE UNIQUE INDEX idx_taxon_names_one_zh_name
    ON taxon_names(taxon_id) WHERE name_type = 3;
CREATE UNIQUE INDEX idx_taxon_names_one_en_name
    ON taxon_names(taxon_id) WHERE name_type = 5;
CREATE INDEX idx_taxa_parent ON taxa(parent_taxon_id);
CREATE INDEX idx_taxa_parent_rank_id ON taxa(parent_taxon_id, rank, taxon_id);
CREATE INDEX idx_taxa_rank ON taxa(rank);
CREATE INDEX idx_taxon_names_type_name ON taxon_names(name_type, name);
CREATE INDEX idx_taxon_names_type_taxon ON taxon_names(name_type, taxon_id);
CREATE INDEX idx_taxon_names_name ON taxon_names(name);
CREATE INDEX idx_taxon_names_name_search
    ON taxon_names(normalized_name, taxon_id);
CREATE INDEX idx_operations_applied
    ON operations(applied_at DESC, operation_id DESC);
CREATE INDEX idx_operation_audit_entity
    ON operation_audit_rows(entity_type, entity_id, operation_id, sequence);
CREATE INDEX idx_taxonomy_sync_events_created
    ON taxonomy_sync_events(sync_id);

PRAGMA user_version = 2;
"#;

const PHOTO_SCHEMA: &str = r#"
CREATE TABLE photo_library (
    library_id INTEGER PRIMARY KEY CHECK (library_id = 1),
    library_uuid TEXT NOT NULL UNIQUE,
    root_path TEXT NOT NULL,
    bound_taxonomy_identity TEXT NOT NULL,
    last_taxonomy_sync_id INTEGER NOT NULL DEFAULT 0,
    CHECK (last_taxonomy_sync_id >= 0)
);

CREATE TABLE photo_directories (
    directory_id INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_directory_id INTEGER,
    name TEXT NOT NULL,
    relative_path TEXT NOT NULL UNIQUE,
    UNIQUE (parent_directory_id, name),
    FOREIGN KEY (parent_directory_id)
        REFERENCES photo_directories(directory_id) ON DELETE CASCADE
);

CREATE TABLE photos (
    photo_id INTEGER PRIMARY KEY AUTOINCREMENT,
    directory_id INTEGER NOT NULL,
    filename TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    modified_at_ns INTEGER NOT NULL,
    thumbnail_path TEXT,
    UNIQUE (directory_id, filename),
    CHECK (length(filename) > 0),
    CHECK (file_size >= 0),
    FOREIGN KEY (directory_id)
        REFERENCES photo_directories(directory_id) ON DELETE CASCADE
);

CREATE TABLE photo_metadata (
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

CREATE VIRTUAL TABLE photo_filenames_fts USING fts5(
    filename,
    content = 'photos',
    content_rowid = 'photo_id',
    tokenize = 'trigram'
);

CREATE TRIGGER photos_ai AFTER INSERT ON photos BEGIN
    INSERT INTO photo_filenames_fts(rowid, filename)
    VALUES (new.photo_id, new.filename);
END;

CREATE TRIGGER photos_ad AFTER DELETE ON photos BEGIN
    INSERT INTO photo_filenames_fts(photo_filenames_fts, rowid, filename)
    VALUES ('delete', old.photo_id, old.filename);
END;

CREATE TRIGGER photos_au AFTER UPDATE OF filename ON photos BEGIN
    INSERT INTO photo_filenames_fts(photo_filenames_fts, rowid, filename)
    VALUES ('delete', old.photo_id, old.filename);
    INSERT INTO photo_filenames_fts(rowid, filename)
    VALUES (new.photo_id, new.filename);
END;

CREATE TABLE photo_taxon_mapping (
    photo_id INTEGER PRIMARY KEY,
    taxon_id INTEGER,
    status TEXT NOT NULL,
    CHECK (status IN ('matched', 'unmatched', 'ambiguous')),
    CHECK ((status = 'matched' AND taxon_id IS NOT NULL)
        OR (status != 'matched' AND taxon_id IS NULL)),
    FOREIGN KEY (photo_id) REFERENCES photos(photo_id) ON DELETE CASCADE
);

CREATE TABLE photo_taxon_candidates (
    photo_id INTEGER NOT NULL,
    taxon_id INTEGER NOT NULL,
    PRIMARY KEY (photo_id, taxon_id),
    FOREIGN KEY (photo_id)
        REFERENCES photo_taxon_mapping(photo_id) ON DELETE CASCADE
) WITHOUT ROWID;

CREATE TABLE photo_taxon_candidate_names (
    photo_id INTEGER NOT NULL,
    taxon_id INTEGER NOT NULL,
    name_id INTEGER NOT NULL,
    name_type INTEGER NOT NULL,
    name TEXT NOT NULL,
    PRIMARY KEY (photo_id, taxon_id, name_id),
    CHECK (name_type BETWEEN 1 AND 6),
    CHECK (length(trim(name)) > 0),
    FOREIGN KEY (photo_id, taxon_id)
        REFERENCES photo_taxon_candidates(photo_id, taxon_id) ON DELETE CASCADE
) WITHOUT ROWID;

CREATE TRIGGER photo_taxon_candidates_bi
BEFORE INSERT ON photo_taxon_candidates
WHEN NOT EXISTS (
    SELECT 1
    FROM photo_taxon_mapping
    WHERE photo_id = new.photo_id
      AND status = 'ambiguous'
) BEGIN
    SELECT RAISE(ABORT, 'photo candidates require an ambiguous mapping');
END;

CREATE TRIGGER photo_taxon_mapping_au_candidates
AFTER UPDATE OF status ON photo_taxon_mapping
WHEN new.status != 'ambiguous' BEGIN
    DELETE FROM photo_taxon_candidates WHERE photo_id = new.photo_id;
END;

CREATE TABLE photo_taxon_usage (
    taxon_id INTEGER PRIMARY KEY,
    direct_photo_count INTEGER NOT NULL,
    subtree_photo_count INTEGER NOT NULL,
    CHECK (direct_photo_count >= 0),
    CHECK (subtree_photo_count >= direct_photo_count)
);

CREATE TABLE photo_mapping_queue (
    photo_id INTEGER PRIMARY KEY,
    reason TEXT NOT NULL,
    CHECK (reason IN ('refresh', 'taxonomy', 'hook', 'settings')),
    FOREIGN KEY (photo_id) REFERENCES photos(photo_id) ON DELETE CASCADE
);

CREATE TABLE operations (
    operation_id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL,
    source TEXT NOT NULL,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    total_items INTEGER NOT NULL,
    succeeded_items INTEGER NOT NULL,
    failed_items INTEGER NOT NULL,
    rollbackable INTEGER NOT NULL,
    has_formatted_input INTEGER NOT NULL,
    CHECK (total_items >= 0),
    CHECK (succeeded_items >= 0),
    CHECK (failed_items >= 0),
    CHECK (succeeded_items + failed_items = total_items),
    CHECK (rollbackable IN (0, 1)),
    CHECK (has_formatted_input IN (0, 1))
);

CREATE TABLE operation_audit_rows (
    operation_id INTEGER NOT NULL,
    sequence INTEGER NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT,
    action TEXT NOT NULL,
    before_json TEXT,
    after_json TEXT,
    succeeded INTEGER NOT NULL,
    message TEXT NOT NULL,
    PRIMARY KEY (operation_id, sequence),
    CHECK (sequence > 0),
    CHECK (succeeded IN (0, 1)),
    FOREIGN KEY (operation_id)
        REFERENCES operations(operation_id) ON DELETE CASCADE
);

CREATE INDEX idx_photo_directories_parent_name
    ON photo_directories(parent_directory_id, name, directory_id);
CREATE INDEX idx_photos_directory_filename
    ON photos(directory_id, filename, photo_id);
CREATE INDEX idx_photo_metadata_coordinates
    ON photo_metadata(latitude, longitude, photo_id);
CREATE INDEX idx_photo_taxon_mapping_taxon
    ON photo_taxon_mapping(taxon_id, photo_id);
CREATE INDEX idx_photo_taxon_mapping_status
    ON photo_taxon_mapping(status, photo_id);
CREATE INDEX idx_photo_taxon_candidates_taxon
    ON photo_taxon_candidates(taxon_id, photo_id);
CREATE INDEX idx_photo_taxon_usage_subtree
    ON photo_taxon_usage(subtree_photo_count, taxon_id);
CREATE INDEX idx_photo_mapping_queue_reason
    ON photo_mapping_queue(reason, photo_id);
CREATE INDEX idx_operation_audit_entity
    ON operation_audit_rows(entity_type, entity_id, operation_id, sequence);
CREATE INDEX idx_operations_applied
    ON operations(applied_at DESC, operation_id DESC);

PRAGMA user_version = 2;
"#;

pub fn photo_library_location(
    database: &Database,
    library_uuid: &str,
) -> CoreResult<PhotoLibraryLocation> {
    let library = database.photo_library(library_uuid)?;
    Ok(PhotoLibraryLocation {
        library_uuid: library.library_uuid,
        root_path: library.root_path,
        database_path: library.db_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_three_schema_two_databases() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("metadata.db")).unwrap();
        let locations = database.locations().unwrap();
        assert_eq!(
            locations.metadata_database,
            path_string(&database.metadata_path())
        );
        assert_eq!(
            schema_version(&database.connect_metadata().unwrap()),
            SCHEMA_VERSION
        );
        assert_eq!(
            schema_version(&database.connect_taxonomy().unwrap()),
            SCHEMA_VERSION
        );
        assert_eq!(
            schema_version(&database.connect_photo_library().unwrap()),
            SCHEMA_VERSION
        );
        assert_ne!(locations.metadata_database, locations.taxonomy_database);
        let active = database.active_photo_library().unwrap().unwrap();
        assert_ne!(active.db_path, locations.taxonomy_database);
    }

    #[test]
    fn switches_between_independent_photo_libraries() {
        let directory = tempfile::tempdir().unwrap();
        let root_a = directory.path().join("photos-a");
        let root_b = directory.path().join("photos-b");
        fs::create_dir_all(&root_a).unwrap();
        fs::create_dir_all(&root_b).unwrap();
        let database = Database::open(directory.path().join("metadata.db")).unwrap();
        let library_a = database
            .register_photo_library(&root_a, &directory.path().join("a.db"), Some("A"))
            .unwrap();
        database
            .connect()
            .unwrap()
            .execute(
                r#"
                INSERT INTO photos (
                    directory_id, filename, file_size, modified_at_ns
                ) SELECT directory_id, 'a.jpg', 1, 1
                  FROM photo_directories WHERE relative_path = ''
                "#,
                [],
            )
            .unwrap();
        let library_b = database
            .register_photo_library(&root_b, &directory.path().join("b.db"), Some("B"))
            .unwrap();
        assert_eq!(
            database
                .connect()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM photos", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        database
            .switch_photo_library(&library_a.library_uuid)
            .unwrap();
        assert_eq!(
            database
                .connect()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM photos", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_ne!(library_a.library_uuid, library_b.library_uuid);
    }

    #[test]
    fn rejects_any_other_schema_version() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("metadata.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("PRAGMA user_version = 1;")
            .unwrap();
        drop(connection);
        let error = Database::open(path).unwrap_err();
        assert!(error.to_string().contains("expected 2"));
    }

    fn schema_version(connection: &Connection) -> i64 {
        connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap()
    }
}
