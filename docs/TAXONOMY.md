# Taxonomy Backend Public API

This document describes the public API exported by
`phytoindex_core::taxonomy`. The desktop commands are adapters over the same
types and behavior.

All functions return `CoreResult<T>`. Errors include invalid input, missing
records, database failures, and taxonomy validation failures.

## Common types

`TaxonRank` has the serialized values `kingdom`, `order`, `family`, `genus`,
and `species`.

`TaxonomyNameType` has the serialized values `sci_name`, `synonym`, `zh_name`,
`zh_alias`, `en_name`, and `en_alias`. A taxon has exactly one `sci_name`, and
may have at most one `zh_name` and one `en_name`.

`TaxonomyPage<T>` is used by paginated interfaces:

| Field | Type | Description |
| --- | --- | --- |
| `items` | `Vec<T>` | Current page in interface-defined order. |
| `next_cursor` | `Option<String>` | Opaque cursor for the next page. |

Pass `None` for the first page. A returned cursor must only be reused with the
same interface and parent resource. Page limits are clamped to `1..=500`.

## Read views

The public read models are:

| Type | Fields | Description |
| --- | --- | --- |
| `TaxonDisplayNames` | `sci_name`, `zh_name`, `en_name` | Compact accepted-name view. |
| `TaxonBreadcrumbItem` | `taxon_id`, `rank`, `names` | One ancestor in root-to-parent order. |
| `TaxonSummary` | `taxon_id`, `rank`, `breadcrumb`, `names` | Compact taxon view with lineage. |
| `TaxonChild` | `taxon_id`, `rank`, `names` | Compact direct-child view. |
| `TaxonNameDetail` | `name_id`, `name`, `authority_year`, `source` | One stable name record. |
| `TaxonNamesDetail` | `sci_name`, `synonyms`, `zh_name`, `zh_aliases`, `en_name`, `en_aliases` | Names grouped by type. |
| `TaxonDetail` | `taxon_id`, `rank`, `parent_taxon_id`, `geological_range`, `names` | Complete editable taxon data. |
| `TaxonDetailNode` | `summary`, `detail`, `children` | Detail view with the first or requested child page. |

### `get_taxon_summary`

```rust
pub fn get_taxon_summary(
    database: &Database,
    taxon_id: i64,
) -> CoreResult<Option<TaxonSummary>>
```

`taxon_id` identifies the requested taxon. The return value is `None` when the
taxon does not exist.

### `get_taxon_detail`

```rust
pub fn get_taxon_detail(
    database: &Database,
    taxon_id: i64,
) -> CoreResult<Option<TaxonDetail>>
```

`taxon_id` identifies the requested taxon. The return value is `None` when the
taxon does not exist.

### `get_taxon_detail_node`

```rust
pub fn get_taxon_detail_node(
    database: &Database,
    taxon_id: i64,
    children_cursor: Option<&str>,
    children_limit: usize,
) -> CoreResult<Option<TaxonDetailNode>>
```

`children_cursor` selects a child page and `children_limit` requests its size.
The return value is `None` when the taxon does not exist.

### `list_taxon_children`

```rust
pub fn list_taxon_children(
    database: &Database,
    taxon_id: i64,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<TaxonomyPage<TaxonChild>>
```

Returns direct children of `taxon_id`. `cursor` and `limit` control
pagination.

## Search

```rust
pub fn search_taxa(
    database: &Database,
    query: &str,
    limit: usize,
) -> CoreResult<Vec<TaxonSearchResult>>
```

`query` is matched against taxonomy names. Blank input returns an empty list.
`limit` is clamped to `1..=500`.

Each `TaxonSearchResult` contains:

| Field | Type | Description |
| --- | --- | --- |
| `summary` | `TaxonSummary` | Compact result and lineage. |
| `detail` | `TaxonDetail` | Full taxonomy data. |
| `matches` | `Vec<TaxonNameMatch>` | Name records responsible for the match. |

`TaxonNameMatch` contains `name_id`, `name_type`, and `name`. Results prefer
exact and prefix matches before broader and fuzzy matches.

## Formatted input

### Input model

`TaxonInputRow` is the structured form accepted by preview and apply:

