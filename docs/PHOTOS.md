# Photos Backend API

This document describes the public backend contract for the photo library.
It covers:

- `phytoindex_core::photos`: library, directory, file, metadata, thumbnail,
  refresh, and rename APIs.
- `phytoindex_core::mapping`: photo-to-taxon matching and taxon-based photo
  browsing APIs.
- Desktop Tauri commands, operation events, and media URLs that expose those
  core APIs.

Database tables and other internal storage details are not part of this
contract.

The Rust surfaces are imported from:

```rust
use phytoindex_core::mapping::*;
use phytoindex_core::photos::*;
```

Shared taxonomy types such as `TaxonRank`, `TaxonomyNameKind`,
`TaxonDisplayNames`, and `TaxonSummary` are defined in the
[taxonomy backend API](TAXONOMY.md).

## General conventions

- The backend has one open photo library root at a time.
- Paths returned in photo and directory models are relative to that root unless
  a field explicitly says it is absolute.
- Rust APIs return `CoreResult<T>`.
- Tauri commands return the documented value or a string error.
- Serialized field names and enum values use `snake_case`.
- IDs are signed 64-bit integers.

### Cursor pages

Photo list APIs return:

```rust
pub struct PhotoPage<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}
```

| Field | Description |
| --- | --- |
| `items` | Items in the current page. |
| `next_cursor` | Opaque cursor for the next page, or `null` when this is the actual last page. |

The cursor has the same external behavior as a taxonomy cursor:

- Omit it for the first page.
- Pass `next_cursor` unchanged to request the next page.
- Do not parse, edit, or persist assumptions about its contents.
- A cursor is bound to its endpoint and request scope. Reusing it with another
  directory, taxon, option set, or mapping status is invalid.
- `limit` is clamped to `1..=500`.
- Desktop commands use a default limit of `50`.

## Public photo types

### `PhotoLibrary`

| Field | Type | Description |
| --- | --- | --- |
| `root_path` | `String` | Canonical absolute path of the open photo root. |
| `root_directory_id` | `i64` | ID used to browse the root directory. |

### `PhotoDirectory`

| Field | Type | Description |
| --- | --- | --- |
| `directory_id` | `i64` | Directory ID. |
| `parent_directory_id` | `Option<i64>` | Parent directory ID, or `null` for the root. |
| `name` | `String` | Immediate directory name. The root name is empty. |
| `relative_path` | `String` | Slash-separated path relative to the open root. The root path is empty. |

### `Photo`

| Field | Type | Description |
| --- | --- | --- |
| `photo_id` | `i64` | Photo ID. |
| `directory_id` | `i64` | ID of the containing directory. |
| `relative_path` | `String` | File path relative to the open root. |
| `filename` | `String` | Current real filesystem filename. |
| `file_size` | `i64` | File size in bytes. |
| `modified_at_ns` | `i64` | Filesystem modification time in nanoseconds since the Unix epoch. |
| `thumbnail_path` | `Option<String>` | Cached thumbnail path when one is available. |

### `PhotoMetadata`

| Field | Type | Description |
| --- | --- | --- |
| `photo_id` | `i64` | Photo ID. |
| `captured_at` | `Option<String>` | Capture time read from image metadata. |
| `camera` | `Option<String>` | Camera make and model when available. |
| `width` | `Option<i64>` | Image width in pixels. |
| `height` | `Option<i64>` | Image height in pixels. |
| `longitude` | `Option<f64>` | GPS longitude when available. |
| `latitude` | `Option<f64>` | GPS latitude when available. |
| `exif_json` | `Option<String>` | Additional EXIF values encoded as JSON. |

Missing image metadata is returned as `null`; it is not an error.

### `PhotoDirectoryItem`

This is a tagged enum using the serialized field `kind`.

| `kind` | Payload | Description |
| --- | --- | --- |
| `directory` | `directory: PhotoDirectory` | Immediate child directory. |
| `photo` | `photo: Photo` | Photo directly contained in the requested directory. |

Directory browse pages form one virtual list: all child directories are
returned before photos.

### `DirectoryEntryCounts`

