# Photos Domain

Location: `apps/desktop/src/features/photos`

The Photos domain owns photo browsing and interactions. Folder, photographed
taxonomy, map, search Photo Sets, detail views, media rendering, and the photo
context menu share the same selection and mutation behavior.

## Public pages and components

### `PhotoBrowser(props)`

Parameters include a cursor page controller, optional title, and
`PhotoOpenHandlers` for opening details, taxonomy, or mapping. Returns a
virtual list or grid with a shared photo stage.

### `PhotoSet(props)`

Parameters: either a search `query` or a `taxonId`, plus
`PhotoOpenHandlers`.

Returns: a cursor-backed `PhotoBrowser` for global search or one taxon.

Photo Browser list and media panes, Folder and photographed-taxonomy tree and
preview panes, and Photo Detail media and metadata panes are adjustable within
page-specific minimum widths. Their divider positions belong to the tab view
state.

### `FolderPhotosView(props)`

Parameters: `handlers: PhotoOpenHandlers`.

Returns: an expandable directory tree and photo stage for the active library.

### `TaxonPhotosView(props)`

Parameters: `handlers: PhotoOpenHandlers`.

Returns: an expandable photographed-taxonomy tree and photo stage.

### `PhotoMapView(props)`

Parameters include `handlers` and an optional refresh key.

Returns: a viewport-driven MapLibre page. Map requests use the visible bounds
and backend cursor; only returned markers are mounted.

### `PhotoDetailView({ photo })`

Parameters: one `Photo`.

Returns: a photo stage and copyable file and EXIF metadata. The heading shows
the filename followed by file size, a middle dot, and the modified time from
the supplied `Photo`; the page does not request mapping data. Width, height,
longitude, and latitude are separate detail rows.

### `PhotoStage` and `PhotoThumb`

Parameters: a `Photo`, with display options appropriate to the full image or
thumbnail. Returns media UI using the desktop photo URI.

### `EmptyWorkspace(props)`

Parameters: the recent-search list, recent-search mutation callbacks, a search
submitter, suggestion availability, and an input ref.

Returns: the no-tab workspace containing only photo search, current-query
suggestions, and recent searches when the query is empty.

### `GlobalSearchOverlay(props)`

Parameters: a search submitter, suggestion availability, and a close callback.

Returns: a modal photo-search input with current-query suggestions. Escape or
a pointer action on the backdrop closes it.

### `PhotoSearch(props)`

Parameters: a `PhotoSearchController`, suggestion availability, an ID prefix,
and optional focus configuration.

Returns: the shared keyboard-accessible search input and suggestion list used
by `EmptyWorkspace` and `GlobalSearchOverlay`. Arrow keys select suggestions;
Enter submits the selected suggestion or normalized input. A pointer action
outside the search closes suggestions. Each suggestion shows available
scientific, Chinese, and English names on the first line and rank on the
second line.

### Search state helpers

`useSearchSuggestions(query, enabled)` waits for a 260 ms input pause, then
returns photo-backed taxon suggestions and a loading flag. Suggestion results
render as a low-priority update, and only the latest request may update them.

`usePhotoSearch(onSubmit)` returns the controlled query, submission error,
query setter, and asynchronous submit function.

Submitted photo searches show a centered searching status until the first
result page resolves.

`loadRecentSearches()`, `saveRecentSearches(searches)`,
`addRecentSearch(searches, query)`, and `removeRecentSearch(searches, query)`
manage the bounded, most-recent-first search list in browser-local storage.

## Interaction contract

`PhotoOpenHandlers` contains callbacks for opening details, taxonomy, and the
mapping editor. `usePhotoInteraction` loads lightweight mapping state when a
context menu opens and routes context actions through those handlers.

`emitPhotoMutation(mutation)` broadcasts a committed photo or mapping change.
`usePhotoMutation(listener)` receives it immediately.
`useDeferredPhotoMutation(listener, active)` defers refresh work while a view
is hidden and delivers one invalidation when it becomes active.
