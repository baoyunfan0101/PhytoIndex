# Photo Mapping Backend API

This document describes the public interfaces in
`vividarium_core::mapping`. Photo records and cursor-page conventions are
documented in [PHOTOS.md](PHOTOS.md), and taxonomy views in
[TAXONOMY.md](TAXONOMY.md).

All functions take `&Database` as their first parameter and return
`CoreResult<T>`. IDs are signed 64-bit integers. Serialized enum values use
`snake_case`.

List interfaces return `PhotoPage<T>`. Pass `None` for the first page and pass
`next_cursor` back unchanged. Cursors are scoped to the interface and all
filter parameters; `limit` is clamped to `1..=500`.

## State types

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

`PhotoMatchedName` contains `name_id`, `name_type`, and `name`.
`PhotoTaxonCandidate` contains a compact `summary`, the `matched_names` that
produced the candidate, and `accepted_names`. Candidates are returned only
for an `ambiguous` mapping.

`PhotoMappingRunResult` contains the number of `processed` photos, mappings
whose state `changed`, and the number still `pending`.

## Mapping read and write interfaces

| Function | Parameters after `database` | Return | Description |
| --- | --- | --- | --- |
| `get_photo_mapping` | `photo_id: i64` | `PhotoMappingSummary` | Read the lightweight current state. Missing photos and broken state invariants are errors. |
| `get_photo_mapping_candidates` | `photo_id: i64` | `Vec<PhotoTaxonCandidate>` | Read persisted candidates for an ambiguous photo; otherwise return an empty vector. |
| `set_photo_mapping` | `photo_id: i64`, `taxon_id: i64` | `PhotoMappingSummary` | Force or replace a mapping, including choosing an ambiguous candidate. |
| `clear_photo_mapping` | `photo_id: i64` | `PhotoMappingSummary` | Set the photo to `unmatched`. |
| `remap_photo` | `photo_id: i64` | `PhotoMappingSummary` | Automatically remap one photo from its current filename. |
| `process_pending_photo_matches` | `progress: &mut MappingProgressCallback` | `PhotoMappingRunResult` | Process the active library queue. |
| `get_metadata` | none | `MappingMetadata` | Return counts for each logical state and the photo taxonomy tree. |

Automatic mapping candidates are retrieved separately from the lightweight
mapping state.

## Mapping status list and search

`PhotoMappingListStatus` mirrors the four public states.
`PhotoMappingListItem` contains `photo` and its `mapping`.

| Function | Parameters after `database` | Return |
| --- | --- | --- |
| `list_photos_by_mapping_status` | `status: PhotoMappingListStatus`, `cursor: Option<&str>`, `limit: usize` | `PhotoPage<PhotoMappingListItem>` |
| `search_photos_by_mapping_status` | `status: PhotoMappingListStatus`, `query: &str`, `cursor: Option<&str>`, `limit: usize` | `PhotoPage<PhotoMappingListItem>` |

For `matched`, search combines filename and current mapped taxon. Other
states search filename only.

## Photo taxonomy navigation

| Function | Parameters after `database` | Return | Description |
| --- | --- | --- | --- |
| `search_photo_taxa` | `query: &str`, `cursor: Option<&str>`, `limit: usize` | `PhotoPage<TaxonSearchResult>` | Search taxa that currently have photos with the taxonomy ranked search order. |
| `suggest_photo_taxa` | `query: &str`, `limit: usize` | `Vec<TaxonSuggestion>` | Lightweight autocomplete restricted to taxa with photos. |
| `list_taxon_photos` | `taxon_id: i64`, `cursor: Option<&str>`, `limit: usize` | `PhotoPage<Photo>` | List current matched photos for the taxon and descendants. |
| `get_photo_taxon_node` | `taxon_id: Option<i64>`, `show_empty: bool` | `PhotoTaxonNode` | Load one photo taxonomy node or the virtual root. |
| `browse_photo_taxon` | `taxon_id: Option<i64>`, `show_empty: bool`, `cursor: Option<&str>`, `limit: usize` | `PhotoPage<PhotoTaxonItem>` | Browse direct child taxa followed by directly mapped photos. |

The desktop `suggest_photo_taxa` command executes the database lookup on a
blocking worker and resolves asynchronously.

`PhotoTaxonUsage` contains `taxon_id`, `rank`, accepted `names`,
`direct_photo_count`, and `subtree_photo_count`.
`PhotoTaxonNode` contains the optional selected `taxon` and its
`subtree_photo_count`. `PhotoTaxonItem` is a tagged enum containing either a
child `taxon` or a `photo`.

## Name matching settings

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
