use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, OptionalExtension, Row, TransactionBehavior, params};
use uuid::Uuid;

use crate::error::{CoreError, CoreResult};
use crate::models::{DatabaseLocations, Photo, PhotoLibraryLocation, PhotoLibraryRegistration};

pub const SCHEMA_VERSION: i64 = 2;
pub(crate) const LOCAL_TAXON_ID_FLOOR: i64 = 8_000_000_000_000_000;

const DEFAULT_TAXONOMY_FILENAME: &str = "taxonomy.db";
const DEFAULT_LIBRARY_DIRECTORY: &str = "photo-libraries";

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

    #[cfg(test)]
    pub(crate) fn open_test(metadata_path: impl Into<PathBuf>) -> CoreResult<Self> {
        let metadata_path = metadata_path.into();
        let directory = metadata_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let root_path = directory.join("test-photo-root");
        fs::create_dir_all(&root_path)?;
        let database = Self::open(metadata_path)?;
        database.register_photo_library(
            &root_path,
            &directory.join("test-photo-library.db"),
            Some("Test Photo Library"),
        )?;
        database
            .connect()?
            .execute("DELETE FROM photo_directories", [])?;
        Ok(database)
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
        if !root_path.is_dir() {
            return Err(CoreError::InvalidArgument(format!(
                "photo library root is not an existing directory: {}",
                root_path.display()
            )));
        }
        let root_path = canonical_or_absolute(root_path)?;
        let database_path = absolute_path(database_path)?;
        let registered_database_path = self
            .connect_metadata()?
            .query_row(
                "SELECT db_path FROM photo_libraries WHERE root_path = ?",
                [path_string(&root_path)],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if registered_database_path
            .as_deref()
            .is_some_and(|path| Path::new(path) != database_path)
        {
            return Err(CoreError::InvalidArgument(format!(
                "photo library root is already registered: {}",
                root_path.display()
            )));
        }
        let stored_state = if database_path.exists() {
            initialize_existing_file(&database_path, PHOTO_SCHEMA)?;
            read_photo_library_sync_state(&database_path)?
        } else {
            if let Some(parent) = database_path.parent() {
                fs::create_dir_all(parent)?;
            }
            initialize_file(&database_path, PHOTO_SCHEMA)?;
            None
        };
        let library_uuid = stored_state
            .as_ref()
            .map(|state| state.library_uuid.clone())
            .unwrap_or_else(new_uuid);
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
        let mut metadata = self.connect_metadata()?;
        let metadata_transaction =
            metadata.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let latest_sync_id = metadata_transaction.query_row(
            r#"
            SELECT last_dispatched_sync_id
            FROM taxonomy_sync_dispatch
            WHERE dispatch_id = 1
            "#,
            [],
            |row| row.get::<_, i64>(0),
        )?;
        {
            let connection = open_existing_connection(&database_path)?;
            connection.execute(
                r#"
                INSERT INTO photo_library (
                    library_id, library_uuid, root_path,
                    bound_taxonomy_identity, last_taxonomy_sync_id
                ) VALUES (1, ?, ?, ?, ?)
                ON CONFLICT(library_id) DO UPDATE SET
                    library_uuid = excluded.library_uuid,
                    root_path = excluded.root_path
                "#,
                params![
                    library_uuid,
                    path_string(&root_path),
                    taxonomy_identity,
                    latest_sync_id
                ],
            )?;
            ensure_photo_root(&connection)?;
        }
        metadata_transaction.execute(
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
        if let Some(stored_state) = stored_state.as_ref() {
            mark_full_remap_if_stale(
                &metadata_transaction,
                stored_state,
                &taxonomy_identity,
                latest_sync_id,
            )?;
        }
        metadata_transaction.commit()?;
        self.switch_photo_library(&library_uuid)
    }

    pub fn switch_photo_library(&self, library_uuid: &str) -> CoreResult<PhotoLibraryRegistration> {
        let library = self
            .connect_metadata()?
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
        if !Path::new(&library.root_path).is_dir() {
            return Err(CoreError::NotFound(format!(
                "photo library root {}",
                library.root_path
            )));
        }
        initialize_existing_file(Path::new(&library.db_path), PHOTO_SCHEMA)?;
        crate::taxonomy::sync::synchronize_photo_library(self, &library)?;
        let mut connection = self.connect_metadata()?;
        let transaction = connection.transaction()?;
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
        if !root_path.is_dir() {
            return Err(CoreError::InvalidArgument(format!(
                "photo library root is not an existing directory: {}",
                root_path.display()
            )));
        }
        let root_path = canonical_or_absolute(root_path)?;
        let library = self.photo_library(library_uuid)?;
        let duplicate = self.connect_metadata()?.query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM photo_libraries
                WHERE root_path = ? AND library_uuid != ?
            )
            "#,
            params![path_string(&root_path), library_uuid],
            |row| row.get::<_, bool>(0),
        )?;
        if duplicate {
            return Err(CoreError::InvalidArgument(format!(
                "photo library root is already registered: {}",
                root_path.display()
            )));
        }
        let mut connection = self.connect_photo_library_registration(&library)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "UPDATE photo_library SET root_path = ? WHERE library_id = 1",
            [path_string(&root_path)],
        )?;
        transaction.execute(
            "UPDATE metadata.photo_libraries SET root_path = ? WHERE library_uuid = ?",
            params![path_string(&root_path), library_uuid],
        )?;
        transaction.commit()?;
        self.photo_library(library_uuid)
    }

    pub fn rebind_photo_library_database(
        &self,
        library_uuid: &str,
        existing_database_path: &Path,
    ) -> CoreResult<PhotoLibraryRegistration> {
        self.photo_library(library_uuid)?;
        let database_path = absolute_path(existing_database_path)?;
        validate_existing_file(&database_path)?;
        let stored_state = read_photo_library_sync_state(&database_path)
            .map_err(|state_error| {
                CoreError::InvalidArgument(format!(
                    "invalid photo library database {}: {state_error}",
                    database_path.display()
                ))
            })?
            .ok_or_else(|| {
                CoreError::InvalidArgument(format!(
                    "photo library database has no persisted identity: {}",
                    database_path.display()
                ))
            })?;
        if stored_state.library_uuid != library_uuid {
            return Err(CoreError::InvalidArgument(format!(
                "photo library database identity does not match registration {library_uuid}"
            )));
        }
        let taxonomy_identity = self.taxonomy_identity()?;
        let mut metadata = self.connect_metadata()?;
        let transaction = metadata.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let duplicate = transaction.query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM photo_libraries
                WHERE db_path = ? AND library_uuid != ?
            )
            "#,
            params![path_string(&database_path), library_uuid],
            |row| row.get::<_, bool>(0),
        )?;
        if duplicate {
            return Err(CoreError::InvalidArgument(format!(
                "photo library database is already registered: {}",
                database_path.display()
            )));
        }
        let latest_sync_id = transaction.query_row(
            r#"
            SELECT last_dispatched_sync_id
            FROM taxonomy_sync_dispatch
            WHERE dispatch_id = 1
            "#,
            [],
            |row| row.get::<_, i64>(0),
        )?;
        let updated = transaction.execute(
            "UPDATE photo_libraries SET db_path = ? WHERE library_uuid = ?",
            params![path_string(&database_path), library_uuid],
        )?;
        if updated != 1 {
            return Err(CoreError::NotFound(format!("photo library {library_uuid}")));
        }
        mark_full_remap_if_stale(
            &transaction,
            &stored_state,
            &taxonomy_identity,
            latest_sync_id,
        )?;
        transaction.commit()?;
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

    pub(crate) fn connect(&self) -> CoreResult<Connection> {
        self.connect_photo_library()
    }

    pub(crate) fn connect_metadata(&self) -> CoreResult<Connection> {
        open_existing_connection(&self.metadata_path())
    }

    pub(crate) fn connect_taxonomy(&self) -> CoreResult<Connection> {
        open_existing_connection(&self.taxonomy_path()?)
    }

    pub(crate) fn connect_taxonomy_metadata_context(&self) -> CoreResult<Connection> {
        let connection = self.connect_taxonomy()?;
        attach_database(&connection, &self.metadata_path(), "metadata")?;
        Ok(connection)
    }

    pub(crate) fn connect_taxonomy_photo_context(&self) -> CoreResult<Connection> {
        let connection = self.connect_taxonomy_metadata_context()?;
        let library = self.active_photo_library()?.ok_or_else(|| {
            CoreError::InvalidArgument("no active photo library is registered".into())
        })?;
        attach_database(
            &connection,
            Path::new(&library.db_path),
            "active_photo_library",
        )?;
        create_cross_database_views(&connection)?;
        Ok(connection)
    }

    pub(crate) fn connect_photo_library(&self) -> CoreResult<Connection> {
        let library = self.active_photo_library()?.ok_or_else(|| {
            CoreError::InvalidArgument("no active photo library is registered".into())
        })?;
        self.connect_photo_library_registration(&library)
    }

    pub(crate) fn connect_photo_library_registration(
        &self,
        library: &PhotoLibraryRegistration,
    ) -> CoreResult<Connection> {
        let connection = open_existing_connection(Path::new(&library.db_path))?;
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

    pub(crate) fn replace_taxonomy_database_file(&self, replacement: &Path) -> CoreResult<()> {
        initialize_existing_file(replacement, TAXONOMY_SCHEMA)?;
        let replacement_identity = open_existing_connection(replacement)?
            .query_row(
                "SELECT taxonomy_identity FROM taxonomy_identity WHERE identity_id = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if replacement_identity.as_deref().is_none_or(str::is_empty) {
            return Err(CoreError::InvalidArgument(
                "replacement taxonomy database has no identity".into(),
            ));
        }
        let target = self.taxonomy_path()?;
        let candidate = target.with_file_name(format!(
            ".taxonomy-replacement-candidate-{}.db",
            Uuid::new_v4()
        ));
        copy_database_file(replacement, &candidate, TAXONOMY_SCHEMA)?;
        let replacement_result = (|| -> CoreResult<()> {
            {
                let mut metadata = self.connect_metadata()?;
                let transaction =
                    metadata.transaction_with_behavior(TransactionBehavior::Immediate)?;
                let target_sync_id = transaction.query_row(
                    r#"
                    SELECT last_dispatched_sync_id
                    FROM taxonomy_sync_dispatch
                    WHERE dispatch_id = 1
                    "#,
                    [],
                    |row| row.get::<_, i64>(0),
                )?;
                transaction.execute(
                    r#"
                    INSERT INTO photo_library_taxonomy_pending (
                        library_uuid, target_sync_id, full_remap_required
                    )
                    SELECT library_uuid, ?, 1
                    FROM photo_libraries
                    WHERE true
                    ON CONFLICT(library_uuid) DO UPDATE SET
                        target_sync_id = excluded.target_sync_id,
                        full_remap_required = 1
                    "#,
                    [target_sync_id],
                )?;
                transaction.execute("DELETE FROM photo_library_taxonomy_pending_taxa", [])?;
                transaction.commit()?;
            }
            open_existing_connection(&target)?.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
            remove_sqlite_sidecar(&target, "-wal")?;
            remove_sqlite_sidecar(&target, "-shm")?;
            let backup = target.with_file_name(format!(
                ".taxonomy-replacement-backup-{}.db",
                Uuid::new_v4()
            ));
            fs::rename(&target, &backup)?;
            if let Err(error) = fs::rename(&candidate, &target) {
                let _ = fs::rename(&backup, &target);
                return Err(error.into());
            }
            let _ = fs::remove_file(&backup);
            Ok(())
        })();
        if let Err(error) = replacement_result {
            let _ = fs::remove_file(&candidate);
            return Err(error);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn latest_taxonomy_sync_id(&self) -> CoreResult<i64> {
        self.connect_metadata()?
            .query_row(
                r#"
                SELECT last_dispatched_sync_id
                FROM taxonomy_sync_dispatch
                WHERE dispatch_id = 1
                "#,
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
        self.seed_metadata()?;
        crate::taxonomy::sync::dispatch_pending_events(self)?;
        if let Some(active) = self.active_photo_library()? {
            let _ = crate::taxonomy::sync::synchronize_photo_library(self, &active);
        }
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
    initialize_connection(&connection, schema)
}

pub(crate) fn initialize_taxonomy_database_file(path: &Path) -> CoreResult<()> {
    initialize_file(path, TAXONOMY_SCHEMA)?;
    let connection = open_existing_connection(path)?;
    connection.execute(
        r#"
        INSERT INTO taxonomy_identity (identity_id, taxonomy_identity)
        VALUES (1, ?)
        ON CONFLICT(identity_id) DO UPDATE
        SET taxonomy_identity = excluded.taxonomy_identity
        "#,
        [new_uuid()],
    )?;
    Ok(())
}

fn initialize_connection(connection: &Connection, schema: &str) -> CoreResult<()> {
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

fn initialize_existing_file(path: &Path, schema: &str) -> CoreResult<()> {
    let connection = open_existing_connection(path)?;
    initialize_connection(&connection, schema)
}

fn validate_existing_file(path: &Path) -> CoreResult<()> {
    let connection = open_existing_connection(path)?;
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version == SCHEMA_VERSION {
        Ok(())
    } else {
        Err(CoreError::InvalidArgument(format!(
            "unsupported database schema version: {version}; expected {SCHEMA_VERSION}"
        )))
    }
}

fn open_connection(path: &Path) -> CoreResult<Connection> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI,
    )?;
    configure_connection(connection)
}

fn open_existing_connection(path: &Path) -> CoreResult<Connection> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|error| {
        if path.is_file() {
            CoreError::Database(error)
        } else {
            CoreError::NotFound(format!("database file {}", path.display()))
        }
    })?;
    configure_connection(connection)
}

fn configure_connection(connection: Connection) -> CoreResult<Connection> {
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
    if !path.is_file() {
        return Err(CoreError::NotFound(format!(
            "database file {}",
            path.display()
        )));
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

struct PhotoLibrarySyncState {
    library_uuid: String,
    bound_taxonomy_identity: String,
    last_taxonomy_sync_id: i64,
}

fn read_photo_library_sync_state(path: &Path) -> CoreResult<Option<PhotoLibrarySyncState>> {
    let connection = open_existing_connection(path)?;
    connection
        .query_row(
            r#"
            SELECT library_uuid, bound_taxonomy_identity, last_taxonomy_sync_id
            FROM photo_library
            WHERE library_id = 1
            "#,
            [],
            |row| {
                Ok(PhotoLibrarySyncState {
                    library_uuid: row.get(0)?,
                    bound_taxonomy_identity: row.get(1)?,
                    last_taxonomy_sync_id: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
}

fn mark_full_remap_if_stale(
    transaction: &rusqlite::Transaction<'_>,
    stored_state: &PhotoLibrarySyncState,
    taxonomy_identity: &str,
    latest_sync_id: i64,
) -> CoreResult<()> {
    if stored_state.bound_taxonomy_identity == taxonomy_identity
        && stored_state.last_taxonomy_sync_id == latest_sync_id
    {
        return Ok(());
    }
    transaction.execute(
        r#"
        INSERT INTO photo_library_taxonomy_pending (
            library_uuid, target_sync_id, full_remap_required
        ) VALUES (?, ?, 1)
        ON CONFLICT(library_uuid) DO UPDATE SET
            target_sync_id = excluded.target_sync_id,
            full_remap_required = 1
        "#,
        params![stored_state.library_uuid, latest_sync_id],
    )?;
    transaction.execute(
        r#"
        DELETE FROM photo_library_taxonomy_pending_taxa
        WHERE library_uuid = ?
        "#,
        [&stored_state.library_uuid],
    )?;
    Ok(())
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
    initialize_existing_file(source, schema)?;
    let source_connection = open_connection(source)?;
    let mut destination_connection = Connection::open(destination)?;
    let backup_result = (|| -> CoreResult<()> {
        let backup =
            rusqlite::backup::Backup::new(&source_connection, &mut destination_connection)?;
        backup.run_to_completion(256, Duration::from_millis(10), None)?;
        drop(backup);
        let version: i64 =
            destination_connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version != SCHEMA_VERSION {
            return Err(CoreError::InvalidArgument(format!(
                "unsupported database schema version: {version}; expected {SCHEMA_VERSION}"
            )));
        }
        Ok(())
    })();
    drop(destination_connection);
    drop(source_connection);
    if let Err(error) = backup_result {
        let _ = fs::remove_file(destination);
        return Err(error);
    }
    if let Err(error) = fs::remove_file(source) {
        let _ = fs::remove_file(destination);
        return Err(error.into());
    }
    remove_sqlite_sidecar(source, "-wal")?;
    remove_sqlite_sidecar(source, "-shm")?;
    Ok(())
}

fn copy_database_file(source: &Path, destination: &Path, schema: &str) -> CoreResult<()> {
    if destination.exists() {
        return Err(CoreError::InvalidArgument(format!(
            "database destination already exists: {}",
            destination.display()
        )));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    initialize_existing_file(source, schema)?;
    let source_connection = open_existing_connection(source)?;
    let mut destination_connection = Connection::open(destination)?;
    let backup_result = (|| -> CoreResult<()> {
        let backup =
            rusqlite::backup::Backup::new(&source_connection, &mut destination_connection)?;
        backup.run_to_completion(256, Duration::from_millis(10), None)?;
        drop(backup);
        let version: i64 =
            destination_connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version != SCHEMA_VERSION {
            return Err(CoreError::InvalidArgument(format!(
                "unsupported database schema version: {version}; expected {SCHEMA_VERSION}"
            )));
        }
        Ok(())
    })();
    drop(destination_connection);
    drop(source_connection);
    if let Err(error) = backup_result {
        let _ = fs::remove_file(destination);
        return Err(error);
    }
    Ok(())
}

fn remove_sqlite_sidecar(database_path: &Path, suffix: &str) -> CoreResult<()> {
    let sidecar = PathBuf::from(format!("{}{suffix}", database_path.display()));
    if sidecar.exists() {
        fs::remove_file(sidecar)?;
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
    root_path TEXT NOT NULL UNIQUE,
    db_path TEXT NOT NULL UNIQUE,
    last_opened_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (length(library_uuid) > 0),
    CHECK (length(display_name) > 0),
    CHECK (length(root_path) > 0)
);

CREATE TABLE taxonomy_sync_dispatch (
    dispatch_id INTEGER PRIMARY KEY CHECK (dispatch_id = 1),
    last_dispatched_sync_id INTEGER NOT NULL DEFAULT 0,
    CHECK (last_dispatched_sync_id >= 0)
);

INSERT INTO taxonomy_sync_dispatch (dispatch_id, last_dispatched_sync_id)
VALUES (1, 0);

CREATE TABLE photo_library_taxonomy_pending (
    library_uuid TEXT PRIMARY KEY,
    target_sync_id INTEGER NOT NULL,
    full_remap_required INTEGER NOT NULL DEFAULT 0,
    CHECK (target_sync_id >= 0),
    CHECK (full_remap_required IN (0, 1)),
    FOREIGN KEY (library_uuid)
        REFERENCES photo_libraries(library_uuid) ON DELETE CASCADE
);

CREATE TABLE photo_library_taxonomy_pending_taxa (
    library_uuid TEXT NOT NULL,
    taxon_id INTEGER NOT NULL,
    PRIMARY KEY (library_uuid, taxon_id),
    FOREIGN KEY (library_uuid)
        REFERENCES photo_library_taxonomy_pending(library_uuid) ON DELETE CASCADE
) WITHOUT ROWID;

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
    fn initializes_metadata_and_taxonomy_without_a_fake_photo_library() {
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
        assert_ne!(locations.metadata_database, locations.taxonomy_database);
        assert_eq!(database.active_photo_library().unwrap(), None);
        assert!(database.list_photo_libraries().unwrap().is_empty());
        assert_eq!(locations.active_photo_library_uuid, None);
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
    fn switching_a_missing_registered_database_does_not_recreate_it() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("photos");
        fs::create_dir_all(&root).unwrap();
        let database = Database::open(directory.path().join("metadata.db")).unwrap();
        let database_path = directory.path().join("library.db");
        let library = database
            .register_photo_library(&root, &database_path, Some("Library"))
            .unwrap();
        fs::remove_file(&database_path).unwrap();

        let error = database
            .switch_photo_library(&library.library_uuid)
            .unwrap_err();

        assert!(matches!(error, CoreError::NotFound(_)));
        assert!(!database_path.exists());
    }

    #[test]
    fn ordinary_connections_do_not_recreate_a_missing_active_library() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("photos");
        fs::create_dir_all(&root).unwrap();
        let database = Database::open(directory.path().join("metadata.db")).unwrap();
        let database_path = directory.path().join("library.db");
        database
            .register_photo_library(&root, &database_path, Some("Library"))
            .unwrap();
        fs::remove_file(&database_path).unwrap();

        assert!(matches!(database.connect(), Err(CoreError::NotFound(_))));
        assert!(!database_path.exists());
        database.connect_taxonomy_metadata_context().unwrap();
        assert!(matches!(
            database.connect_taxonomy_photo_context(),
            Err(CoreError::NotFound(_))
        ));
        assert!(!database_path.exists());
        assert!(
            crate::taxonomy::suggest_taxa(&database, "missing", 10)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            crate::taxonomy::get_taxonomy_name_separator(&database).unwrap(),
            ";"
        );
    }

    #[test]
    fn rebinds_a_missing_active_library_database_and_restores_sync() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("photos");
        fs::create_dir_all(&root).unwrap();
        let database = Database::open(directory.path().join("metadata.db")).unwrap();
        let old_path = directory.path().join("old.db");
        let new_path = directory.path().join("new.db");
        let library = database
            .register_photo_library(&root, &old_path, Some("Library"))
            .unwrap();
        database
            .connect()
            .unwrap()
            .execute(
                r#"
                INSERT INTO photos (
                    directory_id, filename, file_size, modified_at_ns
                )
                SELECT directory_id, 'photo.jpg', 1, 1
                FROM photo_directories
                WHERE relative_path = ''
                "#,
                [],
            )
            .unwrap();
        fs::rename(&old_path, &new_path).unwrap();
        assert!(matches!(database.connect(), Err(CoreError::NotFound(_))));

        {
            let mut taxonomy = database.connect_taxonomy().unwrap();
            let transaction = taxonomy
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            crate::taxonomy::sync::record_event(&transaction, None, [42], false).unwrap();
            transaction.commit().unwrap();
        }
        crate::taxonomy::sync::synchronize_pending_photo_libraries(&database).unwrap();
        let latest_sync_id = database.latest_taxonomy_sync_id().unwrap();
        database
            .connect_metadata()
            .unwrap()
            .execute(
                "DELETE FROM photo_library_taxonomy_pending WHERE library_uuid = ?",
                [&library.library_uuid],
            )
            .unwrap();

        let rebound = database
            .rebind_photo_library_database(&library.library_uuid, &new_path)
            .unwrap();

        assert_eq!(rebound.db_path, path_string(&new_path));
        assert_eq!(
            database
                .connect_metadata()
                .unwrap()
                .query_row(
                    r#"
                    SELECT target_sync_id, full_remap_required
                    FROM photo_library_taxonomy_pending
                    WHERE library_uuid = ?
                    "#,
                    [&library.library_uuid],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, bool>(1)?)),
                )
                .unwrap(),
            (latest_sync_id, true)
        );
        crate::taxonomy::sync::synchronize_pending_photo_libraries(&database).unwrap();
        assert_eq!(
            database
                .connect()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM photo_mapping_queue", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
    }

    #[test]
    fn rejects_rebinding_to_a_different_library_database() {
        let directory = tempfile::tempdir().unwrap();
        let root_a = directory.path().join("photos-a");
        let root_b = directory.path().join("photos-b");
        fs::create_dir_all(&root_a).unwrap();
        fs::create_dir_all(&root_b).unwrap();
        let database = Database::open(directory.path().join("metadata.db")).unwrap();
        let path_a = directory.path().join("a.db");
        let path_b = directory.path().join("b.db");
        let library_a = database
            .register_photo_library(&root_a, &path_a, Some("A"))
            .unwrap();
        database
            .register_photo_library(&root_b, &path_b, Some("B"))
            .unwrap();

        let error = database
            .rebind_photo_library_database(&library_a.library_uuid, &path_b)
            .unwrap_err();

        assert!(matches!(error, CoreError::InvalidArgument(_)));
        assert_eq!(
            database
                .photo_library(&library_a.library_uuid)
                .unwrap()
                .db_path,
            path_string(&path_a)
        );
    }

    #[test]
    fn photo_library_roots_are_unique() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("photos");
        fs::create_dir_all(&root).unwrap();
        let database = Database::open(directory.path().join("metadata.db")).unwrap();
        database
            .register_photo_library(&root, &directory.path().join("first.db"), Some("First"))
            .unwrap();

        let error = database
            .register_photo_library(&root, &directory.path().join("second.db"), Some("Second"))
            .unwrap_err();

        assert!(matches!(error, CoreError::InvalidArgument(_)));
        assert_eq!(database.list_photo_libraries().unwrap().len(), 1);
        assert!(!directory.path().join("second.db").exists());
    }

    #[test]
    fn registering_a_stale_library_queues_a_full_remap() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("metadata.db")).unwrap();
        let taxonomy_identity = database.taxonomy_identity().unwrap();
        {
            let mut taxonomy = database.connect_taxonomy().unwrap();
            let transaction = taxonomy
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            crate::taxonomy::sync::record_event(&transaction, None, [42], false).unwrap();
            transaction.commit().unwrap();
        }
        crate::taxonomy::sync::synchronize_pending_photo_libraries(&database).unwrap();
        let latest_sync_id = database.latest_taxonomy_sync_id().unwrap();
        assert!(latest_sync_id > 0);
        assert_eq!(
            database
                .connect_taxonomy()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM taxonomy_sync_events", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );

        let root = directory.path().join("photos");
        fs::create_dir_all(&root).unwrap();
        let library_path = directory.path().join("stale.db");
        initialize_file(&library_path, PHOTO_SCHEMA).unwrap();
        let library_uuid = new_uuid();
        {
            let connection = open_existing_connection(&library_path).unwrap();
            connection
                .execute(
                    r#"
                    INSERT INTO photo_library (
                        library_id, library_uuid, root_path,
                        bound_taxonomy_identity, last_taxonomy_sync_id
                    ) VALUES (1, ?, ?, ?, 0)
                    "#,
                    params![library_uuid, path_string(&root), taxonomy_identity],
                )
                .unwrap();
            ensure_photo_root(&connection).unwrap();
            connection
                .execute(
                    r#"
                    INSERT INTO photos (
                        directory_id, filename, file_size, modified_at_ns
                    )
                    SELECT directory_id, 'stale.jpg', 1, 1
                    FROM photo_directories
                    WHERE relative_path = ''
                    "#,
                    [],
                )
                .unwrap();
        }

        let registered = database
            .register_photo_library(&root, &library_path, Some("Stale"))
            .unwrap();

        assert_eq!(registered.library_uuid, library_uuid);
        let connection = database.connect().unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT last_taxonomy_sync_id FROM photo_library WHERE library_id = 1",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            latest_sync_id
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM photo_mapping_queue", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
    }

    #[test]
    fn relocation_uses_a_consistent_wal_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("photos");
        fs::create_dir_all(&root).unwrap();
        let database = Database::open(directory.path().join("metadata.db")).unwrap();
        let source = directory.path().join("source.db");
        let library = database
            .register_photo_library(&root, &source, Some("Library"))
            .unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                r#"
                INSERT INTO photos (
                    directory_id, filename, file_size, modified_at_ns
                )
                SELECT directory_id, 'wal-photo.jpg', 1, 1
                FROM photo_directories
                WHERE relative_path = ''
                "#,
                [],
            )
            .unwrap();
        let destination = directory.path().join("destination.db");

        database
            .relocate_photo_library_database(&library.library_uuid, &destination)
            .unwrap();
        drop(connection);

        assert!(!source.exists());
        assert!(destination.exists());
        assert_eq!(
            database
                .connect()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM photos WHERE filename = 'wal-photo.jpg'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
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