| Field | Type | Description |
| --- | --- | --- |
| `directory_count` | `i64` | Number of immediate child directories. |
| `file_count` | `i64` | Number of immediate photos. |

### `PhotoSyncResult`

| Field | Type | Description |
| --- | --- | --- |
| `directory_id` | `i64` | Refreshed directory ID. |
| `inserted` | `usize` | Newly indexed photos. |
| `unchanged` | `usize` | Existing photos whose indexed file facts did not change. |
| `updated` | `usize` | Existing photos whose indexed file facts changed. |
| `deleted` | `usize` | Immediate photos no longer present in the refreshed directory. |
| `directories_inserted` | `usize` | Newly indexed immediate child directories. |
| `directories_deleted` | `usize` | Immediate child directories removed; each removal also removes its indexed subtree. |

### `PhotoOperationSource`

| Value | Description |
| --- | --- |
| `manual_rename` | One call to `rename_photo`. |
| `taxon_rename` | One call to `rename_photo_from_taxon`. |
| `taxon_batch_rename` | One call to `rename_photos_from_taxa`. |

### `PhotoOperationStatus`

| Value | Description |
| --- | --- |
| `applied` | The recorded rename is currently applied. |
| `reverted` | The recorded rename has been reverted. |

### `PhotoOperationBatch`

One public rename call creates at most one batch. A no-op rename does not
create a batch or operation.

| Field | Type | Description |
| --- | --- | --- |
| `batch_id` | `i64` | Batch ID. |
| `source` | `PhotoOperationSource` | Rename API that created the batch. |
| `root_path` | `String` | Canonical photo library root at apply time. |
| `created_at` | `String` | Batch creation timestamp. |

### `PhotoOperation`

The operation stores explicit rename values. It does not expose or depend on a
SQLite changeset.

| Field | Type | Description |
| --- | --- | --- |
| `operation_id` | `i64` | Operation ID. |
| `batch_id` | `i64` | Parent batch ID. |
| `row_number` | `usize` | One-based position in the source call. |
| `status` | `PhotoOperationStatus` | `applied` or `reverted`. |
| `photo_id` | `i64` | Photo ID at apply time. |
| `directory_relative_path` | `String` | Exact containing-directory path at apply time. |
| `old_filename` | `String` | Exact filename before the rename. |
| `new_filename` | `String` | Exact filename after the rename. |
| `applied_at` | `String` | Apply timestamp. |
| `reverted_at` | `Option<String>` | Revert timestamp, or `null`. |

## `phytoindex_core::photos`

Every function below takes `database: &Database`. The parameter is omitted
from the parameter descriptions because it always identifies the current
application database.

### Library and directory APIs

#### `open_library`

```rust
pub fn open_library(
    database: &Database,
    root: &str,
) -> CoreResult<PhotoLibrary>
```

| Parameter | Description |
| --- | --- |
| `root` | Existing directory to open as the photo library root. It is canonicalized before use. |

Returns the active `PhotoLibrary`. Opening a different root replaces the
previous photo index and mappings; taxonomy data is not replaced.

#### `get_library`

```rust
pub fn get_library(
    database: &Database,
) -> CoreResult<Option<PhotoLibrary>>
```

Returns the active library, or `None` when no photo root is open.

#### `get_photo_count`

```rust
pub fn get_photo_count(database: &Database) -> CoreResult<i64>
```

Returns the number of indexed photos in the active library.

#### `get_directory_counts`

```rust
pub fn get_directory_counts(
    database: &Database,
    directory_id: i64,
) -> CoreResult<DirectoryEntryCounts>
```

| Parameter | Description |
| --- | --- |
| `directory_id` | Directory whose immediate entries are counted. |

Returns separate child-directory and photo counts. A missing directory is an
error.

#### `browse_directory`

```rust
pub fn browse_directory(
    database: &Database,
    directory_id: i64,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<PhotoDirectoryItem>>
```

| Parameter | Description |
| --- | --- |
| `directory_id` | Directory whose immediate child directories and photos are listed. |
| `cursor` | `None` for the first page, otherwise the previous page's `next_cursor`. |
| `limit` | Requested maximum number of virtual-list items. |

