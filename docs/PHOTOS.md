# Photos Backend API

This document describes the public backend contract for the photo library.
It covers:

- `vividarium_core::photos`: library, directory, file, metadata, thumbnail,
  refresh, and rename APIs.
- `vividarium_core::mapping`: photo-to-taxon matching and taxon-based photo
  browsing APIs.
- `vividarium_core::naming`: shared name normalization and filename hook APIs,
  documented in [the naming backend API](NAMING.md).
- Desktop Tauri commands, operation events, and media URLs that expose those
  core APIs.

The Rust surfaces are imported from:

```rust
use vividarium_core::mapping::*;
use vividarium_core::photos::*;
```

Shared taxonomy types such as `TaxonRank`, `TaxonomyNameType`,
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

### `PhotoRenameRowStatus`

| Value | Description |
| --- | --- |
| `applied` | The real file and database record were renamed and an operation was logged. |
| `no_change` | The requested filename already matched the current filename. |
| `failed` | This input row failed; its `message` describes the error. |

### `PhotoRenameRowOutcome`

| Field | Type | Description |
| --- | --- | --- |
| `row_number` | `usize` | One-based position in the input `photo_ids`. |
| `photo_id` | `i64` | Requested photo ID. |
| `operation_id` | `Option<i64>` | Logged operation ID for an `applied` row; otherwise `null`. |
| `status` | `PhotoRenameRowStatus` | Per-row result. |
| `message` | `String` | Short result or error description. |
| `photo` | `Option<Photo>` | Current photo for `applied` and `no_change`; `null` for `failed`. |

### `PhotoRenameOperationResult`

| Field | Type | Description |
| --- | --- | --- |
| `operation_id` | `Option<i64>` | Shared operation ID when at least one row was applied; otherwise `null`. |
| `rows` | `Vec<PhotoRenameRowOutcome>` | One outcome for every input ID, in input order. |

### `PhotoOperationSource`

| Value | Description |
| --- | --- |
| `manual_rename` | One call to `rename_photo`. |
| `taxon_rename` | One call to `rename_photo_from_taxon`. |
| `taxon_selection_rename` | One call to `rename_photos_from_taxa` or the directory selection API. |

### `PhotoOperation`

One public rename call creates at most one operation. A no-op call creates no
operation. The operation stores its ordered request input and all successful
file rename items.

| Field | Type | Description |
| --- | --- | --- |
| `operation_id` | `i64` | Operation ID. |
| `source` | `PhotoOperationSource` | Rename API that created the operation. |
| `root_path` | `String` | Canonical photo library root at apply time. |
| `input` | `Vec<PhotoOperationInput>` | Ordered requested photo IDs and optional manual filenames. |
| `items` | `Vec<PhotoOperationItem>` | Successful renames in original row order. |
| `applied_at` | `String` | Apply timestamp. |

`PhotoOperationItem` contains:

| Field | Type | Description |
| --- | --- | --- |
| `row_number` | `usize` | One-based position in the source call. |
| `photo_id` | `i64` | Photo ID at apply time. |
| `directory_relative_path` | `String` | Exact containing-directory path at apply time. |
| `old_filename` | `String` | Exact filename before the rename. |
| `new_filename` | `String` | Exact filename after the rename. |

## `vividarium_core::photos`

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
directories are indexed but are not recursively scanned. Every newly discovered
photo becomes `processing` during its first refresh. New or changed photos
remain in that state until taxon matching completes.

### Photo read APIs

#### `get_photo`

```rust
pub fn get_photo(
    database: &Database,
    photo_id: i64,
) -> CoreResult<Option<Photo>>
```

Returns the indexed photo, or `None` when the ID does not exist.

#### `search_photos`

```rust
pub fn search_photos(
    database: &Database,
    query: &str,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<Photo>>
```

| Parameter | Description |
| --- | --- |
| `query` | Text matched against filenames and taxonomy names. |
| `cursor` | `None` for the first page, otherwise the previous `next_cursor`. |
| `limit` | Requested maximum number of photos. |

