# Storage Backend API

`vividarium_core::storage` owns database locations and photo library
registration. The application uses three database roles:

- one metadata database;
- one independently located taxonomy database;
- one or more independently located photo library databases.

The application version is `3.0.0` and every database role uses schema `2`.
Any other schema version is rejected. There is no migration or compatibility
interface.

## Types

`Database` is the handle passed to all core modules.

`DatabaseLocations` contains:

| Field | Type | Description |
| --- | --- | --- |
| `metadata_database` | `String` | Absolute metadata database path. |
| `taxonomy_database` | `String` | Active taxonomy database path. |
| `default_taxonomy_directory` | `String` | Default creation destination for taxonomy databases. |
| `default_photo_library_directory` | `String` | Default creation destination for photo library databases. |
| `active_photo_library_uuid` | `Option<String>` | Active library identity. |

`PhotoLibraryRegistration` contains `library_uuid`, `display_name`,
`root_path`, `db_path`, and `last_opened_at`.

`PhotoLibraryLocation` contains `library_uuid`, `root_path`, and
`database_path`.

## Interfaces

| Interface | Parameters | Return | Description |
| --- | --- | --- | --- |
| `Database::open` | `metadata_path` | `CoreResult<Database>` | Open or initialize metadata and taxonomy storage. No photo library is created automatically. |
| `Database::metadata_path` | none | `PathBuf` | Return the metadata database path. |
| `Database::taxonomy_path` | none | `CoreResult<PathBuf>` | Return the configured taxonomy database path. |
| `Database::locations` | none | `CoreResult<DatabaseLocations>` | Return all current and default locations. |
| `Database::active_photo_library` | none | `CoreResult<Option<PhotoLibraryRegistration>>` | Return the active registration. |
| `Database::list_photo_libraries` | none | `CoreResult<Vec<PhotoLibraryRegistration>>` | List registered libraries. |
| `Database::register_photo_library` | `root_path: &Path`, `database_path: &Path`, `display_name: Option<&str>` | `CoreResult<PhotoLibraryRegistration>` | Register an existing or new photo library DB and activate it. The root must be an existing directory not used by another library. |
| `Database::switch_photo_library` | `library_uuid: &str` | `CoreResult<PhotoLibraryRegistration>` | Validate the registered DB, apply its pending taxonomy synchronization, and activate it. A missing DB is reported and never recreated. |
| `Database::remove_photo_library` | `library_uuid: &str` | `CoreResult<()>` | Remove only the metadata registration; files and the library DB remain. |
| `Database::rebind_photo_library_root` | `library_uuid: &str`, `root_path: &Path` | `CoreResult<PhotoLibraryRegistration>` | Bind a copied library DB to a new local photo root. |
| `Database::relocate_photo_library_database` | `library_uuid: &str`, `destination: &Path` | `CoreResult<PhotoLibraryRegistration>` | Move one library DB through a consistent SQLite online-backup snapshot and update metadata. |
| `Database::relocate_taxonomy_database` | `destination: &Path` | `CoreResult<DatabaseLocations>` | Move taxonomy storage through a consistent SQLite online-backup snapshot and update metadata. |
| `Database::set_default_taxonomy_directory` | `directory: &Path` | `CoreResult<DatabaseLocations>` | Set the default taxonomy creation directory. |
| `Database::set_default_photo_library_directory` | `directory: &Path` | `CoreResult<DatabaseLocations>` | Set the default photo library creation directory. |
| `photo_library_location` | `database: &Database`, `library_uuid: &str` | `CoreResult<PhotoLibraryLocation>` | Return typed paths for one registration. |

Photo library identity is its persisted UUID, not its root path. A copied DB
therefore remains the same library and can be rebound on another computer.
Each library owns its photo index, mapping state, usage counts, and rename
history.

Every photo library must represent one real photo root. Root paths and
database paths are unique among registrations. Callers must prevent concurrent
mutating operations while relocating a database; the desktop commands enforce
this operation guard.

All ordinary database connections and attached taxonomy/photo contexts require
their configured files to exist. A disconnected or missing registered
database returns `CoreError::NotFound`; no read, search, or mutation interface
silently creates an empty replacement.