Returns child directories followed by photos in a single cursor page. It does
not refresh the filesystem or count all entries.

#### `refresh_directory`

```rust
pub fn refresh_directory(
    database: &Database,
    directory_id: i64,
) -> CoreResult<PhotoSyncResult>
```

| Parameter | Description |
| --- | --- |
| `directory_id` | Directory to compare with the real filesystem. |

Returns the changes found among the directory's immediate entries. Child
directories are indexed but are not recursively scanned. New or changed photos
become `processing` until taxon matching completes.

### Photo read APIs

#### `get_photo`

```rust
pub fn get_photo(
    database: &Database,
    photo_id: i64,
) -> CoreResult<Option<Photo>>
```

Returns the indexed photo, or `None` when the ID does not exist.

#### `list_photos`

```rust
pub fn list_photos(database: &Database) -> CoreResult<Vec<Photo>>
```

Returns every indexed photo ordered by `photo_id`. Interactive views should use
the cursor-based directory, taxon, or mapping-status APIs instead.

#### `photo_file_path`

```rust
pub fn photo_file_path(
    database: &Database,
    photo_id: i64,
) -> CoreResult<PathBuf>
```

Returns the validated absolute path of the real photo file. A missing database
record, missing file, or path outside the open root is an error.

#### `get_photo_metadata`

```rust
pub fn get_photo_metadata(
    database: &Database,
    photo_id: i64,
) -> CoreResult<PhotoMetadata>
```

Returns image and EXIF metadata for the photo. Optional values are `None` when
the source file does not provide them.

#### `get_or_create_thumbnail`

```rust
pub fn get_or_create_thumbnail(
    database: &Database,
    photo_id: i64,
    thumbnail_root: &Path,
) -> CoreResult<PathBuf>
```

| Parameter | Description |
| --- | --- |
| `photo_id` | Source photo ID. |
| `thumbnail_root` | Directory in which cached thumbnails are stored. |

Returns the absolute path of an existing or newly generated thumbnail.

#### `rebase_thumbnail_paths`

```rust
pub fn rebase_thumbnail_paths(
    database: &Database,
    thumbnail_root: &Path,
) -> CoreResult<usize>
```

| Parameter | Description |
| --- | --- |
| `thumbnail_root` | Current thumbnail cache directory. |

Returns the number of cached thumbnail paths updated to files found under the
provided directory.

### Rename APIs

#### `rename_photo`

```rust
pub fn rename_photo(
    database: &Database,
    photo_id: i64,
    new_filename: &str,
) -> CoreResult<Photo>
```

| Parameter | Description |
| --- | --- |
| `photo_id` | Photo to rename. |
| `new_filename` | Complete new filename, including a supported image extension. It must not contain path components. |

Renames the real file, updates the indexed `Photo`, and immediately rematches
the new filename. Returns the updated `Photo`. A conflicting destination or an
invalid filename is an error.

#### `rename_photo_from_taxon`

```rust
pub fn rename_photo_from_taxon(
    database: &Database,
    photo_id: i64,
) -> CoreResult<Photo>
```

Renames one real file to:

```text
{accepted scientific name}.{original extension}
```

The photo must have a current `matched` mapping and the selected taxon must
have an accepted scientific name. Returns the updated `Photo`.

#### `rename_photos_from_taxa`

```rust
pub fn rename_photos_from_taxa(
    database: &Database,
    photo_ids: &[i64],
) -> CoreResult<Vec<Photo>>
```

| Parameter | Description |
| --- | --- |
| `photo_ids` | Photos to rename, processed in input order. |

Returns each updated `Photo` in input order. Processing stops at the first
error; earlier successful renames remain applied.

### Rename history APIs

All changing rename calls are logged. Single-photo calls contain one operation.
Successful rows from `rename_photos_from_taxa` share one batch.

#### `list_photo_operation_batches`

```rust
pub fn list_photo_operation_batches(
    database: &Database,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<PhotoOperationBatch>>
```

| Parameter | Description |
| --- | --- |
| `cursor` | `None` for the first page, otherwise the previous page's `next_cursor`. |
| `limit` | Requested maximum number of batches. |