Combines filename matches with photos assigned to matching taxa. Taxon matches
include photos on descendant taxa. Duplicate photos are removed and results
are ordered by `photo_id`. Blank input returns an empty page. The cursor is
bound to the normalized query.

This is the backend source for a general-search PhotoSet. PhotoSet itself is a
frontend window concept and is not stored by the backend.

#### `search_photos_by_filename`

```rust
pub fn search_photos_by_filename(
    database: &Database,
    query: &str,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<Photo>>
```

| Parameter | Description |
| --- | --- |
| `query` | Case-insensitive text contained in `filename`. Leading and trailing whitespace is ignored. |
| `cursor` | `None` for the first page, otherwise the previous page's `next_cursor`. |
| `limit` | Requested maximum number of photos. |

Returns case-insensitive substring matches ordered by `photo_id`. An empty
query returns an empty page. The cursor is bound to the normalized query.

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

Formats and renames one real file using `PhotoFilenameFormatSettings`. The
photo must have a current `matched` mapping. The configured filename parser
supplies the preserved suffix, so serial numbers and the image extension are
not discarded.

`PhotoFilenameFormatSettings` exposes the six booleans `family_zh`,
`family_sci`, `genus_zh`, `genus_sci`, `species_zh`, and `species_sci`.
The default enables only `species_sci`. When the requested rank is absent, the
formatter moves that selection toward genus and then family. When one language
is absent at the selected rank, it uses the other language.

```rust
pub fn get_photo_filename_format_settings(
    database: &Database,
) -> CoreResult<PhotoFilenameFormatSettings>

pub fn set_photo_filename_format_settings(
    database: &Database,
    settings: &PhotoFilenameFormatSettings,
) -> CoreResult<()>

pub fn format_photo_filename(
    info: &TaxonomicNameInfo,
    suffix: &str,
    settings: &PhotoFilenameFormatSettings,
) -> CoreResult<String>
```

| Function | Parameters | Return |
| --- | --- | --- |
| `get_photo_filename_format_settings` | `database`: project database | Current six-field filename format settings. |
| `set_photo_filename_format_settings` | `database`; `settings`: complete six-field settings | `()` after validation and save. |
| `format_photo_filename` | `info`: parsed six-dimensional names; `suffix`: preserved filename suffix; `settings`: fields to include | Formatted filename. |

#### `rename_photos_from_taxa`

```rust
pub fn rename_photos_from_taxa(
    database: &Database,
    photo_ids: &[i64],
) -> CoreResult<PhotoRenameOperationResult>
```

| Parameter | Description |
| --- | --- |
| `photo_ids` | Photos to rename, processed in input order. |

Returns one outcome for every input ID. A row failure does not stop later rows;
successful earlier and later renames remain applied. All changing rows share
the returned `operation_id`. If every row fails or needs no change,
`operation_id` is `None`.

#### `rename_photos_in_directory_from_taxa`

```rust
pub fn rename_photos_in_directory_from_taxa(
    database: &Database,
    directory_id: i64,
    include_descendants: bool,
) -> CoreResult<PhotoRenameOperationResult>
```

| Parameter | Description |
| --- | --- |
| `directory_id` | Directory whose photos are considered. |
| `include_descendants` | Whether to include photos in descendant directories. |

Only photos with a current `matched` mapping are renamed. The return value has
the same per-row and operation semantics as `rename_photos_from_taxa`.

### Rename history APIs

All changing rows from one public rename call share one operation.

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

#### `get_photo_operation`

```rust
pub fn get_photo_operation(
    database: &Database,
    operation_id: i64,
) -> CoreResult<Option<PhotoOperation>>
```

Returns one operation with all successful file items in input order.

#### `revert_photo_operation`

```rust
pub fn revert_photo_operation(
    database: &Database,
    operation_id: i64,
) -> CoreResult<()>
```

Reverts one operation and deletes its history record after success. Revert
preflights every recorded file and then restores all items in reverse row
order. It succeeds
only when:

