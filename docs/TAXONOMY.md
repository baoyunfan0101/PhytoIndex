# Taxonomy Knowledge Base Backend API

This document describes the public Rust API in
`phytoindex_core::taxonomy` and its Tauri command adapters.

## Data model

The taxonomy domain uses two canonical tables:

- `taxa(taxon_id, parent_taxon_id, rank, geological_range)`
- `taxon_names(name_id, taxon_id, name_kind, name, normalized_name,
  is_accepted, authority_year, category, source)`

The internal database schema version remains `2`. Databases with any other
schema version are rejected; no schema migration is performed.

`taxon_id` is the external base-database identifier. The schema does not
contain a `taxon_identifiers` table.

New local taxa use SQLite IDs above `8000000000000000`, leaving the ordinary
external-ID range available to imported base databases. This value remains
below the largest exactly representable JavaScript integer.

Ranks are serialized as `kingdom`, `order`, `family`, `genus`, and `species`.
Name kinds are serialized as `scientific`, `english`, and `chinese`.

## Read API

The main read functions are:

```rust
pub fn search_taxa(
    database: &Database,
    query: &str,
    limit: usize,
) -> CoreResult<Vec<TaxonSearchResult>>

pub fn get_taxon_summary(
    database: &Database,
    taxon_id: i64,
) -> CoreResult<Option<TaxonSummary>>

pub fn get_taxon_detail(
    database: &Database,
    taxon_id: i64,
) -> CoreResult<Option<TaxonDetail>>

pub fn get_taxon_detail_node(
    database: &Database,
    taxon_id: i64,
    children_cursor: Option<&str>,
    children_limit: usize,
) -> CoreResult<Option<TaxonDetailNode>>

pub fn list_taxon_children(
    database: &Database,
    taxon_id: i64,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<TaxonomyPage<TaxonChild>>
```

`TaxonSummary` contains the current taxon, its compact display names, and a
root-to-parent breadcrumb. `TaxonDetail` contains parentage, geological range,
and all scientific, English, and Chinese name records. It no longer contains
an `identifiers` field.

Search normalizes whitespace and evaluates exact, prefix, word-prefix,
substring, and fuzzy matches. Page and search limits are clamped to
`1..=500`.

## Formatted updates

The old quick-update name is replaced by formatted update. Adapters parse a
workbook or table into `TaxonInputRow` values:

```rust
pub struct TaxonInputRow {
    pub selected_taxon_id: Option<i64>,
    pub kingdom: Option<String>,
    pub order: Option<String>,
    pub family: Option<String>,
    pub genus: Option<String>,
    pub species: Option<String>,
    pub geological_range: Option<String>,
    pub scientific: Option<TaxonNameInput>,
    pub english: Option<TaxonNameInput>,
    pub chinese: Option<TaxonNameInput>,
}
```

The deepest rank name locates the target. Higher rank names narrow ambiguous
lineages. `selected_taxon_id` may resolve a preview ambiguity, but it is never
stored in operation input or exported for rebase.

`TaxonNameInput` contains `name`, optional `is_accepted`,
`authority_year`, `category`, and `source`.

`TaxonUpdateOptions` has four flags:

- `allow_new_names`
- `allow_new_taxa`
- `allow_overwrite`
- `allow_switch_accepted_name`

Preview performs no persistent write:

```rust
pub fn preview_rows(
    database: &Database,
    rows: &[TaxonInputRow],
    options: TaxonUpdateOptions,
) -> CoreResult<TaxonomyPreviewResult>
```

Apply creates exactly one operation for one save, including when every row
fails or makes no change:

```rust
pub fn apply_rows(
    database: &Database,
    rows: &[TaxonInputRow],
    options: TaxonUpdateOptions,
) -> CoreResult<TaxonomyOperationResult>
```

Rows are evaluated in input order in one transaction. Valid rows may succeed
while invalid, missing, ambiguous, conflicting, or unchanged rows are logged
as failures. A database/runtime failure aborts the entire transaction.

`TaxonomyOperationResult` contains:

| Field | Description |
| --- | --- |
| `operation_id` | The one operation created for this save. |
| `total_rows` | Number of input rows. |
| `succeeded_rows` | Rows that changed taxonomy data. |
| `failed_rows` | Rows that failed or made no change. |
| `rows` | Ordered `TaxonomyOperationRowLog` entries. |

Successful row logs contain the target `scientific_name`, structured
`changes`, and a message describing each taxon or name field added or changed.
Failed messages contain the reason and include candidate scientific names for
ambiguous locators. Detailed target and candidate objects are preview-only and
are not returned or stored as execution results.

## Direct search-page actions

```rust
pub fn update_taxon(
    database: &Database,
    input: TaxonUpdateInput,
    options: TaxonUpdateOptions,
) -> CoreResult<TaxonomyOperationResult>
```

A direct update first converts the selected taxon and its current lineage into
one formatted input row. The exact taxon ID is used only while executing the
save and is stripped from stored/exported operation input.

Deletion is intentionally destructive and has no operation log or rollback:

```rust
pub fn delete_taxon_name(
    database: &Database,
    input: DeleteTaxonNameInput,
) -> CoreResult<()>

pub fn delete_taxon(
    database: &Database,
    taxon_id: i64,
) -> CoreResult<()>
```

