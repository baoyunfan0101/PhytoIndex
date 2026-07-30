# Taxonomy Backend API

This document describes the public interface in
`vividarium_core::taxonomy`. Shared operation DTOs are documented in
[OPERATIONS.md](OPERATIONS.md), naming hooks in [NAMING.md](NAMING.md), and
database locations in [STORAGE.md](STORAGE.md).

All functions take `&Database` as their first parameter unless noted and
return `CoreResult<T>`. IDs are signed 64-bit integers. Serialized enum values
use `snake_case`.

## Core taxonomy types

`TaxonRank` values are `kingdom`, `order`, `family`, `genus`, and `species`.

`TaxonomyNameType` values are:

| Value | Stored code | Meaning |
| --- | --- | --- |
| `sci_name` | `1` | Unique accepted scientific name. |
| `synonym` | `2` | Scientific synonym. |
| `zh_name` | `3` | Unique accepted Chinese name. |
| `zh_alias` | `4` | Chinese alias. |
| `en_name` | `5` | Unique accepted English name. |
| `en_alias` | `6` | English alias. |

`TaxonomyPage<T>` contains `items` and an opaque `next_cursor`. Pass `None`
for the first page and reuse a returned cursor only with the same interface
and parent resource. Page limits are clamped to `1..=500`.

## Read views

| Type | Fields | Description |
| --- | --- | --- |
| `TaxonDisplayNames` | `sci_name`, `zh_name`, `en_name` | Compact accepted names. |
| `TaxonBreadcrumbItem` | `taxon_id`, `rank`, `names` | One ancestor. |
| `TaxonSummary` | `taxon_id`, `rank`, `breadcrumb`, `names` | Compact taxon and lineage. |
| `TaxonChild` | `taxon_id`, `rank`, `names` | Compact direct child. |
| `TaxonNameDetail` | `name_id`, `name`, `authority_year`, `source` | One stable name record. |
| `TaxonNamesDetail` | `sci_name`, `synonyms`, `zh_name`, `zh_aliases`, `en_name`, `en_aliases` | Names grouped by type. |
| `TaxonDetail` | `taxon_id`, `rank`, `parent_taxon_id`, `geological_range`, `names` | Editable taxon detail. |
| `TaxonDetailNode` | `summary`, `detail`, `children` | Detail plus one child page. |

| Function | Parameters after `database` | Return |
| --- | --- | --- |
| `get_taxon_summary` | `taxon_id: i64` | `Option<TaxonSummary>` |
| `get_taxon_detail` | `taxon_id: i64` | `Option<TaxonDetail>` |
| `get_taxon_detail_node` | `taxon_id: i64`, `children_cursor: Option<&str>`, `children_limit: usize` | `Option<TaxonDetailNode>` |
| `list_taxon_children` | `taxon_id: i64`, `cursor: Option<&str>`, `limit: usize` | `TaxonomyPage<TaxonChild>` |

## Search

`TaxonSearchResult` contains a `summary`, full `detail`, and the
`TaxonNameMatch` records responsible for the match. `TaxonSuggestion` omits
detail and breadcrumb content for autocomplete.

| Function | Parameters after `database` | Return |
| --- | --- | --- |
| `search_taxa` | `query: &str`, `limit: usize` | `Vec<TaxonSearchResult>` |
| `suggest_taxa` | `query: &str`, `limit: usize` | `Vec<TaxonSuggestion>` |

Both interfaces use the same canonical normalization and ranked order:
exact, full prefix, word prefix, substring, then fuzzy. Blank input returns
an empty vector. These taxonomy-only interfaces remain available when the
active photo library is offline.

## Formatted update

### `TaxonInputRow`

| Field | Type | Description |
| --- | --- | --- |
| `kingdom` | `Option<String>` | Kingdom scientific name. |
| `order` | `Option<String>` | Order scientific name. |
| `family` | `Option<String>` | Family scientific name. |
| `genus` | `Option<String>` | Genus scientific name. |
| `species` | `Option<String>` | Species scientific name. |
| `authority_year` | `Option<String>` | Authority text supplied in the row. |
| `synonyms` | `Vec<String>` | Ordered raw scientific synonym strings. |
| `zh_name` | `Option<String>` | First Chinese input name. |
| `zh_alias` | `Vec<String>` | Additional Chinese input names. |
| `en_name` | `Option<String>` | First English input name. |
| `en_alias` | `Vec<String>` | Additional English input names. |
| `geological_range` | `Option<String>` | Geological range for the target taxon. |
| `source` | `Option<String>` | Source applied to supplied names when allowed. |
| `selected_taxon_id` | `Option<i64>` | Exact target used by UI direct edit; not part of CSV. |