- the currently open root equals the recorded root;
- every photo still has the recorded ID and directory;
- every current indexed and real filename equals its `new_filename`;
- every filesystem rename can be completed.

Database changes and status update use one transaction. Filesystem work uses
compensating renames if a later file or database step fails, so the operation
restores as a whole or reports a consistency error.

#### `export_photo_operation_csv`

```rust
pub fn export_photo_operation_csv(
    database: &Database,
    operation_id: i64,
) -> CoreResult<String>
```

`operation_id` identifies one existing rename operation. The return value is a
UTF-8, pipe-delimited audit CSV with one header and the operation's successful
items in original row order.

#### `export_all_photo_operations_csv`

```rust
pub fn export_all_photo_operations_csv(
    database: &Database,
) -> CoreResult<String>
```

Returns the same audit CSV for every rename operation. Operations are ordered
from oldest to newest, their items retain row order, and the combined file has
one header.

Both audit exports use fields already present in `PhotoOperation` and
`PhotoOperationItem`:

```text
operation_id|source|applied_at|root_path|row_number|photo_id|directory_relative_path|old_filename|new_filename
```

The exports are audit records only. No CSV-driven rename import is provided.

## Public mapping types

### `PhotoTaxonStatus`

| Value | Description |
| --- | --- |
| `processing` | The photo is waiting for background knowledge-base matching. |
| `unmatched` | Matching found no candidate taxon. |
| `ambiguous` | The highest-priority matching dimension found more than one taxon. |
| `matched` | Matching found one taxon, or the user selected or forced a taxon. |

There is no `resolved_by` field. A selected taxon remains selected after
rematching only while it is still a current candidate.

`matched`, `ambiguous`, and `unmatched` are stable results. `processing` is a
temporary state and takes precedence over any previous result. A processing
photo is excluded from matched-only navigation, taxonomy browsing, search,
counts, and rename operations until matching completes.

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
| `name_type` | `TaxonomyNameType` | One of the six taxonomy name types. |
| `name` | `String` | Taxonomy name text that matched. |

### `PhotoTaxonCandidate`

| Field | Type | Description |
| --- | --- | --- |
| `summary` | `TaxonSummary` | Candidate taxon, accepted display names, and breadcrumb. |
| `matched_names` | `Vec<PhotoMatchedName>` | Taxonomy names responsible for this candidate. |
| `accepted_names` | `TaxonDisplayNames` | Current `sci_name`, `en_name`, and `zh_name`. |

Candidates and matched-name snapshots are available only for `ambiguous`
mappings. A later automatic or manual result replaces them.

### `PhotoTaxonMatch`

| Field | Type | Description |
| --- | --- | --- |
| `mapping` | `PhotoTaxonMapping` | Current logical mapping state. |
| `candidates` | `Vec<PhotoTaxonCandidate>` | Persisted candidates when the current status is `ambiguous`; otherwise empty. |

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

A photo waiting for matching belongs to `processing`, regardless of its
previous stable result.

### `PhotoMappingListItem`

| Field | Type | Description |
| --- | --- | --- |
| `photo` | `Photo` | Photo in the requested logical status. |
| `mapping` | `PhotoTaxonMapping` | Current logical mapping state. |

### `MappingMetadata`

| Field | Type | Description |
| --- | --- | --- |
| `mapped_photo_count` | `i64` | Number of current `matched` mappings. |
| `unmatched_photo_count` | `i64` | Number of current `unmatched` mappings. |
| `ambiguous_photo_count` | `i64` | Number of current `ambiguous` mappings. |
| `processing_photo_count` | `i64` | Number of photos waiting for matching. |
| `mapping_taxa_count` | `i64` | Number of non-empty taxon nodes in the photo taxonomy view. |

The four photo-status counts are mutually exclusive.

### `PhotoMappingRunResult`

