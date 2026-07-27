# Photos and Mapping Backend API

This document describes the public interfaces in `vividarium_core::photos` and
`vividarium_core::mapping`. Storage management is documented in
[STORAGE.md](STORAGE.md), shared history interfaces in
[OPERATIONS.md](OPERATIONS.md), and naming hooks in [NAMING.md](NAMING.md).

All functions take `&Database` as their first parameter and return
`CoreResult<T>`. IDs are signed 64-bit integers. Serialized enum values use
`snake_case`.

## Shared cursor contract

Photo list interfaces return `PhotoPage<T>`:

| Field | Type | Description |
| --- | --- | --- |
| `items` | `Vec<T>` | Items in the current page. |
| `next_cursor` | `Option<String>` | Opaque next-page cursor, or `None` on the last page. |

Pass `None` for the first page and pass a returned cursor back unchanged.
Cursors are scoped to the interface and all filter parameters. `limit` is
clamped to `1..=500`.

## `vividarium_core::photos`

### Core types

| Type | Fields | Description |
| --- | --- | --- |
| `PhotoLibrary` | `root_path`, `root_directory_id` | Active library root and browse entry point. |
| `PhotoDirectory` | `directory_id`, `parent_directory_id`, `name`, `relative_path` | One indexed directory. |
| `Photo` | `photo_id`, `directory_id`, `relative_path`, `filename`, `file_size`, `modified_at_ns`, `thumbnail_path` | One indexed image file. |
| `PhotoMetadata` | `photo_id`, `captured_at`, `camera`, `width`, `height`, `longitude`, `latitude`, `exif_json` | Cached image metadata. |
| `DirectoryEntryCounts` | `directory_count`, `file_count` | Immediate child counts. |
| `PhotoSyncResult` | `directory_id`, `inserted`, `unchanged`, `updated`, `deleted`, `directories_inserted`, `directories_deleted` | Refresh result. |

`PhotoDirectoryItem` is a tagged enum with either
`directory: PhotoDirectory` or `photo: Photo`. Directory browse pages return
child directories before photos.

### Library and browse interfaces

| Function | Parameters after `database` | Return | Description |
| --- | --- | --- | --- |
| `open_library` | `root: &str` | `PhotoLibrary` | Register or activate the library at an existing photo root. |
| `get_library` | none | `Option<PhotoLibrary>` | Return the active library. |
| `get_photo_count` | none | `i64` | Count photos in the active library. |
| `get_directory_counts` | `directory_id: i64` | `DirectoryEntryCounts` | Count immediate directories and photos. |
| `get_photo` | `photo_id: i64` | `Option<Photo>` | Load one photo. |
| `browse_directory` | `directory_id: i64`, `cursor: Option<&str>`, `limit: usize` | `PhotoPage<PhotoDirectoryItem>` | Browse one directory as a cursor page. |
| `refresh_directory` | `directory_id: i64` | `PhotoSyncResult` | Reconcile immediate entries; removed child directories also remove their indexed subtrees. |

Every newly discovered or changed photo is queued for automatic mapping.

### Search interfaces

| Function | Parameters after `database` | Return | Description |
| --- | --- | --- | --- |
| `search_photos_by_filename` | `query: &str`, `cursor: Option<&str>`, `limit: usize` | `PhotoPage<Photo>` | Search filenames only. |
| `search_photos` | `query: &str`, `cursor: Option<&str>`, `limit: usize` | `PhotoPage<Photo>` | Search filename and current mapped taxon, then deduplicate by photo. |

Blank search text returns an empty page.

### Media interfaces

| Function | Parameters after `database` | Return | Description |
| --- | --- | --- | --- |
| `photo_file_path` | `photo_id: i64` | `PathBuf` | Resolve and validate the current absolute file path. |
| `get_photo_metadata` | `photo_id: i64` | `PhotoMetadata` | Return cached metadata or extract and cache it. |
| `get_or_create_thumbnail` | `photo_id: i64`, `thumbnail_root: &Path` | `PathBuf` | Return or create a WebP thumbnail. |
| `rebase_thumbnail_paths` | `thumbnail_root: &Path` | `usize` | Rebind stored thumbnail paths to files found under a new cache root. |

### Rename interfaces

| Function | Parameters after `database` | Return | Description |
| --- | --- | --- | --- |
| `rename_photo` | `photo_id: i64`, `new_filename: &str` | `Photo` | Rename one file to an explicit filename. |
| `rename_photo_from_taxon` | `photo_id: i64` | `Photo` | Rename one currently matched photo with naming settings. |
| `rename_photos_from_taxa` | `photo_ids: &[i64]` | `PhotoRenameOperationResult` | Rename selected photos; rows may succeed independently. |
| `rename_photos_in_directory_from_taxa` | `directory_id: i64`, `include_descendants: bool` | `PhotoRenameOperationResult` | Rename current matched photos in the requested scope. |

`PhotoRenameOperationResult` contains a shared `operation_id` for every
non-empty request and one `PhotoRenameRowOutcome` per input. Each row contains
`row_number`, the same `operation_id`, `photo_id`, `status`, `message`, and
optional current `photo`. `PhotoRenameRowStatus` is `applied`, `no_change`, or
`failed`. An empty request returns no operation.

### Filename format settings

`PhotoFilenameFormatSettings` contains six booleans: `family_zh`,
`family_sci`, `genus_zh`, `genus_sci`, `species_zh`, and `species_sci`.