| Field | Type | Description |
| --- | --- | --- |
| `kingdom` | `Option<String>` | Kingdom scientific name. |
| `order` | `Option<String>` | Order scientific name. |
| `family` | `Option<String>` | Family scientific name. |
| `genus` | `Option<String>` | Genus scientific name. |
| `species` | `Option<String>` | Species scientific name. |
| `authority_year` | `Option<String>` | Authority text paired with the target scientific-name value in this row. |
| `synonyms` | `Vec<String>` | Ordered scientific synonyms, optionally including authority text. |
| `zh_name` | `Option<String>` | First Chinese input name. |
| `zh_alias` | `Vec<String>` | Additional Chinese input names. |
| `en_name` | `Option<String>` | First English input name. |
| `en_alias` | `Vec<String>` | Additional English input names. |
| `geological_range` | `Option<String>` | Geological range of the target taxon. |
| `source` | `Option<String>` | Source used for names in this row. |
| `selected_taxon_id` | `Option<i64>` | Exact locator reserved for `update_taxon`; CSV clients omit it. |

The CSV columns are:

```text
kingdom|order|family|genus|species|authority_year|synonyms|zh_name|zh_alias|en_name|en_alias|geological_range|source
```

CSV is UTF-8, uses `|` between columns, and requires a header. Columns may be
omitted or reordered. Every data row must have the same number of fields as
the header.

### `taxonomy_formatted_update_template`

```rust
pub fn taxonomy_formatted_update_template() -> CoreResult<String>
```

Returns a UTF-8, pipe-delimited, header-only CSV template.

### `parse_taxonomy_input_csv`

```rust
pub fn parse_taxonomy_input_csv(
    database: &Database,
    input: &str,
) -> CoreResult<Vec<TaxonInputRow>>
```

`input` is a complete CSV document. The return value preserves data-row
order. Unknown or duplicate headers, malformed CSV, and rows with inconsistent
field counts are rejected.

### Name separator metadata

```rust
pub fn get_taxonomy_name_separator(
    database: &Database,
) -> CoreResult<String>

pub fn set_taxonomy_name_separator(
    database: &Database,
    separator: &str,
) -> CoreResult<()>
```

The separator splits `synonyms`, `zh_alias`, and `en_alias` cells. The default
is `;`. `set_taxonomy_name_separator` accepts exactly one non-whitespace
character other than `|`.

### Scientific-name authority parser

```rust
pub fn split_scientific_name_authority(
    value: &str,
) -> ScientificNameParts
```

`value` is one scientific-name string from a synonym cell. The return value
contains `name: String` and `authority_year: Option<String>`, preserving the
original characters within both parts after trimming the outer input.

The parser starts authority text at the first applicable word:

1. a word containing `(`;
2. the second word whose first character is uppercase;
3. an independent `de`, `von`, or `van` word.

This parser is a standalone public interface because its rules can evolve
independently from the formatted-update workflow.

### Matching and update behavior

The lowest supplied rank is the target. Supplied higher ranks narrow matching
but do not need to form a continuous path from `kingdom`. Creating a
non-kingdom taxon requires its direct parent. A missing genus may be derived
from the first word of a species name.

Input matching priority is the target scientific name, then each synonym in
input order. For each input name, database `sci_name` records are considered
before database `synonym` records. The first priority level producing matches
determines the result.

The row's `authority_year` is paired with its target scientific-name value.
Authority text parsed from a synonym is paired with that synonym. After a
taxon match, the paired authority may supplement or overwrite the matched
name's authority. Other input scientific names are processed in priority
order as synonyms.

Formatted updates can create taxa, append names, supplement empty fields, and
overwrite existing fields. They never switch an accepted name with an alias.

Chinese inputs are processed as one ordered list: `zh_name` first, then
`zh_alias`. If the taxon lacks `zh_name`, the first new value becomes
`zh_name`; later values are aliases. Otherwise all new values are aliases.
English inputs follow the same rule.

`source` fills an empty existing source or initializes a new name. It does not
overwrite a non-empty source.

## Preview and apply

```rust
pub fn preview_rows(
    database: &Database,
    rows: &[TaxonInputRow],
) -> CoreResult<TaxonomyPreviewResult>

pub fn apply_rows(
    database: &Database,
    rows: &[TaxonInputRow],
) -> CoreResult<TaxonomyOperationResult>
```

Both interfaces evaluate rows in input order. `preview_rows` rolls back all
evaluated changes and returns:

| Field | Type | Description |
| --- | --- | --- |
| `delimiter` | `String` | Log delimiter, currently `|`. |
| `encoding` | `String` | Log encoding, currently `UTF-8`. |
| `rows` | `Vec<TaxonRowOutcome>` | One outcome per input row. |

`apply_rows` commits successful rows and stores one operation. Invalid,
unmatched, or ambiguous rows fail independently; other rows in the operation
may succeed. Its return value contains:

| Field | Type | Description |
| --- | --- | --- |
| `operation_id` | `i64` | Stored operation identifier. |
| `total_rows` | `usize` | Number of attempted rows. |
| `succeeded_rows` | `usize` | Rows without failure statuses. |
| `failed_rows` | `usize` | Rows with a failure status. |
| `delimiter` | `String` | Log delimiter. |
| `encoding` | `String` | Log encoding. |
| `rows` | `Vec<TaxonRowOutcome>` | Stored result log. |