| Field | Type | Description |
| --- | --- | --- |
| `processed` | `usize` | Photos evaluated by this run. |
| `changed` | `usize` | Mapping states changed by this run. |
| `pending` | `i64` | Photos still waiting for matching. |

### `PhotoNameField` and `PhotoNameMatchSettings`

`PhotoNameField` serializes as `species_sci`, `species_zh`, `genus_sci`,
`genus_zh`, `family_sci`, or `family_zh`.

`PhotoNameMatchSettings` contains `priority: Vec<PhotoNameField>`. The list
must contain all six fields exactly once and is evaluated from first to last.

## `vividarium_core::mapping`

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

| Parameter | Description |
| --- | --- |
| `photo_id` | Photo whose current mapping is requested. |

Returns the photo's current logical mapping, including `processing`, or
`None` when the photo does not exist. A photo that exists but has no mapping
state is reported as a consistency error.

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

Returns the current mapping and its persisted candidates. Candidates are
returned only for a current `ambiguous` mapping. A missing photo is an error.

The configured filename parser returns six possible names: scientific and
Chinese names at species, genus, and family ranks. Each scientific field
queries `sci_name` and `synonym` together; each Chinese field queries `zh_name`
and `zh_alias` together. Results within one field are merged and deduplicated
by taxon ID. The first configured field producing candidates wins. One
candidate is mapped automatically; several candidates are persisted with an
`ambiguous` mapping.

The default priority is `species_sci`, `species_zh`, `genus_sci`, `genus_zh`,
`family_sci`, `family_zh`.

#### `get_photo_name_match_settings`

```rust
pub fn get_photo_name_match_settings(
    database: &Database,
) -> CoreResult<PhotoNameMatchSettings>
```

Returns the six matching fields in their current priority order.

#### `set_photo_name_match_settings`

```rust
pub fn set_photo_name_match_settings(
    database: &Database,
    settings: &PhotoNameMatchSettings,
) -> CoreResult<()>
```

| Parameter | Description |
| --- | --- |
| `settings` | Priority list containing every `PhotoNameField` exactly once. |

Returns `()` after saving the settings. Changing the priority marks every
photo as `processing` for automatic remapping.

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

#### `clear_photo_mapping`

```rust
pub fn clear_photo_mapping(
    database: &Database,
    photo_id: i64,
) -> CoreResult<PhotoTaxonMapping>
```

| Parameter | Description |
| --- | --- |
| `photo_id` | Photo whose binding is removed. |

Removes the current taxon binding and records the photo as `unmatched`.
Candidates and pending automatic remapping are cancelled. Returns the new
mapping. A missing photo is an error.

#### `set_photo_mapping`

```rust
pub fn set_photo_mapping(
    database: &Database,
    photo_id: i64,
    taxon_id: i64,
) -> CoreResult<PhotoTaxonMapping>
```

| Parameter | Description |
| --- | --- |
| `photo_id` | Photo whose binding is assigned or replaced. |
| `taxon_id` | Existing taxon to bind. |

Forces the photo to `matched` with any existing taxon. The taxon does not need
to be an automatic candidate. Returns the new mapping and cancels candidates
and pending automatic remapping.

#### `remap_photo`

```rust
pub fn remap_photo(
    database: &Database,
    photo_id: i64,
) -> CoreResult<PhotoTaxonMatch>
```

| Parameter | Description |
| --- | --- |
| `photo_id` | Photo to match immediately. |

Runs the configured filename parser and six-field matching engine for one
photo and returns the resulting mapping. Candidates are included when the
result is `ambiguous`.

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

### Taxon-based browsing

#### `search_photo_taxa`

```rust
pub fn search_photo_taxa(
    database: &Database,
    query: &str,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<TaxonSearchResult>>
```

| Parameter | Description |
| --- | --- |
| `query` | Taxonomy name query. |
| `cursor` | `None` for the first page, otherwise the previous page's `next_cursor`. |
| `limit` | Requested maximum number of taxa. |