Returns batches newest first by creation time and batch ID.

#### `list_photo_operations`

```rust
pub fn list_photo_operations(
    database: &Database,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<PhotoOperation>>
```

| Parameter | Description |
| --- | --- |
| `cursor` | `None` for the first page, otherwise the previous page's `next_cursor`. |
| `limit` | Requested maximum number of operations. |

Returns operations newest first by operation ID.

#### `list_photo_operations_for_batch`

```rust
pub fn list_photo_operations_for_batch(
    database: &Database,
    batch_id: i64,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<PhotoOperation>>
```

| Parameter | Description |
| --- | --- |
| `batch_id` | Batch whose operations are requested. |
| `cursor` | `None` for the first page, otherwise the previous page's `next_cursor`. |
| `limit` | Requested maximum number of operations. |

Returns operations in source-row and operation-ID order. A cursor from one
batch cannot be used with another batch.

#### `revert_photo_operation`

```rust
pub fn revert_photo_operation(
    database: &Database,
    operation_id: i64,
) -> CoreResult<()>
```

Reverts one applied rename and marks it `reverted`. Revert succeeds only when:

- the currently open root equals the recorded root;
- the photo still has the recorded ID and directory;
- its current indexed and real filename equals `new_filename`;
- `old_filename` is available as the destination.

On success the real file is renamed to `old_filename`, the photo mapping is
re-evaluated, and the operation status is updated together. If a later rename
changed the filename again, that later operation must be reverted first.

## Public mapping types

### `PhotoTaxonStatus`

| Value | Description |
| --- | --- |
| `processing` | The photo is waiting for background knowledge-base matching. |
| `unmatched` | Matching found no candidate taxon. |
| `ambiguous` | Matching found one or more candidates and no taxon has been selected. |
| `matched` | A candidate taxon has been selected. |
| `stale` | The previously selected taxon is no longer valid. |

There is no `resolved_by` field. A selected taxon remains selected after
rematching only while it is still a current candidate.

### `PhotoTaxonMapping`

| Field | Type | Description |
| --- | --- | --- |
| `photo_id` | `i64` | Photo ID. |
| `taxon_id` | `Option<i64>` | Selected taxon ID when available. |
| `status` | `PhotoTaxonStatus` | Current logical mapping status. |

A `processing` response may retain the previously selected `taxon_id` while
that selection is being revalidated.

### `PhotoMatchedName`

| Field | Type | Description |
| --- | --- | --- |
| `name_id` | `i64` | Matched taxonomy name ID. |
| `name_kind` | `TaxonomyNameKind` | Scientific, English, or Chinese name kind. |
| `name` | `String` | Taxonomy name text that matched. |
| `is_accepted` | `bool` | Whether the matched name is currently accepted. |

### `PhotoTaxonCandidate`

| Field | Type | Description |
| --- | --- | --- |
| `summary` | `TaxonSummary` | Candidate taxon, accepted display names, and breadcrumb. |
| `matched_names` | `Vec<PhotoMatchedName>` | Taxonomy names responsible for this candidate. |
| `accepted_names` | `TaxonDisplayNames` | Current accepted scientific, English, and Chinese names. |

### `PhotoTaxonMatch`

| Field | Type | Description |
| --- | --- | --- |
| `mapping` | `PhotoTaxonMapping` | Current stored or synthesized mapping state. |
| `candidates` | `Vec<PhotoTaxonCandidate>` | Current candidates for the photo filename. |

### `PhotoTaxonUsage`

| Field | Type | Description |
| --- | --- | --- |
| `taxon_id` | `i64` | Taxon ID. |
| `rank` | `TaxonRank` | Taxonomic rank. |
| `names` | `TaxonDisplayNames` | Current accepted display names. |
| `direct_photo_count` | `i64` | Photos mapped directly to this taxon. |
| `subtree_photo_count` | `i64` | Photos mapped to this taxon or any descendant. |

### `PhotoTaxonNode`

| Field | Type | Description |
| --- | --- | --- |
| `taxon` | `Option<PhotoTaxonUsage>` | Selected taxon, or `null` for the virtual root. |
| `subtree_photo_count` | `i64` | Matched photos under this node. |