A `TaxonRowOutcome` contains:

| Field | Type | Description |
| --- | --- | --- |
| `row_number` | `usize` | One-based input row number. |
| `operation_types` | `Vec<TaxonRowStatus>` | All statuses applying to the row. |
| `message` | `String` | Human-readable result or failure reason. |
| `target` | `Option<TaxonSummary>` | Matched or resulting taxon summary. |
| `parent` | `Option<TaxonSummary>` | Parent summary when a taxon is created. |
| `candidates` | `Vec<TaxonSummary>` | Ambiguous matches, when present. |
| `changes` | `Vec<TaxonChange>` | Structured field-level changes. |

`TaxonRowStatus` serializes as `no_change`, `supplement`, `new_name`,
`new_taxon`, `overwrite`, `invalid`, `not_matched`, or
`multiple_candidates`. A row may contain more than one status.

Each `TaxonChange` contains `kind`, `field`, `old_value`, and `new_value`.
`kind` serializes as `create_taxon`, `append_name`, `supplement`, or
`overwrite`.

### `taxonomy_log_csv`

```rust
pub fn taxonomy_log_csv(
    rows: &[TaxonRowOutcome],
) -> CoreResult<String>
```

Returns the supplied row outcomes as UTF-8, pipe-delimited CSV. Structured
status, summary, parent, and change values are JSON strings inside CSV cells.

## Direct actions

### `update_taxon`

```rust
pub fn update_taxon(
    database: &Database,
    input: TaxonUpdateInput,
) -> CoreResult<TaxonomyOperationResult>
```

`TaxonUpdateInput` contains `taxon_id`, `authority_year`, `synonyms`,
`zh_name`, `zh_alias`, `en_name`, `en_alias`, `geological_range`, and
`source`. The function resolves the selected taxon and lineage, converts the
request into one formatted input row, validates it without a separate preview,
and applies it as one operation.

The exact selected ID is not stored in operation input and is not included in
later exports.

### `promote_taxon_name`

```rust
pub fn promote_taxon_name(
    database: &Database,
    input: PromoteTaxonNameInput,
) -> CoreResult<()>
```

`PromoteTaxonNameInput` contains `taxon_id` and `name_id`. The name must
belong to that taxon. Promotion swaps the selected alias type with the current
accepted type: `synonym` with `sci_name`, `zh_alias` with `zh_name`, or
`en_alias` with `en_name`.

For a species synonym, the first word must exactly equal the parent genus
scientific name. Promotion is unlogged and cannot be reverted through
operation history.

### `delete_taxon_name`

```rust
pub fn delete_taxon_name(
    database: &Database,
    input: DeleteTaxonNameInput,
) -> CoreResult<()>
```

`DeleteTaxonNameInput` contains `taxon_id` and `name_id`. The name must belong
to that taxon. The unique `sci_name` cannot be deleted. Deletion is unlogged.

### `delete_taxon`

```rust
pub fn delete_taxon(
    database: &Database,
    taxon_id: i64,
) -> CoreResult<()>
```

Deletes the identified taxon when it has no children. The action is unlogged.

### `execute_custom_taxonomy_sql`

```rust
pub fn execute_custom_taxonomy_sql(
    database: &Database,
    sql: &str,
    input: Option<TaxonomyCustomSqlTempTable>,
) -> CoreResult<TaxonomyCustomSqlResult>
```

`sql` is the authorized SQL batch to execute. Optional tabular input contains
`columns: Vec<String>` and `rows: Vec<Vec<String>>` and is exposed to the SQL
batch as its temporary input table.

The return value contains `changeset_size`, the byte size of the applied
taxonomy changeset. The action is transactional and validates taxonomy
integrity before commit. It is unlogged and cannot be reverted through
operation history.

## Operation history

`TaxonomyOperation` contains:

| Field | Type | Description |
| --- | --- | --- |
| `operation_id` | `i64` | Operation identifier. |
| `source` | `TaxonomyOperationSource` | Source, currently `formatted_update`. |
| `input` | `Vec<TaxonInputRow>` | Ordered attempted input without transient IDs. |
| `result` | `TaxonomyOperationResult` | Exact apply log. |
| `changeset_size` | `usize` | Stored changeset size in bytes. |
| `applied_at` | `String` | Database-generated apply timestamp. |

### `list_taxonomy_operations`

```rust
pub fn list_taxonomy_operations(
    database: &Database,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<TaxonomyPage<TaxonomyOperation>>
```

Returns newest operations first. `cursor` and `limit` control pagination.

