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
- `refreshKey`: optional external invalidation value.

Returns: current mapping details, persisted Ambiguous candidates, taxonomy
search, and controls to clear, assign, replace, or automatically remap.

### `MappingBadge({ status })`

Parameters: `PhotoTaxonStatus`.

Returns: a compact, color-coded status label.

## Status use

The UI treats `matched`, `ambiguous`, and `unmatched` as long-lived states and
`processing` as the visible state while a photo is queued. Navigation to a
taxon is enabled only when the lightweight mapping response contains Matched
state and a taxon ID.
