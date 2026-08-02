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

A kingdom is a root taxon. Every other taxon has an existing parent whose
rank is strictly higher than the child rank. Intermediate ranks may be
omitted; equal-rank, reversed-rank, missing-parent, and cyclic relationships
are invalid.

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

Custom SQL and Base Import use persistent SQL inputs. `SqlInputKind` is `csv`
or `sqlite`. SQLite inputs are read through `alias.object`; CSV inputs are
read as a table named by the alias. Aliases must be valid SQLite identifiers
and cannot use reserved database names.

| Type | Fields | Description |
| --- | --- | --- |
| `AddSqlInputRequest` | `kind`, `alias`, `path` | Register a source and create a managed copy. |
| `AddSqlInputResult` | `input`, `inputs`, `warnings` | Added source, authoritative source list, and noncritical cleanup warnings. |
| `RemoveSqlInputRequest` | `alias` | Identify one source in the selected workflow. |
| `PersistentSqlInput` | `kind`, `alias`, `original_path`, `available`, `schema` | Registered source, managed-copy availability, and inspected schema. |
| `RemoveSqlInputResult` | `inputs`, `warnings` | Authoritative remaining sources and noncritical cleanup warnings. |

Custom SQL and Base Import keep separate source registries. Sources persist
across database reopen until explicitly removed. Once registry removal
commits, managed-file cleanup errors are warnings and do not change success.

`CustomTaxonomySqlRequest` contains one statement in `sql` and optional
`maximum_result_rows`. `CustomTaxonomySqlExportRequest` contains one read-only
query in `sql` and an absolute `destination_path`.

### SQL result types

| Type | Fields | Description |
| --- | --- | --- |
| `CustomSqlExecutionResult` | `operation_id`, `changeset_size`, `result_sets`, `messages`, `script_saved`, `warnings` | Committed SQL result and independent script-save status. |
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
| `get_custom_taxonomy_sql` | none | Last successful SQL, or the initial built-in script `String` |
| `list_custom_sql_inputs` | none | `Vec<PersistentSqlInput>` |
| `add_custom_sql_input` | `request: &AddSqlInputRequest` | `AddSqlInputResult` |
| `remove_custom_sql_input` | `request: &RemoveSqlInputRequest` | `RemoveSqlInputResult` |
| `execute_custom_taxonomy_sql` | `request: &CustomTaxonomySqlRequest` | `CustomSqlExecutionResult` |
| `export_custom_taxonomy_query` | `request: &CustomTaxonomySqlExportRequest` | `SqlExportResult` |

Custom SQL may read taxonomy tables, views, search structures, history, and
internal metadata, but it may directly mutate only `taxa` and `taxon_names`.
Schema changes, transaction control, attachment control, internal-table
writes, unsafe pragmas, and extension loading are denied while each statement
executes.

The statement succeeds or fails as one unit and the resulting taxonomy must
remain valid. A pure query creates no operation. A successful mutation creates
a rollbackable operation without formatted input and records photo-library
synchronization. SQL is saved only after prepare, execution, and transaction
commit succeed. A script-save failure returns the committed execution with
`script_saved = false` and a warning; an execution failure does not replace
the last successful SQL. Export accepts exactly one
read-only query and streams its rows to the destination CSV.

## Base database

`TaxonomyBaseMetadata` contains `source_path`, `taxa_count`,
`taxon_names_count`, and `imported_at`.
`TaxonomyBaseReplaceResult` contains the resulting `metadata` and noncritical
cleanup `warnings`.

### Base import types

| Type | Fields |
| --- | --- |
| `ValidateBaseImportRequest` | `sql` |
| `BaseImportExecutionResult` | `statements_executed`, `messages`, `script_saved`, `warnings` |
| `NameTypeCount` | `name_type`, `count` |
| `BaseImportIssue` | `code`, `message`, `taxon_id`, `related_taxon_id`, `table`, `row_identifier` |
| `BaseImportValidationResult` | `valid`, `can_apply`, `taxa_count`, `name_counts`, `normalization_changes`, `total_warning_count`, `total_error_count`, `warnings`, `errors` |
| `ValidateBaseImportResult` | `execution`, `validation`, `warnings`, `can_apply` |

Validation returns at most 100 warning and error samples while the total count
fields remain authoritative.

### Base import interfaces

| Function | Parameters after `database` | Return |
| --- | --- | --- |
| `get_base_import_sql` | none | Last successful SQL, or the initial built-in script `String` |
| `list_base_import_inputs` | none | `Vec<PersistentSqlInput>` |
| `add_base_import_input` | `request: &AddSqlInputRequest` | `AddSqlInputResult` |
| `remove_base_import_input` | `request: &RemoveSqlInputRequest` | `RemoveSqlInputResult` |
| `validate_base_import` | `request: &ValidateBaseImportRequest` | `ValidateBaseImportResult` |
| `validate_base_import_with_progress` | `request: &ValidateBaseImportRequest`, `progress: &mut FnMut(OperationProgress)` | `ValidateBaseImportResult` |
| `apply_base_import` | none | `TaxonomyBaseReplaceResult` |

Base Import has one fixed workspace. Persistent inputs and the last successful
SQL outlive tabs, application restarts, and successful Apply. Adding or
removing an input, validating new SQL, or recreating staging
invalidates the prior staging-dependent candidate and validation state.
Removal is rejected while another operation holds the workspace lock.

Validate first executes Base Import SQL, which can read the isolated source
and can attach only the
backend-selected `vividarium_base.db` path with the `base` alias. It may create
and mutate staging objects only in `base`; the execution result never creates
a taxonomy operation or returns Custom SQL result sets. It reports only
per-statement messages or a syntax/runtime error. After a fully successful
script commits, script persistence is attempted separately. Save failure is
reported through `script_saved = false` and `warnings` without changing the
execution result. A failed execution restores the prior staging and validation
artifacts and stops before candidate validation.

After SQL succeeds, validation builds the candidate and checks file integrity,
required schema and constraints, supported ranks and name types, canonical
normalization, and the complete taxonomy invariants. Taxonomy data violations
are returned with `valid = false`, `can_apply = false`, and structured issues;
SQL, SQLite, file, and candidate-build failures remain interface errors. The
result contains authoritative totals and bounded warning and error samples.
Any later source or SQL change requires validation again.

The progress callback reports a stage plus optional row counts and SQL
statement indexes. Stages cover input preparation, SQL execution, staging,
name normalization, candidate taxa and names, taxonomy validation, and the
terminal validation result. Missing counts mean that only the stage is known;
they do not represent a percentage.

`apply_base_import` accepts only the latest successfully validated candidate.
Successful replacement assigns a new taxonomy identity, clears taxonomy
history, and marks every registered photo library for a full remap. It removes
staging, candidate, and validation artifacts while retaining inputs and SQL.
Post-commit cleanup failures are warnings and are queued for later retry. A
failed validation or replacement leaves the current taxonomy unchanged.

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