Uses the taxonomy search matching stages, priorities, summaries, details, and
matched-name output unchanged. The additional filter requires
`subtree_photo_count > 0`, so a result has a matched photo on itself or a
descendant. An empty query returns an empty page. The opaque cursor is bound
to the normalized query and continues the same ranked result order.

#### `suggest_photo_taxa`

```rust
pub fn suggest_photo_taxa(
    database: &Database,
    query: &str,
    limit: usize,
) -> CoreResult<Vec<TaxonSuggestion>>
```

Uses the same matching stages, priorities, ordering, and photo filter as
`search_photo_taxa`, while only loading the minimal autocomplete fields:
`taxon_id`, `rank`, accepted display names, and matched names.

#### `list_taxon_photos`

```rust
pub fn list_taxon_photos(
    database: &Database,
    taxon_id: i64,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<Photo>>
```

| Parameter | Description |
| --- | --- |
| `taxon_id` | Root of the selected taxonomy subtree. |
| `cursor` | `None` for the first page, otherwise the previous page's `next_cursor`. |
| `limit` | Requested maximum number of photos. |

Returns matched photos assigned to the requested taxon or any descendant,
ordered by `photo_id`. A missing taxon is an error. The cursor is bound to
`taxon_id`. This directly supplies the PhotoSet opened from a knowledge-base
taxon without per-photo lookup calls.

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

#### `search_photos_by_mapping_status`

```rust
pub fn search_photos_by_mapping_status(
    database: &Database,
    status: PhotoMappingListStatus,
    query: &str,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<PhotoMappingListItem>>
```

| Parameter | Description |
| --- | --- |
| `status` | Logical status to search. |
| `query` | Filename or taxonomy query. |
| `cursor` | `None` for the first page, otherwise the previous page's `next_cursor`. |
| `limit` | Requested maximum number of photos. |

