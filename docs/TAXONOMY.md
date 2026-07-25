# Taxonomy Knowledge Base Backend API

This document describes the public Rust API in `phytoindex_core::taxonomy`
and its Tauri command adapters.

## Data model

The canonical taxonomy tables are:

- `taxa(taxon_id, parent_taxon_id, rank, geological_range)`
- `taxon_names(name_id, taxon_id, name_type, name, normalized_name,
  authority_year, source)`

The database schema version remains `2`. A database with any other schema
version is rejected. There are no migrations.

Imported base databases own their `taxon_id` values. New local taxa use
SQLite IDs above `8000000000000000`, which keeps imported identifier ranges
available.

Ranks are `kingdom`, `order`, `family`, `genus`, and `species`. Name types are:

- `sci_name`: the one scientific name for a taxon
- `synonym`: a scientific synonym
- `zh_name`: the optional one accepted Chinese name
- `zh_alias`: a Chinese alias
- `en_name`: the optional one accepted English name
- `en_alias`: an English alias

`TaxonNamesDetail` exposes the same six groups. Each detail record contains
`name`, optional `authority_year`, and optional `source`.

## Formatted input

The fixed input columns are:

```text
kingdom|order|family|genus|species|authority_year|synonyms|zh_name|zh_alias|en_name|en_alias|geological_range|source
```

CSV input and output use UTF-8 and `|` as the column delimiter. A CSV input
must have a header. Columns may be omitted or reordered, but every supplied
header must exactly match a fixed column name.

`taxonomy_formatted_update_template` returns a header-only template.
`parse_taxonomy_input_csv` parses CSV into ordered `TaxonInputRow` values.

The separator inside multi-name fields defaults to `;`. It is application
metadata and is available through:

```rust
pub fn get_taxonomy_name_separator(database: &Database) -> CoreResult<String>
pub fn set_taxonomy_name_separator(
    database: &Database,
    separator: &str,
) -> CoreResult<()>
```

`synonyms`, `zh_alias`, and `en_alias` accept multiple names. The synonym
parser separates a scientific name from its authority text at the first of:

1. a word containing `(`;
2. the second word whose first character is uppercase;
3. an independent `de`, `von`, or `van` word.

`split_scientific_name_authority` is a standalone public function so this
rule can be changed independently.

The deepest supplied rank is the target. Any supplied higher ranks narrow its
lineage; they do not need to form a continuous path from kingdom. A new
non-kingdom taxon requires only its immediate parent scientific name. A
missing genus may be derived from the first word of a species name.

Matching uses two priority levels. Input names are tried as the target
scientific name, first synonym, second synonym, and so on. For each input
name, existing `sci_name` records are tried before existing `synonym`
records. The first input-name/database-type pair that produces matches ends
matching; lower-priority pairs are not considered.

Each input scientific name carries its own authority text. The target
scientific name uses the row's `authority_year`; each synonym uses the
authority text parsed from that synonym string. When a taxon matches, the
matched existing name receives the authority text paired with the matching
input name, either as a supplement or overwrite. Every other input scientific
name, including the target scientific name when a synonym matched, is then
processed in input priority order as a `synonym`. Existing accepted names are
never switched by this process.

Formatted updates have no options. New taxa, new names, supplements, and
overwrites are enabled. The only forbidden formatted action is switching an
accepted name with an alias.

`authority_year` belongs to the deepest rank scientific name. A synonym's
authority text comes from the synonym parser. `source` applies to every name
in the row. It fills an empty existing source or initializes a new name; it
never overwrites a non-empty existing source.

Incoming `zh_name` and `zh_alias` values form one ordered Chinese name list.
If no `zh_name` exists, the first new value becomes `zh_name`; every other
new value becomes `zh_alias`. If `zh_name` already exists, all new values are
aliases. English input follows the same rule.

## Preview and apply

Preview and apply are independent interfaces and accept no update options:

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

Preview evaluates rows in order inside a transaction and rolls the transaction
back. Apply evaluates the same way and stores one operation. A row can have
several operation types at once:

- `no_change`
- `supplement`
- `new_name`
- `new_taxon`
- `overwrite`
- `invalid`
- `not_matched`
- `multiple_candidates`

Every row log contains the input row number, operation types, message, summary
view, optional parent view for a new taxon, candidate views, and structured
field changes. `taxonomy_log_csv` renders these logs as UTF-8, pipe-delimited
CSV.

Invalid or ambiguous rows fail independently. Valid rows in the same
operation can still succeed. A database or runtime failure aborts the entire
transaction.

## Search-page actions

`update_taxon` converts an exact selected taxon and its lineage into one
formatted input row and applies it without preview. It creates one operation:

```rust
pub fn update_taxon(
    database: &Database,
    input: TaxonUpdateInput,
) -> CoreResult<TaxonomyOperationResult>
```

The transient selected taxon ID is removed from stored and exported input.

Name promotion is separate, destructive, and unlogged:

```rust
pub fn promote_taxon_name(
    database: &Database,
    input: PromoteTaxonNameInput,
) -> CoreResult<()>
```

Promotion exchanges the selected alias type with the current accepted type:
`synonym` with `sci_name`, `zh_alias` with `zh_name`, or `en_alias` with
`en_name`. When promoting a species synonym, its first word must exactly equal
the parent genus `sci_name`.

Name and taxon deletion are also unlogged:

```rust
pub fn delete_taxon_name(
    database: &Database,
    input: DeleteTaxonNameInput,
) -> CoreResult<()>

pub fn delete_taxon(database: &Database, taxon_id: i64) -> CoreResult<()>
```

The unique `sci_name` cannot be deleted. A taxon with children cannot be
deleted.

## Operation history and rollback

One formatted apply is one operation. Operations store only their source,
ordered input, exact result log, changeset, and apply time. They do not store
options or rollback state.

Rollback applies the inverse changeset in one transaction. It succeeds as a
whole or fails without a partial rollback. A successful rollback deletes the
operation record.

Selected operations can be exported in ascending operation ID order. Rows
retain their original order and use the fixed formatted input columns. The
export records attempted input, not only successful changes. It contains no
selected taxon ID or other current-database identity. A later rebase is
intentionally allowed to reproduce only part of the old edits.

## Custom SQL

Custom SQL is transactional, authorization-limited, and validated before
commit. It creates no operation log and cannot be rolled back through
operation history.

## Base database replacement

An external SQLite `.db` is accepted when it contains the canonical `taxa`
and `taxon_names` layouts.

Replacement is one transaction. It clears current mapping state, taxonomy,
operation history, and previous base metadata. Before deleting the old tree it
sets every `parent_taxon_id` to `NULL`, so self-referencing `ON DELETE
RESTRICT` constraints cannot block complete replacement. It then copies
external IDs, validates the imported tree, restores the high local-ID floor,
and queues every photo for mapping.

The desktop command processes the global mapping queue asynchronously after
replacement.