The CSV columns are:

```text
kingdom|order|family|genus|species|authority_year|synonyms|zh_name|zh_alias|en_name|en_alias|geological_range|source
```

CSV is UTF-8, pipe-delimited, and requires a header. Columns may be omitted or
reordered, but every row must have the same field count as the header.
Multi-name cells use the configured one-character name separator.

The lowest supplied scientific lineage name and then each supplied synonym
are matched in input order. Each input name searches both existing
`sci_name` and `synonym` records. Once one input name matches, remaining names
are applied as synonyms according to the formatted-update rules.

### Preview and apply types

`TaxonRowStatus` may include `no_change`, `supplement`, `new_name`,
`new_taxon`, `overwrite`, `invalid`, `not_matched`, and
`multiple_candidates`. One row may contain multiple statuses.

`TaxonRowOutcome` contains `row_number`, `operation_types`, target `summary`,
optional `parent`, field-level `changes`, and `message`.

`TaxonomyPreviewResult` contains `delimiter`, `encoding`, and `rows`.
`TaxonomyOperationResult` additionally contains `operation_id`, row totals,
and the same ordered row log.

### Formatted update interfaces

| Function | Parameters after `database` | Return |
| --- | --- | --- |
| `taxonomy_formatted_update_template` | no `database` parameter | UTF-8 pipe-delimited header `String` |
| `parse_taxonomy_input_csv` | `input: &str` | `Vec<TaxonInputRow>` |
| `preview_rows` | `rows: &[TaxonInputRow]` | `TaxonomyPreviewResult` |
| `apply_rows` | `rows: &[TaxonInputRow]` | `TaxonomyOperationResult` |
| `taxonomy_log_csv` | no `database`; `rows: &[TaxonRowOutcome]` | UTF-8 pipe-delimited CSV `String` |
| `get_taxonomy_name_separator` | none | `String` |
| `set_taxonomy_name_separator` | `separator: &str` | `()` |

Preview evaluates the same changes as apply and leaves no stored taxonomy or
operation changes.

## Direct UI changes

`TaxonUpdateInput` identifies `taxon_id` and supplies editable taxonomy
fields. It is converted to one formatted input row and therefore returns a
normal `TaxonomyOperationResult`.

`PromoteTaxonNameInput` and `DeleteTaxonNameInput` both identify a name by
`taxon_id` plus stable `name_id`.

| Function | Parameters after `database` | Return | Description |
| --- | --- | --- | --- |
| `update_taxon` | `input: TaxonUpdateInput` | `TaxonomyOperationResult` | Apply one direct edit as formatted input. |
| `promote_taxon_name` | `input: PromoteTaxonNameInput` | `()` | Exchange an alias type with its accepted type. |
| `delete_taxon_name` | `input: DeleteTaxonNameInput` | `()` | Delete a non-`sci_name` record. |
| `delete_taxon` | `taxon_id: i64` | `()` | Delete a childless taxon. |

Promotion and deletion create rollbackable audit operations without formatted
input.

## Custom SQL

`SqlDataSource` is tagged by `kind`:

| Kind | Fields | Description |
| --- | --- | --- |
| `csv` | `alias: String`, `path: PathBuf` | A temporary read-only table. Headers retain their order and all values are `TEXT`; empty CSV fields remain empty text. |
| `sqlite` | `alias: String`, `path: PathBuf` | A read-only attached SQLite database. |

Aliases must be valid SQLite identifiers, unique within the request, and not a
reserved database name such as `main`, `temp`, or `base`.

`CustomTaxonomySqlRequest` contains `sql`, `sources`, and optional
`maximum_result_rows`. `CustomTaxonomySqlExportRequest` contains one read-only
query in `sql`, `sources`, and an absolute `destination_path`.

### SQL result types

