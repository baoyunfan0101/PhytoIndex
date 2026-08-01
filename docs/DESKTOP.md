# Desktop Integration API

The Vividarium desktop application exposes the V3 backend through Tauri
commands and typed TypeScript wrappers. The application version remains
`3.0.0`, every database uses schema `2`, and this interface provides no
migration or compatibility path for another schema.

## Desktop modules

| Module | Public interface |
| --- | --- |
| `v3/api` | Typed command wrappers, IPC DTOs, file and directory pickers, media URLs, and CSV download helpers. |
| `App` | Application shell, Photo Library workspace selection, tabs, global search, and background-operation display. |
| `SettingsView` | Singleton keep-alive settings page with General, Storage, Photo Libraries, Naming, Map, Hooks, Base Import, and About sections. |
| `BaseImportSettings` | `onApplied?: () => void`; edits the fixed persistent Base Import workspace. |
| `CustomSqlView` | `onStatus: (message: string) => void`; executes Custom SQL and displays typed result sets. |
| `OperationHistoryView` | `domain: "photo" | "taxonomy"`, `onStatus: (message: string) => void`; lists operation summaries and paged audit rows. |
| `PhotoBrowser` | Shared virtual photo list/grid and photo interaction surface. |
| `MappingEditor` | `photo`, optional `embedded` and `refreshKey`; reads current mapping and candidates through separate backend interfaces. |

Photo Library is the active workspace for photo, mapping, and map modules.
Taxonomy, taxonomy history, Custom SQL, Base Import, and taxonomy metadata do
not require an available Photo Library. Switching the active library closes
photo-dependent tabs while retaining taxonomy and settings tabs.

Map, Mapping, Formatted Update, Custom SQL, and Settings tabs remain mounted.
Other tabs unmount while inactive and retain their view state in the tab view
state store.

The application shell records tab navigation separately from tab order.
Back and Forward skip closed tabs. Closing tabs, switching Photo Libraries,
and replacing the taxonomy database prune invalid navigation entries.

## Storage and Photo Library commands

`PhotoLibraryWorkspace` contains the fields from
`PhotoLibraryRegistration` plus `active`, `root_available`, and
`database_available`.

| Command | Parameters | Return |
| --- | --- | --- |
| `get_database_locations` | none | `DatabaseLocations` |
| `list_photo_libraries` | none | `Vec<PhotoLibraryWorkspace>` |
| `register_photo_library` | `root_path: String`, `database_path: String`, `display_name: Option<String>` | `PhotoLibraryRegistration` |
| `switch_photo_library` | `library_uuid: String` | `PhotoLibraryRegistration` |
| `rename_photo_library` | `library_uuid: String`, `display_name: String` | `PhotoLibraryRegistration` |
| `rebind_photo_library_root` | `library_uuid: String`, `root_path: String` | `PhotoLibraryRegistration` |
| `rebind_photo_library_database` | `library_uuid: String`, `database_path: String` | `PhotoLibraryRegistration` |
| `relocate_photo_library_database` | `library_uuid: String`, `database_path: String` | `PhotoLibraryRegistration` |
| `remove_photo_library` | `library_uuid: String` | `()` |
| `relocate_taxonomy_database` | `database_path: String` | `DatabaseLocations` |
| `set_default_taxonomy_database_directory` | `directory: String` | `DatabaseLocations` |
| `set_default_photo_library_database_directory` | `directory: String` | `DatabaseLocations` |

Registration removal removes metadata only. Relocation moves a database.
Rebinding changes a registration to an existing path without moving data.
Unavailable registered paths are reported by `list_photo_libraries` and are
never silently recreated.

## Custom SQL commands

`PersistentSqlInput` describes a managed CSV or SQLite source by `kind`, SQL
`alias`, `original_path`, managed-copy `available`, and inspected `schema`. Custom SQL and
Base Import use separate persistent registries with the same DTOs.

| Command | Parameters | Return |
| --- | --- | --- |
| `get_custom_taxonomy_sql` | none | `String` |
| `list_custom_sql_inputs` | none | `Vec<PersistentSqlInput>` |
| `add_custom_sql_input` | `request: AddSqlInputRequest` | `AddSqlInputResult` |
| `remove_custom_sql_input` | `request: RemoveSqlInputRequest` | `RemoveSqlInputResult` |
| `execute_custom_taxonomy_sql` | `request: CustomTaxonomySqlRequest` | `CustomSqlExecutionResult` |
| `export_custom_taxonomy_query` | `request: CustomTaxonomySqlExportRequest` | `SqlExportResult` |

