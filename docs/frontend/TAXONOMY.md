# Taxonomy Domain

Location: `apps/desktop/src/features/taxonomy`

The Taxonomy domain owns taxon browsing and taxonomy mutation workspaces. It
can operate without an available Photo Library except for explicit requests
that list photos belonging to a taxon.

## Public pages

### `TaxonomySearchView(props)`

Parameters include an optional selected taxon, a callback for the currently
displayed taxon, and a callback for opening taxon photos.

Returns: taxon search results or a single-taxon page with breadcrumb and child
navigation. Typing requests lightweight suggestions after a 260 ms input
pause. Arrow keys select a suggestion, and Enter or a pointer selection
submits the full search.

### `FormattedUpdateView({ mutationDisabled })`

Parameters: optional mutation guard.

Returns: CSV upload and template actions, an editable formatted-input table,
preview, apply, and row-level result logs. The current taxonomy name separator
is loaded from Settings metadata.

### `CustomSqlView(props)`

Parameters include a status callback and optional mutation guard.

Returns: managed input sources, SQL editor, execution messages, typed result
sets, warnings, and full export for truncated read-only queries.

### `BaseImportSettings({ onApplied })`

Parameters: optional callback invoked after a base database is applied.

Returns: the base-import source registry, SQL workspace, validation issues,
metadata, and apply controls.

Search results and taxon details, formatted input and result logs, SQL input
sources and editors, and execution or validation outputs use adjustable panes
with minimum sizes appropriate to their controls.

## Reusable interfaces

`TaxonCard` renders a `TaxonSummary` with optional selection and actions.
`useTaxonSuggestions(query, enabled)` returns lightweight suggestions after a
260 ms input pause; only the latest request may publish its low-priority
result. `useTaxonSearch(query, options)` returns submitted search results,
loading state, and an error message. `emitTaxonomyMutation` and
`useTaxonomyMutation` coordinate refresh after committed taxonomy changes.