### `get_taxonomy_operation`

```rust
pub fn get_taxonomy_operation(
    database: &Database,
    operation_id: i64,
) -> CoreResult<Option<TaxonomyOperation>>
```

Returns `None` when the operation does not exist.

### `export_taxonomy_operation_inputs`

```rust
pub fn export_taxonomy_operation_inputs(
    database: &Database,
    operation_ids: &[i64],
) -> CoreResult<OperationInputTable>
```

Returns `columns: Vec<String>` and `rows: Vec<Vec<String>>` in formatted-input
column order. Operations are exported in ascending ID order, and rows preserve
their original order. The output records attempted input, not only successful
changes. Transient selected IDs are excluded.

### `revert_taxonomy_operation`

```rust
pub fn revert_taxonomy_operation(
    database: &Database,
    operation_id: i64,
) -> CoreResult<()>
```

Reverts the complete operation in one transaction. A conflict fails the whole
revert. A successful revert deletes the operation from history.

## Base database

### `get_taxonomy_base_metadata`

```rust
pub fn get_taxonomy_base_metadata(
    database: &Database,
) -> CoreResult<Option<TaxonomyBaseMetadata>>
```

Returns `None` before a base database has been imported. Otherwise the
metadata contains `source_path`, `taxa_count`, `taxon_names_count`, and
`imported_at`.

### `replace_taxonomy_base_database`

```rust
pub fn replace_taxonomy_base_database(
    database: &Database,
    source_path: &Path,
) -> CoreResult<TaxonomyBaseReplaceResult>
```

`source_path` identifies the external SQLite base database. It must differ
from the application database and contain valid taxonomy data in the expected
base-database layout.

Replacement discards the current taxonomy, taxonomy operation history, and
mapping state, then imports the external taxon and name IDs. The return value
contains:

| Field | Type | Description |
| --- | --- | --- |
| `metadata` | `TaxonomyBaseMetadata` | Metadata for the imported base database. |
| `queued_photo_count` | `i64` | Number of photos queued for global remapping. |

The desktop adapter starts processing the global mapping queue asynchronously
after replacement succeeds.

## Desktop command adapters

The desktop layer supplies `Database` through application state. Its public
taxonomy commands are:

| Command | Explicit parameters | Return value |
| --- | --- | --- |
| `search_taxa` | `query: String`, `limit: Option<usize>` | `Vec<TaxonSearchResult>` |
| `get_taxon_detail_node` | `taxon_id: i64`, `children_cursor: Option<String>`, `children_limit: Option<usize>` | `TaxonDetailNode`; missing taxa are errors |
| `list_taxon_children` | `taxon_id: i64`, `cursor: Option<String>`, `limit: Option<usize>` | `TaxonomyPage<TaxonChild>` |
| `update_taxon` | `input: TaxonUpdateInput` | `TaxonomyOperationResult` |
| `promote_taxon_name` | `input: PromoteTaxonNameInput` | `()` |
| `delete_taxon_name` | `input: DeleteTaxonNameInput` | `()` |
| `delete_taxon` | `taxon_id: i64` | `()` |
| `execute_custom_taxonomy_sql` | `sql: String`, `input: Option<TaxonomyCustomSqlTempTable>` | `TaxonomyCustomSqlResult` |
| `preview_taxonomy_rows` | `rows: Vec<TaxonInputRow>` | `TaxonomyPreviewResult` |
| `apply_taxonomy_rows` | `rows: Vec<TaxonInputRow>` | `TaxonomyOperationResult` |
| `parse_taxonomy_input_csv` | `input: String` | `Vec<TaxonInputRow>` |
| `get_taxonomy_formatted_update_template` | none | UTF-8 CSV `String` |
| `export_taxonomy_log` | `rows: Vec<TaxonRowOutcome>` | UTF-8 CSV `String` |
| `get_taxonomy_name_separator` | none | `String` |
| `set_taxonomy_name_separator` | `separator: String` | `()` |
| `list_taxonomy_operations` | `cursor: Option<String>`, `limit: Option<usize>` | `TaxonomyPage<TaxonomyOperation>` |
| `get_taxonomy_operation` | `operation_id: i64` | `TaxonomyOperation`; missing operations are errors |
| `revert_taxonomy_operation` | `operation_id: i64` | `()` |
| `export_taxonomy_operation_inputs` | `operation_ids: Vec<i64>` | `OperationInputTable` |
| `get_taxonomy_base_metadata` | none | `Option<TaxonomyBaseMetadata>` |
| `replace_taxonomy_base_database` | `source_path: String` | Asynchronous operation handle; final result contains replacement and mapping results |

Optional desktop page limits default to `50`.