`CustomSqlExecutionResult` contains `operation_id`, `changeset_size`,
`result_sets`, statement `messages`, `script_saved`, and `warnings`. Each result set contains typed
`SqlValue` cells and a `truncated` flag. A pure query returns a null
`operation_id`. A mutation creates a rollbackable taxonomy operation. A
successful query or mutation attempts to replace the saved script after the
SQL transaction commits. A save failure leaves SQL execution successful and
is reported by `script_saved = false` plus `warnings`.
Full-query export streams to `destination_path` instead of returning the CSV
through IPC.

## Base Import commands

Base Import uses one backend-located workspace. Its source registry and last
successful SQL are persistent; staging, candidate, and validation are build
artifacts.

| Command | Parameters | Return |
| --- | --- | --- |
| `get_base_import_sql` | none | `String` |
| `list_base_import_inputs` | none | `Vec<PersistentSqlInput>` |
| `add_base_import_input` | `request: AddSqlInputRequest` | `AddSqlInputResult` |
| `remove_base_import_input` | `request: RemoveSqlInputRequest` | `RemoveSqlInputResult` |
| `execute_base_import_sql` | `request: ExecuteBaseImportSqlRequest` | `BaseImportExecutionResult` |
| `validate_base_import` | none | `BaseImportValidationResult` |
| `apply_base_import` | none | `OperationState` |

Validation returns authoritative total warning and error counts plus bounded
issue samples. Apply is allowed only after successful validation. The returned
operation reports replacement progress and stores a
`TaxonomyBaseReplaceResult` in `result`; photo-library remapping continues
through the background synchronization mechanism.

Execution returns statement messages, `script_saved`, and cleanup or save
`warnings`, but no query result tables or query CSV export. Runtime success
attempts to save the script and enables separate validation;
validation issues do not undo that save. Add, remove, and successful SQL
execution invalidate the current candidate and validation result. The frontend
updates its source list from the authoritative `inputs` returned by
`AddSqlInputResult` or `RemoveSqlInputResult`.

After a successful apply, the shell closes tabs that store taxonomy identity,
taxon IDs, or taxonomy paths. Photo and mapping views remain open but reload
their mapping data. Formatted Update retains its draft and clears its previous
preview.

Apply removes staging, candidate, and validation artifacts while preserving
the source registry and saved SQL. Cleanup failures are returned in
`TaxonomyBaseReplaceResult.warnings` and do not undo replacement. Apply is
disabled unless validation reports `can_apply = true`.

## Operation and audit commands

Photo rename history and taxonomy history use the same
`OperationSummary`, `OperationAuditRow`, and `OperationPage<T>` DTOs.

| Command family | Parameters | Return |
| --- | --- | --- |
| `list_photo_operations`, `list_taxonomy_operations` | `cursor: Option<String>`, `limit: Option<usize>` | `OperationPage<OperationSummary>` |
| `list_photo_operation_audit`, `list_taxonomy_operation_audit` | `operation_id: i64`, `cursor: Option<String>`, `limit: Option<usize>` | `OperationPage<OperationAuditRow>` |
| `rollback_photo_operation`, `rollback_taxonomy_operation` | `operation_id: i64` | `()` |
| `export_*_operation_audit` | `operation_id: i64`, `destination_path: String` | `()` |
| `export_all_*_operation_audit` | `destination_path: String` | `()` |
| `export_taxonomy_operation_input` | `operation_id: i64` | formatted-input CSV `String` |
| `export_all_replayable_taxonomy_inputs` | none | formatted-input CSV `String` |

Audit exports stream directly to the selected destination. Taxonomy formatted
input is available only when `OperationSummary.has_formatted_input` is true.
Successful rollback deletes the operation, so the history page reloads its
summary cursor after rollback.

## Background operations

`get_operations_status` returns one `OperationState` per operation module.
The desktop observer polls this lightweight interface and displays current
progress in the top toolbar. When a mapping operation changes from running to
successful completion, the observer emits one shared photo/mapping
invalidation so mounted and restored photo views reload consistently.