| Function | Parameters after `database` | Return | Description |
| --- | --- | --- | --- |
| `get_photo_filename_format_settings` | none | `PhotoFilenameFormatSettings` | Read active rename fields. |
| `set_photo_filename_format_settings` | `settings: &PhotoFilenameFormatSettings` | `()` | Save settings; at least one field must be enabled. |
| `format_photo_filename` | `info: &TaxonomicNameInfo`, `suffix: &str`, `settings: &PhotoFilenameFormatSettings` | `String` | Format a filename without reading the database. |

### Photo operation interfaces

The photo module exports the common interfaces below:

| Function | Parameters after `database` | Return |
| --- | --- | --- |
| `list_operations` | `cursor: Option<&str>`, `limit: usize` | `OperationPage<OperationSummary>` |
| `list_operation_audit` | `operation_id: i64`, `cursor: Option<&str>`, `limit: usize` | `OperationPage<OperationAuditRow>` |
| `export_operation_audit` | `operation_id: i64` | UTF-8 pipe-delimited CSV `String` |
| `export_operations_audit` | `operation_ids: &[i64]` | UTF-8 pipe-delimited CSV `String` |
| `export_all_operation_audit` | none | UTF-8 pipe-delimited CSV `String` |
| `rollback_operation` | `operation_id: i64` | `()` |

Successful rollback restores all recorded filenames and deletes the original
operation. See [OPERATIONS.md](OPERATIONS.md) for shared DTOs and CSV columns.

## `vividarium_core::mapping`

### State types

`PhotoTaxonStatus` has three persistent values, `matched`, `ambiguous`, and
`unmatched`, plus the derived temporary value `processing`.

`PhotoMappingSummary` contains:

| Field | Type | Description |
| --- | --- | --- |
| `photo_id` | `i64` | Photo identity. |
| `taxon_id` | `Option<i64>` | Current taxon only when status is `matched`. |
| `status` | `PhotoTaxonStatus` | Current logical state. |

If a photo is queued, its public status is immediately `processing` and
`taxon_id` is `None`, even if an older stored match exists.

`PhotoTaxonCandidate` contains a compact `summary`, the `matched_names` that
produced the candidate, and `accepted_names`. Candidates are persisted and
returned only for an `ambiguous` mapping.

### Mapping read and write interfaces

| Function | Parameters after `database` | Return | Description |
| --- | --- | --- | --- |
| `get_photo_mapping` | `photo_id: i64` | `PhotoMappingSummary` | Read the lightweight current state. Missing photos and broken state invariants are errors. |
| `get_photo_mapping_candidates` | `photo_id: i64` | `Vec<PhotoTaxonCandidate>` | Read persisted candidates for an ambiguous photo; otherwise return an empty vector. |
| `set_photo_mapping` | `photo_id: i64`, `taxon_id: i64` | `PhotoMappingSummary` | Force or replace a mapping, including choosing an ambiguous candidate. |
| `clear_photo_mapping` | `photo_id: i64` | `PhotoMappingSummary` | Set the photo to `unmatched`. |
| `remap_photo` | `photo_id: i64` | `PhotoMappingSummary` | Automatically remap one photo from its current filename. |
| `process_pending_photo_matches` | `progress: &mut MappingProgressCallback` | `PhotoMappingRunResult` | Process the active library queue. |
| `get_metadata` | none | `MappingMetadata` | Return counts for each logical state and the photo taxonomy tree. |

`PhotoMappingRunResult` reports `processed`, `changed`, and remaining
`pending` counts. Automatic mapping candidates are retrieved separately.

### Mapping status list and search

`PhotoMappingListStatus` mirrors the four public states.
`PhotoMappingListItem` contains `photo` and its `mapping`.

| Function | Parameters after `database` | Return |
| --- | --- | --- |
| `list_photos_by_mapping_status` | `status`, `cursor`, `limit` | `PhotoPage<PhotoMappingListItem>` |
| `search_photos_by_mapping_status` | `status`, `query`, `cursor`, `limit` | `PhotoPage<PhotoMappingListItem>` |

For `matched`, search combines filename and current mapped taxon. Other
states search filename only.

### Photo taxonomy navigation

| Function | Parameters after `database` | Return | Description |
| --- | --- | --- | --- |
| `search_photo_taxa` | `query`, `cursor`, `limit` | `PhotoPage<TaxonSearchResult>` | Search taxa that currently have photos with the taxonomy ranked search order. |
| `suggest_photo_taxa` | `query`, `limit` | `Vec<TaxonSuggestion>` | Lightweight autocomplete restricted to taxa with photos. |
| `list_taxon_photos` | `taxon_id`, `cursor`, `limit` | `PhotoPage<Photo>` | List current matched photos for the taxon and descendants. |
| `get_photo_taxon_node` | `taxon_id: Option<i64>`, `show_empty: bool` | `PhotoTaxonNode` | Load one photo taxonomy node or the virtual root. |
| `browse_photo_taxon` | `taxon_id`, `show_empty`, `include_descendants`, `cursor`, `limit` | `PhotoPage<PhotoTaxonItem>` | Browse child taxa followed by photos. |

`PhotoTaxonUsage` contains `taxon_id`, `rank`, accepted `names`,
`direct_photo_count`, and `subtree_photo_count`.

### Name matching settings

`PhotoNameField` values are `family_sci`, `genus_sci`, `species_sci`,
`family_zh`, `genus_zh`, and `species_zh`.
`PhotoNameMatchSettings.priority` is their ordered priority list.

| Function | Parameters after `database` | Return |
| --- | --- | --- |
| `get_photo_name_match_settings` | none | `PhotoNameMatchSettings` |
| `set_photo_name_match_settings` | `settings: &PhotoNameMatchSettings` | `()` |

Within one field, accepted and alias name types are queried together and
deduplicated by `taxon_id`. The search stops at the first field with any
candidate.