Children and photos are not embedded. Request them with
`browse_photo_taxon`.

### `PhotoTaxonItem`

This is a tagged enum using the serialized field `kind`.

| `kind` | Payload | Description |
| --- | --- | --- |
| `taxon` | `taxon: PhotoTaxonUsage` | Immediate child taxon. |
| `photo` | `photo: Photo` | Matched photo in the requested photo section. |

Taxon browse pages form one virtual list: all immediate child taxa are returned
before photos.

### `PhotoMappingListStatus`

Accepted values are:

- `matched`
- `unmatched`
- `ambiguous`
- `processing`
- `stale`
- `unmapped`

`unmapped` means the photo has neither a current mapping nor pending matching
work. A photo waiting for matching belongs to `processing`, regardless of its
previous stored status.

### `PhotoMappingListItem`

| Field | Type | Description |
| --- | --- | --- |
| `photo` | `Photo` | Photo in the requested logical status. |
| `mapping` | `Option<PhotoTaxonMapping>` | Mapping state, or `null` for `unmapped`. |

### `MappingMetadata`

| Field | Type | Description |
| --- | --- | --- |
| `mapped_photo_count` | `i64` | Number of stored `matched` mappings. |
| `unmatched_photo_count` | `i64` | Number of stored `unmatched` mappings. |
| `ambiguous_photo_count` | `i64` | Number of stored `ambiguous` mappings. |
| `processing_photo_count` | `i64` | Number of photos waiting for matching. |
| `mapping_taxa_count` | `i64` | Number of non-empty taxon nodes in the photo taxonomy view. |

Stored-status counts can overlap `processing_photo_count` while an existing
mapping is being revalidated. Use `list_photos_by_mapping_status` when mutually
exclusive logical status membership is required.

### `PhotoMappingRunResult`

| Field | Type | Description |
| --- | --- | --- |
| `processed` | `usize` | Photos evaluated by this run. |
| `changed` | `usize` | Mapping states changed by this run. |
| `pending` | `i64` | Photos still waiting for matching. |

### `MappingSyncResult`

| Field | Type | Description |
| --- | --- | --- |
| `processed` | `usize` | Photos evaluated by the rebuild. |
| `mapped` | `usize` | Photos ending in `matched`. |
| `unmapped` | `usize` | Photos ending in `unmatched`. |
| `ambiguous` | `usize` | Photos ending in `ambiguous`. |
| `unmapped_photos` | `Vec<Photo>` | Photos ending in `unmatched`. |
| `orphan_mappings_deleted` | `usize` | Obsolete mapping rows removed by the operation. |

## `phytoindex_core::mapping`

Every function below takes `database: &Database`.

### Mapping state and matching

#### `get_metadata`

```rust
pub fn get_metadata(database: &Database) -> CoreResult<MappingMetadata>
```

Returns aggregate mapping counts.

#### `get_photo_mapping`

```rust
pub fn get_photo_mapping(
    database: &Database,
    photo_id: i64,
) -> CoreResult<Option<PhotoTaxonMapping>>
```

Returns the photo's current logical mapping, including synthesized
`processing`, or `None` when the photo has no mapping state or does not exist.

#### `get_photo_taxon_match`

```rust
pub fn get_photo_taxon_match(
    database: &Database,
    photo_id: i64,
) -> CoreResult<PhotoTaxonMatch>
```

| Parameter | Description |
| --- | --- |
| `photo_id` | Photo whose current mapping and candidates are requested. |

Returns the current mapping and freshly evaluated candidates. A missing photo
is an error.

The current filename extractor removes only the final extension. It does not
replace punctuation or other valid name characters. Candidate lookup reuses
taxonomy search order: exact, full-name prefix, word prefix, middle, then
trigram candidates with edit distance.

#### `select_photo_taxon`

```rust
pub fn select_photo_taxon(
    database: &Database,
    photo_id: i64,
    taxon_id: i64,
) -> CoreResult<PhotoTaxonMapping>
```

