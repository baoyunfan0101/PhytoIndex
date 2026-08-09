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
submits the full search. A pointer action outside the search closes
suggestions. Suggestion rows show available scientific, Chinese, and English
names followed by rank. Submitted searches show a centered searching status
until results resolve.

### `FormattedUpdateView({ onStatus, mutationDisabled })`

Parameters: a status callback and optional mutation guard.

Returns: CSV upload and template actions, an editable formatted-input table,
preview, apply, and row-level result logs. Preview stores a backend candidate
and enables Apply. Apply consumes that candidate and directly commits its
precomputed changeset. Editing rows, importing another file, changing the name
separator or CSV delimiter, or receiving another taxonomy mutation clears the
preview and disables Apply until Preview succeeds again. The current taxonomy
name separator is loaded from Settings metadata. CSV uploads and downloaded
templates use the application-wide CSV delimiter.

### `CustomSqlView(props)`

Parameters include a status callback and optional mutation guard.

Returns: a SQL editor, execution messages, typed result sets, warnings, and
full export for truncated read-only queries. Its source sidebar has two
mutually exclusive VS Code-style groups: Input sources is expanded by default,
and All accessible tables combines uploaded source schemas with the complete
readable `main` taxonomy schema. Sources whose stored copies are unavailable
remain visible in Input sources but are omitted from All accessible tables.
The expanded group body scrolls independently.
CSV sources and exports use the application-wide CSV delimiter.

### `SqlImportSettings({ onApplied })`

Parameters: optional callback invoked after an SQL Import is applied.

Returns: the SQL Import source registry, SQL workspace, validation issues,
metadata, and apply controls. Its source sidebar uses the same mutually
exclusive groups and scrolling behavior as Custom SQL. All accessible tables
combines uploaded source schemas with the current internal taxonomy schema,
which SQL Import can read through the `taxonomy` alias. The `sql_import`
staging schema appears only in Input sources when staging exists, so it is not
duplicated in All accessible tables.

### `DirectImportSettings({ onApplied })`

Parameters: optional callback invoked after a Direct Import is applied.

Returns: a native SQLite selection action, a staged source card containing the
validated path, tables, and columns, an explicit confirmation action,
background import status, and the committed replacement result or validation
error. Selecting a database only inspects it; replacement begins only after
confirmation.

Search results and taxon details, formatted input and result logs, SQL input
sources and editors, and execution or validation outputs use adjustable panes
with minimum sizes appropriate to their controls.

`SqlInputList` owns the shared SQL source sidebar. Only one top-level group is
expanded at a time; selecting the expanded group collapses it. Table rows may
still be expanded independently to inspect their ordered columns and declared
types.

## Reusable interfaces

`TaxonCard` renders a `TaxonSummary` with optional selection and actions. Its
third line displays available Chinese and English names separated by a middle
dot, or a dash when both names are absent.
`useTaxonSuggestions(query, enabled)` returns lightweight suggestions after a
260 ms input pause; only the latest request may publish its low-priority
result. `useTaxonSearch(query, options)` returns submitted search results,
loading state, and an error message. `emitTaxonomyMutation` and
`useTaxonomyMutation` coordinate refresh after committed taxonomy changes.