| Type | Fields | Description |
| --- | --- | --- |
| `CustomSqlExecutionResult` | `operation_id`, `changeset_size`, `result_sets`, `messages` | Combined result for the full Custom SQL script. |
| `SqlResultSet` | `statement_index`, `columns`, `rows`, `truncated` | One statement result. |
| `SqlColumn` | `name`, `declared_type` | Result or source column metadata. |
| `SqlStatementMessage` | `statement_index`, `affected_rows`, `message` | Per-statement execution summary. |
| `SqlExportResult` | `path`, `row_count` | Completed streaming CSV export. |

`SqlValue` is tagged by `type` and has the variants `null`, `integer`, `real`,
`text`, and `blob`. Blob values use Base64 in both IPC results and CSV export.
The normal execution limit defaults to and is capped at 1000 rows per result
set. A read-only statement stops after reading the limit plus one row.
Statements that may mutate data always run to completion, including
`UPDATE ... RETURNING`, while retaining only the limited preview rows.

`SqlSourceSchema` contains `alias` and `objects`. Each `SqlSourceObject`
contains `name`, `object_type`, and ordered `columns`. `SqlObjectType` values
are `table`, `view`, and `virtual_table`.

| Function | Parameters after `database` | Return |
| --- | --- | --- |
| `execute_custom_taxonomy_sql` | `request: &CustomTaxonomySqlRequest` | `CustomSqlExecutionResult` |
| `export_custom_taxonomy_query` | `request: &CustomTaxonomySqlExportRequest` | `SqlExportResult` |
| `inspect_sql_data_source` | no `database`; `source: &SqlDataSource` | `SqlSourceSchema` |

Custom SQL may read taxonomy tables, views, search structures, history, and
internal metadata, but it may directly mutate only `taxa` and `taxon_names`.
Schema changes, transaction control, attachment control, internal-table
writes, unsafe pragmas, and extension loading are denied while each statement
executes.

The complete script succeeds or fails as one unit and the resulting taxonomy
must remain valid. A pure query creates no operation. A successful mutation
creates a rollbackable operation without formatted input and records the
required photo-library synchronization. The export interface accepts exactly
one read-only query and writes its rows directly to the destination CSV. CSV
source errors identify the failing input row.

## Base database

`TaxonomyBaseMetadata` contains `source_path`, `taxa_count`,
`taxon_names_count`, and `imported_at`.
`TaxonomyBaseReplaceResult` contains the resulting `metadata`.

### Base import types

`BaseImportSession` contains the stable `session_id` used by all calls for one
isolated import workspace.

| Type | Fields |
| --- | --- |
| `AddBaseImportCsvSourceRequest` | `session_id`, `table_name`, `path` |
| `AddBaseImportSqliteSourceRequest` | `session_id`, `path` |
| `ExecuteBaseImportSqlRequest` | `session_id`, `sql` |
| `BaseImportExecutionResult` | `statements_executed`, `session_revision` |
| `NameTypeCount` | `name_type`, `count` |
| `BaseImportIssue` | `code`, `message`, `table`, `row_identifier` |
| `BaseImportValidationResult` | `can_apply`, `taxa_count`, `name_counts`, `normalization_changes`, `total_warning_count`, `total_error_count`, `warnings`, `errors` |

Validation returns at most 100 warning and error samples while the total count
fields remain authoritative.

### Base import interfaces

| Function | Parameters after `database` | Return |
| --- | --- | --- |
| `create_base_import_session` | none | `BaseImportSession` |
| `add_base_import_csv_source` | `request: &AddBaseImportCsvSourceRequest` | `SqlSourceSchema` |
| `add_base_import_sqlite_source` | `request: &AddBaseImportSqliteSourceRequest` | `SqlSourceSchema` |
| `inspect_base_import_sources` | `session_id: &str` | `Vec<SqlSourceSchema>` |
| `execute_base_import_sql` | `request: &ExecuteBaseImportSqlRequest` | `BaseImportExecutionResult` |
| `validate_base_import` | `session_id: &str` | `BaseImportValidationResult` |
| `apply_base_import` | `session_id: &str` | `TaxonomyBaseReplaceResult` |
| `discard_base_import_session` | `session_id: &str` | `()` |
| `get_default_base_import_sql` | none | `String` |
| `save_default_base_import_sql` | `sql: &str` | `()` |
| `reset_default_base_import_sql` | none | built-in SQL `String` |

