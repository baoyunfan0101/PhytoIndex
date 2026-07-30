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
| `Database::open` | `metadata_path` | `CoreResult<Database>` | Open or initialize metadata and taxonomy storage. No photo library is created automatically, and an unavailable registered photo library does not block startup. |
| `Database::metadata_path` | none | `PathBuf` | Return the metadata database path. |
| `Database::taxonomy_path` | none | `CoreResult<PathBuf>` | Return the configured taxonomy database path. |
| `Database::locations` | none | `CoreResult<DatabaseLocations>` | Return all current and default locations. |
| `Database::active_photo_library` | none | `CoreResult<Option<PhotoLibraryRegistration>>` | Return the active registration. |
| `Database::list_photo_libraries` | none | `CoreResult<Vec<PhotoLibraryRegistration>>` | List registered libraries. |
| `Database::register_photo_library` | `root_path: &Path`, `database_path: &Path`, `display_name: Option<&str>` | `CoreResult<PhotoLibraryRegistration>` | Register an existing or new photo library DB and activate it. The root must be an existing directory not used by another library. An existing DB with a different taxonomy identity or synchronization watermark is fully queued for remapping before activation. |
| `Database::switch_photo_library` | `library_uuid: &str` | `CoreResult<PhotoLibraryRegistration>` | Validate the registered DB, apply its pending taxonomy synchronization, and activate it. A missing DB is reported and never recreated. |
| `Database::remove_photo_library` | `library_uuid: &str` | `CoreResult<()>` | Remove only the metadata registration; files and the library DB remain. |
| `Database::rename_photo_library` | `library_uuid: &str`, `display_name: &str` | `CoreResult<PhotoLibraryRegistration>` | Replace the user-visible registration name. Blank names are rejected. |
| `Database::rebind_photo_library_root` | `library_uuid: &str`, `root_path: &Path` | `CoreResult<PhotoLibraryRegistration>` | Bind a copied library DB to a new local photo root. |
| `Database::rebind_photo_library_database` | `library_uuid: &str`, `existing_database_path: &Path` | `CoreResult<PhotoLibraryRegistration>` | Point an existing registration at an existing DB without copying or moving it. The schema and persisted library UUID must match. A taxonomy identity or synchronization watermark mismatch creates a full-remap request. |
| `Database::relocate_photo_library_database` | `library_uuid: &str`, `destination: &Path` | `CoreResult<PhotoLibraryRegistration>` | Safely move one library database and update its registered path. |
| `Database::relocate_taxonomy_database` | `destination: &Path` | `CoreResult<DatabaseLocations>` | Safely move taxonomy storage and update its configured path. |
| `Database::set_default_taxonomy_directory` | `directory: &Path` | `CoreResult<DatabaseLocations>` | Set the default taxonomy creation directory. |
| `Database::set_default_photo_library_directory` | `directory: &Path` | `CoreResult<DatabaseLocations>` | Set the default photo library creation directory. |
| `photo_library_location` | `database: &Database`, `library_uuid: &str` | `CoreResult<PhotoLibraryLocation>` | Return typed paths for one registration. |

Photo library identity is its persisted UUID, not its root path. A copied DB
therefore remains the same library and can be rebound on another computer.
Each library owns its photo index, mapping state, usage counts, and rename
history.

Every photo library must represent one real photo root. Root paths and
database paths are unique among registrations. Callers must prevent concurrent
mutating operations while relocating a photo database or rebinding its path;
the desktop commands enforce this requirement. Taxonomy relocation cannot
overlap another taxonomy replacement.

Metadata and taxonomy storage remain available when the active photo library
is offline. Pure taxonomy reads, updates, history, settings, and base database
interfaces do not attach the photo library. Photo interfaces and cross-module
taxonomy/photo interfaces require the active library DB and return
`CoreError::NotFound` when it is unavailable. The registration is retained,
and no interface silently creates an empty replacement DB.
