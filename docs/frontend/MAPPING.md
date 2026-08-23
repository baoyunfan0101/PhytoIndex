# Mapping Domain

Location: `apps/desktop/src/features/mapping`

The Mapping domain displays persistent photo-to-taxon state and provides the
editor used by both the Mapping workspace and single-photo tabs.

## Public interfaces

### `MappingView(props)`

Parameters include `PhotoOpenHandlers` and an optional active flag.

Returns: the singleton mapping workspace with state tabs, cursor-backed photo
results, filename or taxon search for Matched items, photo preview, and an
embedded editor.

### `MappingEditor(props)`

Parameters:

- `photo`: the photo being edited.
- `embedded`: whether the editor is placed inside the Mapping workspace.
- `onOpenTaxon`: opens the requested search-result taxon.
- `refreshKey`: optional external invalidation value.

Returns: current mapping details, persisted Ambiguous candidates, taxonomy
search, and controls to clear, assign, replace, or automatically remap. Current
mapping and search results use the same taxon-card structure. Selecting any
non-action area of a search result opens that taxon in Taxonomy; its compact
Map button changes the photo mapping without triggering navigation. The
taxonomy search receives the larger default share of the vertical editor
split, and its result rows use the same immediate hover and pressed states as
Taxonomy Search.

The Mapping workspace exposes independent dividers between the photo list,
preview, and editor. The standalone editor also separates its photo from the
controls, while both standalone and embedded editors allow the current match
and taxonomy-search sections to be resized vertically.

### `MappingBadge({ status })`

Parameters: `PhotoTaxonStatus`.

Returns: a compact, color-coded status label.

## Status use

The UI treats `matched`, `ambiguous`, and `unmatched` as long-lived states and
`processing` as the visible state while a photo is queued. Navigation to a
taxon is enabled only when the lightweight mapping response contains Matched
state and a taxon ID.

Photo mapping status is reused by the current-photo status bar and the photo
context menu. `View taxon details` is enabled only when the photo has `matched`
state with a taxon ID.