An SQLite source becomes the session's `main` database and must be added
before any CSV table. CSV sources become persistent `main.<table_name>` tables
inside the isolated source database. They use their UTF-8 header order, store
every value as `TEXT`, and preserve empty fields as empty text. A bad row
rejects the complete source and reports its row number.

Base Import SQL can read the isolated source and can attach only the
backend-selected `vividarium_base.db` path with the `base` alias. It may create
and mutate staging objects only in `base`; the execution result never creates
a taxonomy operation or return Custom SQL result sets. The script must finish
its transaction and produce `base.taxa` and `base.taxon_names`.

`validate_base_import` separately checks file integrity, foreign keys, required
schema and constraints, supported ranks and name types, canonical
normalization, and the complete taxonomy invariants. The result reports
whether apply is allowed, authoritative totals, and bounded warning and error
samples. Any later source or SQL change requires validation again.

`apply_base_import` accepts only the latest successfully validated session.
Successful replacement assigns a new taxonomy identity, clears taxonomy
history, and marks every registered photo library for a full remap. A failed
validation or replacement leaves the current taxonomy database unchanged.

The default SQL APIs store an exact application-metadata override. Save does
not reformat the text. Reset deletes the override and returns the built-in SQL.

### Direct base database replacement

| Function | Parameters after `database` | Return |
| --- | --- | --- |
| `get_taxonomy_base_metadata` | none | `Option<TaxonomyBaseMetadata>` |
| `replace_taxonomy_base_database` | `source_path: &Path` | `TaxonomyBaseReplaceResult` |

The supplied SQLite file must contain valid `taxa` and `taxon_names` data
using the current schema. Imported names pass through the shared canonical
normalizer. Successful replacement creates a new taxonomy identity, clears
taxonomy user history, and causes every registered photo library to rebuild
mapping state when synchronized. Immediate synchronization and mapping of the
active photo library are best-effort follow-up work; no active library or an
unavailable library does not change a successful replacement result.

## Photo-library synchronization

`TaxonomySyncResult` contains `library_uuid`, `sync_id`,
`queued_photo_count`, and `full_remap`.

`TaxonomySyncRun` contains successful per-library `synchronized` results and
`pending_library_uuids` for libraries that remain unavailable or invalid.

| Function | Parameters after `database` | Return |
| --- | --- | --- |
| `synchronize_pending_photo_libraries` | none | `TaxonomySyncRun` |

Taxonomy mutations commit their data, operation audit, and synchronization
request together, then return without opening photo-library databases.
`synchronize_pending_photo_libraries` processes the active library first,
merges repeated affected-taxon requests, and lets a full-remap request replace
local requests. Its return value separates successful libraries from
unavailable or invalid libraries that remain pending. A library failure never
changes the already successful taxonomy mutation. Registering or activating a
library also checks whether a full remap is required.

## Taxonomy operation interfaces

The taxonomy module exports the common interfaces below:

| Function | Parameters after `database` | Return |
| --- | --- | --- |
| `list_operations` | `cursor: Option<&str>`, `limit: usize` | `OperationPage<OperationSummary>` |
| `list_operation_audit` | `operation_id: i64`, `cursor: Option<&str>`, `limit: usize` | `OperationPage<OperationAuditRow>` |
| `write_operation_audit` | `operation_id: i64`, `writer: &mut W` where `W: Write` | `()` |
| `write_operations_audit` | `operation_ids: &[i64]`, `writer: &mut W` where `W: Write` | `()` |
| `write_all_operation_audit` | `writer: &mut W` where `W: Write` | `()` |
| `rollback_operation` | `operation_id: i64` | `()` |

Taxonomy adds formatted input export:

| Function | Parameters after `database` | Return |
| --- | --- | --- |
| `export_operation_input` | `operation_id: i64` | Formatted-update CSV `String` |
| `export_operations_input` | `operation_ids: &[i64]` | Formatted-update CSV `String` |
| `export_all_replayable_inputs` | none | Formatted-update CSV `String` |

Selected input export fails if any requested operation has no formatted input;
it never silently skips unsupported operations. Successful rollback applies
the reverse changeset, records pending photo-library synchronization, and
deletes the original operation.