Deleting a taxon still uses the taxonomy trigger to mark affected photo
mappings stale and queue them for remapping.

## Custom SQL

```rust
pub fn execute_custom_taxonomy_sql(
    database: &Database,
    sql: &str,
    input: Option<TaxonomyCustomSqlTempTable>,
) -> CoreResult<TaxonomyCustomSqlResult>
```

Custom SQL remains transactional, authorization-limited to taxonomy tables,
and fully taxonomy-validated before commit. Optional input creates
`temp.input` for the call. The result exposes the in-memory changeset size
used to determine affected mapping scope.

Custom SQL creates no operation log and cannot be reverted.

## Operation history, rollback, and rebase export

There is no batch/operation hierarchy. One formatted save is one
`TaxonomyOperation`:

| Field | Description |
| --- | --- |
| `operation_id` | Operation ID. |
| `source` | Currently always `formatted_update`. |
| `options` | Update options used by the save. |
| `input` | Ordered formatted rows with `selected_taxon_id = null`. |
| `result` | The exact returned operation result/log. |
| `status` | `applied` or `reverted`. |
| `changeset_size` | Size of the private operation-wide SQLite changeset. |
| `applied_at` | Apply timestamp. |
| `reverted_at` | Revert timestamp, if any. |

History APIs:

```rust
pub fn list_taxonomy_operations(
    database: &Database,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<TaxonomyPage<TaxonomyOperation>>

pub fn get_taxonomy_operation(
    database: &Database,
    operation_id: i64,
) -> CoreResult<Option<TaxonomyOperation>>

pub fn revert_taxonomy_operation(
    database: &Database,
    operation_id: i64,
) -> CoreResult<()>

pub fn export_taxonomy_operation_inputs(
    database: &Database,
    operation_ids: &[i64],
) -> CoreResult<OperationInputTable>
```

Rollback applies the inverse of the complete operation changeset in one
transaction. It therefore succeeds as a whole or fails without a partial
rollback. Later conflicting changes can block rollback.

Export sorts selected operations by original operation ID, keeps each
operation's original row order, and returns the formatted-input column layout:

```text
kingdom, order, family, genus, species, geological_range,
scientific_name, scientific_is_accepted, scientific_authority_year,
scientific_category, scientific_source,
english_name, english_is_accepted, english_authority_year,
english_category, english_source,
chinese_name, chinese_is_accepted, chinese_authority_year,
chinese_category, chinese_source
```

The export records attempted input, not only successful changes. It contains
no `selected_taxon_id`, result target ID, or other current-database identity,
and rebase is intentionally allowed to reproduce only part of the old edits.

## Base database replacement

An external SQLite `.db` is accepted when it contains the canonical `taxa`
and `taxon_names` column layouts.

```rust
pub fn replace_taxonomy_base_database(
    database: &Database,
    source_path: &Path,
) -> CoreResult<TaxonomyBaseReplaceResult>

pub fn get_taxonomy_base_metadata(
    database: &Database,
) -> CoreResult<Option<TaxonomyBaseMetadata>>
```

Replacement is one database transaction. It:

1. clears current photo mappings, usage, and mapping queue;
2. clears taxonomy operation history and previous base metadata;
3. clears old taxa and names;
4. copies external taxon and name IDs without remapping them;
5. validates the complete imported taxonomy;
6. restores the high local-ID sequence floor;
7. queues every indexed photo with reason `taxonomy`.

The desktop command runs replacement and then processes the complete mapping
queue in the background. A mapping failure does not restore the old taxonomy;
remaining queue state can be processed again.

`TaxonomyBaseMetadata` contains the canonical source path, imported taxon and
name counts, and import timestamp.

## Tauri commands

JavaScript invoke argument names use camel case. Serialized model fields remain
snake case.

| Command | Arguments | Return |
| --- | --- | --- |
| `search_taxa` | `query`, optional `limit` | `TaxonSearchResult[]` |
| `get_taxon_detail_node` | `taxonId`, optional child cursor/limit | `TaxonDetailNode` |
| `list_taxon_children` | `taxonId`, optional cursor/limit | `TaxonomyPage<TaxonChild>` |
| `preview_taxonomy_rows` | `rows`, optional `options` | `TaxonomyPreviewResult` |
| `apply_taxonomy_rows` | `rows`, optional `options` | `TaxonomyOperationResult` |
| `update_taxon` | `input`, optional `options` | `TaxonomyOperationResult` |
| `delete_taxon_name` | `input` | `null` |
| `delete_taxon` | `taxonId` | `null` |
| `execute_custom_taxonomy_sql` | `sql`, optional `input` | `TaxonomyCustomSqlResult` |
| `list_taxonomy_operations` | optional cursor/limit | `TaxonomyPage<TaxonomyOperation>` |
| `get_taxonomy_operation` | `operationId` | `TaxonomyOperation` |
| `revert_taxonomy_operation` | `operationId` | `null` |
| `export_taxonomy_operation_inputs` | `operationIds` | `OperationInputTable` |
| `get_taxonomy_base_metadata` | none | `TaxonomyBaseMetadata \| null` |
| `replace_taxonomy_base_database` | `sourcePath` | `{ operation: OperationState }` |