| Parameter | Description |
| --- | --- |
| `photo_id` | Photo whose mapping is being resolved. |
| `taxon_id` | Taxon selected from the photo's current candidates. |

Returns a `matched` mapping. Selecting a taxon that is not a current candidate
is an error.

#### `process_pending_photo_matches`

```rust
pub fn process_pending_photo_matches(
    database: &Database,
    progress: &mut MappingProgressCallback<'_>,
) -> CoreResult<PhotoMappingRunResult>
```

| Parameter | Description |
| --- | --- |
| `progress` | Callback receiving `(processed, total, message)` updates. |

Processes photos waiting for filename matching and returns the run summary.

#### `rebuild_mapping`

```rust
pub fn rebuild_mapping(
    database: &Database,
) -> CoreResult<MappingSyncResult>
```

Re-evaluates every indexed photo and replaces existing mapping results. Returns
the complete rebuild summary.

### Taxon-based browsing

#### `get_photo_taxon_node`

```rust
pub fn get_photo_taxon_node(
    database: &Database,
    taxon_id: Option<i64>,
    show_empty: bool,
) -> CoreResult<PhotoTaxonNode>
```

| Parameter | Description |
| --- | --- |
| `taxon_id` | Taxon to inspect, or `None` for the virtual root. |
| `show_empty` | Whether a taxon with no matched photos is considered visible. |

Returns the requested node and its subtree photo count. With
`show_empty = false`, requesting an empty taxon returns not found.

#### `browse_photo_taxon`

```rust
pub fn browse_photo_taxon(
    database: &Database,
    taxon_id: Option<i64>,
    show_empty: bool,
    include_descendants: bool,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<PhotoTaxonItem>>
```

| Parameter | Description |
| --- | --- |
| `taxon_id` | Parent taxon to browse, or `None` for the virtual root. |
| `show_empty` | Include immediate child taxa whose subtree has no matched photos. |
| `include_descendants` | Include photos mapped to descendant taxa as well as directly to the selected taxon. |
| `cursor` | `None` for the first page, otherwise the previous page's `next_cursor`. |
| `limit` | Requested maximum number of virtual-list items. |

Returns immediate child taxa followed by matched photos in one cursor page.
`include_descendants` affects the photo section only; taxon items are always
immediate children.

#### `list_photos_by_mapping_status`

```rust
pub fn list_photos_by_mapping_status(
    database: &Database,
    status: PhotoMappingListStatus,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<PhotoMappingListItem>>
```

| Parameter | Description |
| --- | --- |
| `status` | Logical status to list. |
| `cursor` | `None` for the first page, otherwise the previous page's `next_cursor`. |
| `limit` | Requested maximum number of photos. |

Returns photos in the requested logical status ordered by `photo_id`.

### Taxon suggestions

#### `suggest`

```rust
pub fn suggest(
    database: &Database,
    query: &str,
    mode: &str,
    limit: usize,
) -> CoreResult<Vec<Taxon>>
```

| Parameter | Description |
| --- | --- |
| `query` | Text passed to taxonomy search. |
| `mode` | Use `binomial` to retain only results matched by a scientific name; other values keep all name kinds. |
| `limit` | Maximum requested taxonomy search results. |

Returns compact `Taxon` suggestions. Each item contains `taxon_id`, `rank`,
preferred display `name`, optional `parent_id`, and optional scientific
`binomial_name`.

## Desktop interface

The desktop adapter supplies the current database and converts core errors to
strings. Parameter names below are the camel-case keys used in JavaScript
`invoke` payloads. Returned object fields remain `snake_case`.

### Tauri commands

