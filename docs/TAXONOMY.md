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

Every taxon has exactly one `sci_name`, at most one `zh_name`, and at most one
`en_name`. Synonyms and Chinese or English aliases have no count limit.

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
| `TaxonDetail` | `taxon_id`, `rank`, `parent_taxon_id`, `breadcrumb`, `geological_range`, `names` | Complete editable taxon and ancestor lineage. |

| Function | Parameters after `database` | Return |
| --- | --- | --- |
| `get_taxon_summary` | `taxon_id: i64` | `Option<TaxonSummary>` |
| `get_taxon_detail` | `taxon_id: i64` | `Option<TaxonDetail>` |
| `list_taxon_children` | `taxon_id: i64`, `cursor: Option<&str>`, `limit: usize` | `TaxonomyPage<TaxonChild>` |

`TaxonDetail.breadcrumb` lists ancestors from the highest available rank to
the immediate parent. It does not repeat the current taxon. Children are
loaded separately with `list_taxon_children`; they are not embedded in the
detail response.

## Search

`TaxonSearchResult` contains `taxon_id`, `rank`, accepted display `names`, and
the `TaxonNameMatch` records responsible for the match. `TaxonSuggestion` is
the compact autocomplete type with the same taxon identity fields.

| Function | Parameters after `database` | Return |
| --- | --- | --- |
| `search_taxa` | `query: &str`, `limit: usize` | `Vec<TaxonSearchResult>` |
| `suggest_taxa` | `query: &str`, `limit: usize` | `Vec<TaxonSuggestion>` |