For `matched`, the query uses the same combined filename and taxonomy behavior
as `search_photos`, then restricts results to current matched photos. Other
statuses search filenames only. Results are ordered by `photo_id`; the cursor
is bound to both the normalized query and status.

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
| `parse_photo_filename` | `filename: string` | `ParsedPhotoFilename` |
| `normalize_taxonomy_name` | `value: string` | `string \| null` |
| `get_naming_hook_settings` | none | `NamingHookSettings` |
| `get_naming_hook_templates` | none | `NamingHookTemplates` |
| `set_naming_hook` | `kind: NamingHookKind`, optional `script: string` | `null` |
| `test_naming_hook` | `kind: NamingHookKind`, `script: string`, `input: string` | `NamingHookTestResult` |
| `get_naming_hook_test_cases` | none | `NamingHookTestCases` |
| `set_naming_hook_test_cases` | `kind: NamingHookKind`, `cases: NamingHookTestCase[]` | `null` |
| `run_naming_hook_tests` | `kind: NamingHookKind`, optional `script: string` | `NamingHookTestReport` |
| `get_photo_name_match_settings` | none | `PhotoNameMatchSettings` |
| `set_photo_name_match_settings` | `settings: PhotoNameMatchSettings` | `null` |
| `get_photo_filename_format_settings` | none | `PhotoFilenameFormatSettings` |
| `set_photo_filename_format_settings` | `settings: PhotoFilenameFormatSettings` | `null` |
| `format_photo_filename` | `info: TaxonomicNameInfo`, `suffix: string`, `settings: PhotoFilenameFormatSettings` | `string` |
| `rename_photo` | `photoId: number`, `newFilename: string` | `Photo` |
| `rename_photo_from_taxon` | `photoId: number` | `Photo` |
| `rename_photos_from_taxa` | `photoIds: number[]` | `PhotoRenameOperationResult` |
| `rename_photos_in_directory_from_taxa` | `directoryId: number`, optional `includeDescendants: boolean` | `PhotoRenameOperationResult` |
| `list_photo_operations` | optional `cursor: string`, optional `limit: number` | `PhotoPage<PhotoOperation>` |
| `get_photo_operation` | `operationId: number` | `PhotoOperation` |
| `revert_photo_operation` | `operationId: number` | `null` |
| `export_photo_operation_csv` | `operationId: number` | UTF-8 audit CSV `string` |
| `export_all_photo_operations_csv` | none | UTF-8 combined audit CSV `string` |
| `get_photo` | `photoId: number` | `Photo` |
| `search_photos` | `query: string`, optional `cursor: string`, optional `limit: number` | `PhotoPage<Photo>` |
| `search_photos_by_filename` | `query: string`, optional `cursor: string`, optional `limit: number` | `PhotoPage<Photo>` |
| `get_photo_availability` | `photoId: number` | `{ available: boolean, error: string \| null }` |
| `reveal_photo_in_file_manager` | `photoId: number` | `null` |
| `get_photo_metadata` | `photoId: number` | `PhotoMetadata` |
| `get_mapping_metadata` | none | `MappingMetadata` |
| `search_photo_taxa` | `query: string`, optional `cursor: string`, optional `limit: number` | `PhotoPage<TaxonSearchResult>` |
| `suggest_photo_taxa` | `query: string`, optional `limit: number` | `TaxonSuggestion[]` |
| `get_photo_mapping` | `photoId: number` | `PhotoTaxonMapping \| null` |
| `clear_photo_mapping` | `photoId: number` | `PhotoTaxonMapping` |
| `set_photo_mapping` | `photoId: number`, `taxonId: number` | `PhotoTaxonMapping` |
| `remap_photo` | `photoId: number` | `PhotoTaxonMatch` |
| `list_taxon_photos` | `taxonId: number`, optional `cursor: string`, optional `limit: number` | `PhotoPage<Photo>` |
| `get_photo_taxon_match` | `photoId: number` | `PhotoTaxonMatch` |
| `select_photo_taxon` | `photoId: number`, `taxonId: number` | `PhotoTaxonMapping` |
| `get_photo_taxon_node` | optional `taxonId: number`, optional `showEmpty: boolean` | `PhotoTaxonNode` |
| `browse_photo_taxon` | optional `taxonId: number`, optional `showEmpty: boolean`, optional `includeDescendants: boolean`, optional `cursor: string`, optional `limit: number` | `PhotoPage<PhotoTaxonItem>` |
| `list_photos_by_mapping_status` | `status: PhotoMappingListStatus`, optional `cursor: string`, optional `limit: number` | `PhotoPage<PhotoMappingListItem>` |
| `search_photos_by_mapping_status` | `status: PhotoMappingListStatus`, `query: string`, optional `cursor: string`, optional `limit: number` | `PhotoPage<PhotoMappingListItem>` |
| `get_operations_status` | none | `Record<string, OperationState>` |

Desktop defaults:

- `browse_photo_directory.limit = 50`
- `search_photos.limit = 50`
- `search_photos_by_filename.limit = 50`
- Photo operation list limits default to `50`
- `search_photo_taxa.limit = 50`
- `suggest_photo_taxa.limit = 10`
- `list_taxon_photos.limit = 50`
- `browse_photo_taxon.limit = 50`
- `browse_photo_taxon.show_empty = false`
- `browse_photo_taxon.include_descendants = true`
- `list_photos_by_mapping_status.limit = 50`
- `search_photos_by_mapping_status.limit = 50`

`reveal_photo_in_file_manager` validates that the indexed photo still exists
under the active library root, then selects it in Finder on macOS or Explorer
on Windows. It returns an error when the photo is unavailable, the system file
manager cannot be started, or the platform is unsupported.

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
paths to the frontend. Pass the resource path and the `vividarium` protocol to
Tauri's `convertFileSrc`; the final URL syntax is platform-dependent.

| Resource path | Return |
| --- | --- |
| `photo/{photo_id}` | Original photo bytes. |
| `thumbnail/{photo_id}` | Existing or newly generated thumbnail bytes. |

A successful response includes the detected content type. An invalid ID,
missing file, or unknown resource returns an HTTP error response.