| Command | Parameters | Return |
| --- | --- | --- |
| `get_photo_library` | none | `PhotoLibrary \| null` |
| `get_photo_library_count` | none | `i64` |
| `open_photo_library` | `root: string` | `PhotoLibrary` |
| `browse_photo_directory` | `directoryId: number`, optional `cursor: string`, optional `limit: number` | `PhotoPage<PhotoDirectoryItem>` |
| `get_photo_directory_counts` | `directoryId: number` | `DirectoryEntryCounts` |
| `refresh_photo_directory` | `directoryId: number` | `{ operation: OperationState }` |
| `start_photo_mapping` | none | `{ operation: OperationState }` |
| `rename_photo` | `photoId: number`, `newFilename: string` | `Photo` |
| `rename_photo_from_taxon` | `photoId: number` | `Photo` |
| `rename_photos_from_taxa` | `photoIds: number[]` | `Photo[]` |
| `list_photo_operation_batches` | optional `cursor: string`, optional `limit: number` | `PhotoPage<PhotoOperationBatch>` |
| `list_photo_operations` | optional `cursor: string`, optional `limit: number` | `PhotoPage<PhotoOperation>` |
| `list_photo_operations_for_batch` | `batchId: number`, optional `cursor: string`, optional `limit: number` | `PhotoPage<PhotoOperation>` |
| `revert_photo_operation` | `operationId: number` | `null` |
| `get_all_photos` | none | `Photo[]` |
| `get_photo` | `photoId: number` | `Photo` |
| `get_photo_availability` | `photoId: number` | `{ available: boolean, error: string \| null }` |
| `get_photo_metadata` | `photoId: number` | `PhotoMetadata` |
| `get_mapping_metadata` | none | `MappingMetadata` |
| `get_photo_taxon_match` | `photoId: number` | `PhotoTaxonMatch` |
| `select_photo_taxon` | `photoId: number`, `taxonId: number` | `PhotoTaxonMapping` |
| `get_photo_taxon_node` | optional `taxonId: number`, optional `showEmpty: boolean` | `PhotoTaxonNode` |
| `browse_photo_taxon` | optional `taxonId: number`, optional `showEmpty: boolean`, optional `includeDescendants: boolean`, optional `cursor: string`, optional `limit: number` | `PhotoPage<PhotoTaxonItem>` |
| `list_photos_by_mapping_status` | `status: PhotoMappingListStatus`, optional `cursor: string`, optional `limit: number` | `PhotoPage<PhotoMappingListItem>` |
| `suggest_mapping_taxa` | `query: string`, `mode: string` | `Taxon[]` |
| `get_operations_status` | none | `Record<string, OperationState>` |

Desktop defaults:

- `browse_photo_directory.limit = 50`
- Photo operation list limits default to `50`
- `browse_photo_taxon.limit = 50`
- `browse_photo_taxon.show_empty = false`
- `browse_photo_taxon.include_descendants = true`
- `list_photos_by_mapping_status.limit = 50`
- `suggest_mapping_taxa` requests at most 10 taxonomy results

`refresh_photo_directory` and `start_photo_mapping` schedule background work
and return immediately. Their `OperationState` has:

| Field | Type | Description |
| --- | --- | --- |
| `module` | `string` | Operation owner, such as `photos` or `mapping`. |
| `task_id` | `string \| null` | Unique task ID. |
| `operation` | `string \| null` | Operation name. |
| `running` | `boolean` | Whether work is still running. |
| `started_at` | `string \| null` | Start time. |
| `finished_at` | `string \| null` | Completion time. |
| `message` | `string` | Current phase or result message. |
| `processed` | `number` | Completed work units. |
| `total` | `number \| null` | Total work units when known. |
| `result` | `unknown \| null` | Successful operation result. |
| `error` | `string \| null` | Failure message. |

On success:

- `refresh_photo_directory` stores
  `{ refresh: PhotoSyncResult, mapping: PhotoMappingRunResult }` in `result`.
- `start_photo_mapping` stores `PhotoMappingRunResult` in `result`.

Progress is emitted through the Tauri event:

```text
operation-progress
```

The event payload is the latest `OperationState`.

### Media URLs

The desktop custom protocol exposes photo bytes without returning filesystem
paths to the frontend. Pass the resource path and the `phytoindex` protocol to
Tauri's `convertFileSrc`; the final URL syntax is platform-dependent.

| Resource path | Return |
| --- | --- |
| `photo/{photo_id}` | Original photo bytes. |
| `thumbnail/{photo_id}` | Existing or newly generated thumbnail bytes. |

A successful response includes the detected content type. An invalid ID,
missing file, or unknown resource returns an HTTP error response.