Both interfaces use the same canonical normalization and ranked order:
exact, full prefix, word prefix, substring, then fuzzy. Blank input returns
an empty vector. Candidates from every eligible match level are combined
before one best matching name is selected per `taxon_id`; distinct taxa are
then globally ranked and the requested limit is applied. A stronger match
therefore ranks first without suppressing lower-tier matches that fit within
the final limit. A Chinese genus query ending in `属` also admits its stem as a
lower-ranked fuzzy candidate when the stem is at least three characters long;
the complete query keeps its stronger exact, prefix, and substring ranking.
These taxonomy-only interfaces remain available when the active photo library
is offline. The desktop `search_taxa` and `suggest_taxa` commands execute
database lookups on blocking workers and resolve asynchronously. The desktop
`get_taxon_detail` command uses the same execution model and reports a missing
taxon as an error.

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
kingdom,order,family,genus,species,authority_year,synonyms,zh_name,zh_alias,en_name,en_alias,geological_range,source
```

CSV is UTF-8 and requires a header. Its application-wide delimiter is comma by
default and may be configured as comma, semicolon, tab, or pipe. Columns may be
omitted or reordered, but every row must have the same field count as the
header. Multi-name cells use the separate configured one-character name
separator.

When `species` is present and `genus` is blank, normalization copies the first
word of the species scientific name into `genus`. Every later step therefore
treats that row exactly like a row that supplied both fields.

Matching starts at the lowest supplied rank. The rank name and then each input
synonym are considered in input order, and each name is matched against one
combined set of existing `sci_name` and `synonym` records. Zero candidates
means the target is new; one candidate selects it immediately without checking
any supplied ancestor. Multiple candidates are narrowed using supplied
ancestor names from the nearest rank upward, stopping as soon as one remains.
Ancestor matching uses the same combined scientific-name and synonym rule.
After a target is selected or created, the other supplied scientific names are
appended or supplemented as target synonyms.

Updating a selected target keeps the existing supplement, append, and
overwrite behavior. Creating a target requires its strict parent-rank name.
That parent is resolved with the same zero, one, or multiple-candidate rules:
one candidate is reused without consulting higher ranks, zero candidates
creates that parent, and multiple candidates are narrowed by the nearest
supplied ancestor. Creating a missing parent recursively requires and resolves
its own strict parent, so one formatted row can create every missing rank in a
complete supplied lineage. Missing strict-parent input and unresolved
ambiguity fail the row without applying a partial lineage.

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
| `taxonomy_formatted_update_template` | none | UTF-8 header using the configured CSV delimiter `String` |
| `parse_taxonomy_input_csv` | `input: &str` | `Vec<TaxonInputRow>` |
| `preview_rows` | `rows: &[TaxonInputRow]` | `TaxonomyPreviewResult` |
| `prepare_rows` | `rows: &[TaxonInputRow]` | `PreparedTaxonomyUpdate` |
| `apply_rows` | `rows: &[TaxonInputRow]` | `TaxonomyOperationResult` |
| `apply_prepared_rows` | `prepared: PreparedTaxonomyUpdate` | `TaxonomyOperationResult` |
| `taxonomy_log_csv` | `rows: &[TaxonRowOutcome]` | UTF-8 CSV using the configured delimiter `String` |
| `get_taxonomy_name_separator` | none | `String` |
| `set_taxonomy_name_separator` | `separator: &str` | `()` |

`prepare_rows` evaluates and validates the update in a rolled-back transaction,
retaining its inputs, row outcomes, taxonomy changeset, taxonomy identity, and
operation revision in one in-memory candidate. `apply_prepared_rows` rejects a
stale revision and otherwise applies that precomputed changeset before writing
the operation, audit, replayable input, and synchronization event. It does not
reprocess the formatted rows.

The desktop `preview_taxonomy_rows` command replaces the prior in-memory
candidate and returns `preview_id`, `delimiter`, `encoding`, and `rows`.
`apply_taxonomy_rows` accepts only that `preview_id`; the candidate is consumed
by the attempt, so another apply or an apply after editing requires a new
preview. Both commands execute database work on blocking workers and resolve
asynchronously.

## Direct UI changes

`TaxonUpdateInput` identifies `taxon_id` and supplies editable taxonomy
fields. It is converted to one formatted input row and therefore returns a
normal `TaxonomyOperationResult`.

`PromoteTaxonNameInput` and `DeleteTaxonNameInput` both identify a name by
`taxon_id` plus stable `name_id`.

Promotion exchanges the selected alias or synonym with its accepted-name
record. A species synonym does not need to begin with the accepted scientific
name of the parent genus.

`SaveTaxonNameGroupInput` saves one of the six `TaxonomyNameType` groups. Its
`updates` entries identify existing records by `name_id` and replace their
`authority_year` and `source`; `null` clears the corresponding value. Its
`additions` entries contain `name`, `authority_year`, and `source`. A primary
group accepts at most one name, and an alias or synonym can be added only when
its accepted-name group is present. New species scientific names and synonyms
must start with the accepted scientific name of the parent genus. Blank,
duplicate, mismatched-group, and otherwise invalid additions are rejected
without applying any part of the group save.

| Function | Parameters after `database` | Return | Description |
| --- | --- | --- | --- |
| `update_taxon` | `input: TaxonUpdateInput` | `TaxonomyOperationResult` | Apply one direct edit as formatted input. |
| `promote_taxon_name` | `input: PromoteTaxonNameInput` | `()` | Exchange an alias type with its accepted type. |
| `save_taxon_name_group` | `input: SaveTaxonNameGroupInput` | `()` | Atomically update metadata and append records in one name group. |
| `delete_taxon_name` | `input: DeleteTaxonNameInput` | `()` | Delete a non-`sci_name` record. |
| `delete_taxon` | `taxon_id: i64` | `()` | Delete a childless taxon. |

Group saves, promotion, and deletion create rollbackable audit operations
without formatted input. Desktop direct-change commands execute database work
on blocking workers and resolve asynchronously before scheduling taxonomy
synchronization.

## Custom SQL

Custom SQL and SQL Import use persistent SQL inputs. `SqlInputKind` is `csv`
or `sqlite`. SQLite inputs are read through `alias.object`; CSV inputs are
read as a table named by the alias. Aliases must be valid SQLite identifiers
and cannot use reserved database names. CSV source inspection and loading use
the application-wide CSV delimiter.

| Type | Fields | Description |
| --- | --- | --- |
| `AddSqlInputRequest` | `kind`, `alias`, `path` | Register a source and create a managed copy. |
| `AddSqlInputResult` | `input`, `inputs`, `warnings` | Added source, authoritative source list, and noncritical cleanup warnings. |
| `RemoveSqlInputRequest` | `alias` | Identify one source in the selected workflow. |
| `PersistentSqlInput` | `kind`, `alias`, `original_path`, `available`, `schema` | Registered source, managed-copy availability, and inspected schema. |
| `RemoveSqlInputResult` | `inputs`, `warnings` | Authoritative remaining sources and noncritical cleanup warnings. |

Custom SQL and SQL Import keep separate source registries. Sources persist
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
| `list_custom_sql_database_schemas` | none | `Vec<SqlSourceSchema>` for the readable taxonomy database |
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
read-only query and streams its rows to the destination CSV using the
application-wide delimiter.
Desktop source removal, SQL execution, and query export commands execute file
and database work on blocking workers and resolve asynchronously.
`list_custom_sql_database_schemas` returns the complete non-SQLite-internal
table and view catalog exposed through the `main` alias. Managed input schemas
remain available from `list_custom_sql_inputs`; clients combine both sources
when presenting every table accessible to Custom SQL.

## Taxonomy imports

`TaxonomyImportMetadata` contains `source_path`, `taxa_count`,
`taxon_names_count`, and `imported_at`.
`TaxonomyImportResult` contains the resulting `metadata` and noncritical
cleanup `warnings`.

### SQL Import types

| Type | Fields |
| --- | --- |
| `ValidateSqlImportRequest` | `sql` |
| `SqlImportExecutionResult` | `statements_executed`, `messages`, `script_saved`, `warnings` |
| `NameTypeCount` | `name_type`, `count` |
| `SqlImportIssue` | `code`, `message`, `taxon_id`, `related_taxon_id`, `table`, `row_identifier` |
| `SqlImportValidationResult` | `valid`, `can_apply`, `taxa_count`, `name_counts`, `normalization_changes`, `total_warning_count`, `total_error_count`, `warnings`, `errors` |
| `ValidateSqlImportResult` | `execution`, `validation`, `warnings`, `can_apply` |

Validation returns at most 100 warning and error samples while the total count
fields remain authoritative.

### SQL Import interfaces

| Function | Parameters after `database` | Return |
| --- | --- | --- |
| `get_sql_import_sql` | none | Last successful SQL, or the initial built-in script `String` |
| `list_sql_import_inputs` | none | `Vec<PersistentSqlInput>` |
| `list_sql_import_database_schemas` | none | `Vec<SqlSourceSchema>` for the current taxonomy database |
| `list_sql_import_staging_schemas` | none | `Vec<SqlSourceSchema>` for the current staging database, or an empty vector |
| `add_sql_import_input` | `request: &AddSqlInputRequest` | `AddSqlInputResult` |
| `remove_sql_import_input` | `request: &RemoveSqlInputRequest` | `RemoveSqlInputResult` |
| `validate_sql_import` | `request: &ValidateSqlImportRequest` | `ValidateSqlImportResult` |
| `validate_sql_import_with_progress` | `request: &ValidateSqlImportRequest`, `progress: &mut FnMut(OperationProgress)` | `ValidateSqlImportResult` |
| `apply_sql_import` | none | `TaxonomyImportResult` |

SQL Import has one fixed workspace. Persistent inputs and the last successful
SQL outlive tabs, application restarts, and successful Apply. Adding or
removing an input, validating new SQL, or recreating staging
invalidates the prior staging-dependent candidate and validation state.
Removal is rejected while another operation holds the workspace lock.
`list_sql_import_database_schemas` exposes the current taxonomy catalog through
the read-only `taxonomy` alias for the internal-database catalog. Managed input
schemas remain available from `list_sql_import_inputs` and are presented only
with input sources. `list_sql_import_staging_schemas` separately exposes the
`sql_import` staging catalog when staging exists so clients can present it with
input sources without duplicating it in the internal-database catalog.

Validate first executes SQL Import SQL, which can read the current taxonomy
through the read-only `taxonomy` alias and every managed input through its
registered alias. The script can attach only the backend-selected
`vividarium_sql_import.db` path with the `sql_import` alias. It may create and
mutate staging objects only in `sql_import`; the execution result never creates
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

`apply_sql_import` accepts only the latest successfully validated candidate.
Successful replacement assigns a new taxonomy identity, clears taxonomy
history, and marks every registered photo library for a full remap. It removes
staging, candidate, and validation artifacts while retaining inputs and SQL.
Post-commit cleanup failures are warnings and are queued for later retry. A
failed validation or replacement leaves the current taxonomy unchanged.

### Direct Import types

| Type | Fields |
| --- | --- |
| `DirectImportDatabase` | `source_path`, `schema` |

`schema` is a `SqlSourceSchema` containing the inspection alias and every
visible table or view with its columns.

### Direct Import interfaces

| Function | Parameters after `database` | Return |
| --- | --- | --- |
| `get_taxonomy_import_metadata` | none | `Option<TaxonomyImportMetadata>` |
| `inspect_direct_import_database` | `source_path: &Path` | `DirectImportDatabase` |
| `apply_direct_import` | `source_path: &Path` | `TaxonomyImportResult` |

The supplied SQLite file must contain valid `taxa` and `taxon_names` data
using the current schema. Imported names pass through the shared canonical
normalizer. `inspect_direct_import_database` validates the file, rejects the
active taxonomy database as an input, and returns its canonical path and
schema without changing application data. `apply_direct_import` repeats
validation so a file changed after inspection cannot bypass the checks.
Successful replacement creates a new taxonomy identity, clears
taxonomy user history, and causes every registered photo library to rebuild
mapping state when synchronized. Immediate synchronization and mapping of the
active photo library are best-effort follow-up work; no active library or an
unavailable library does not change a successful replacement result.

The desktop `inspect_direct_import_database` command accepts
`source_path: String`, runs on a blocking worker, and returns
`DirectImportDatabase` directly. The desktop `apply_direct_import` command
accepts `source_path: String` and returns a `direct_import` `OperationState`.
It validates and replaces the database on a blocking worker, returns validation
or file failures through the completed operation error, and schedules
taxonomy/photo synchronization only after replacement commits.

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
deletes the original operation. Desktop history pagination, rollback, audit
export, and formatted-input export execute database and file work on blocking
workers and resolve asynchronously. Desktop audit and formatted-input export
commands accept an absolute destination path and write the CSV after the caller
selects that path. Audit and replayable-input exports use the application-wide
CSV delimiter.
