# Photos Domain

Location: `apps/desktop/src/features/photos`

The Photos domain owns photo browsing and interactions. Folder, photographed
taxonomy, map, search Photo Sets, detail views, media rendering, and the photo
context menu share the same selection and mutation behavior.

## Public pages and components

### `PhotoBrowser(props)`

Parameters include a cursor page controller, optional title, and
`PhotoOpenHandlers` for opening details, taxonomy, or mapping. Returns a
virtual list or grid with a shared photo stage. It requests a lightweight
taxonomy display summary only for the selected photo in the active tab and
publishes that path to the right side of the application status bar.

### `PhotoSet(props)`

Parameters: either a search `query` with an optional transient `refreshKey`, or
a `taxonId`, plus `PhotoOpenHandlers`.

Returns: a cursor-backed `PhotoBrowser` for global search or one taxon. A
changed search refresh key clears the saved page and reloads from the first
result.

Photo Browser list and media panes, Folder and photographed-taxonomy tree and
preview panes, and Photo Detail media and metadata panes are adjustable within
page-specific minimum widths. Their divider positions belong to the tab view
state.

### `FolderPhotosView(props)`

Parameters: `handlers: PhotoOpenHandlers` and the optional active background
photo operation.

Returns: an expandable directory tree and photo stage for the active library.
Directory lists use cursor pages and the photo grid is virtualized. Nearing the
loaded page boundary requests the next page; thumbnail media is requested only
near the visible grid area.

### `TaxonPhotosView(props)`

Parameters: `handlers: PhotoOpenHandlers` and the optional active background
photo operation.

Returns: an expandable photographed-taxonomy tree and photo stage. Tree rows
and every clickable breadcrumb node use the Photos visible-name preference.

### `PhotoMapView(props)`

Parameters include `handlers`, the owning tab's active state, and the optional
active background photo operation.

Returns: a viewport-driven MapLibre page. The first open fits the aggregate
coordinates of all geotagged photos; the tab then retains its center and zoom.
Map requests use the visible bounds and backend cursor, and only returned
markers are mounted. Selecting a marker reuses one bottom-right thumbnail and
filename preview. Selecting another marker replaces its contents, selecting
the map closes it, and selecting the preview opens Photo Detail.

Metadata progress invalidations are coalesced into periodic page reloads. While
the user has not dragged or zoomed the map, newly discovered GPS bounds may
refit the initial viewport. The first manual map interaction disables automatic
refitting so background work cannot move the user's chosen view.

### `PhotoDetailView({ photo, handlers })`

Parameters: one `Photo`.

Returns: a photo stage and copyable file and EXIF metadata. The heading shows
the filename followed by the mapped taxon's lightweight family-to-current
display path when available; every node uses the Photos visible-name
preference. Width, height, longitude, and latitude are separate detail rows.
The photo stage exposes the shared photo context menu, which loads mapping
state on demand when opened.

### `PhotoStage` and `PhotoThumb`

Parameters: a `Photo`, with display options appropriate to the full image or
thumbnail. Returns media UI using the desktop photo URI, active Photo Library
UUID, and file identity. This prevents cached media from crossing library
boundaries.

Folder, photographed-taxonomy, and Map pages render a non-blocking indexing
notice while relevant background work is active. Existing results stay
interactive. An initial empty pane has a pane-local Loading state; refresh and
pagination keep existing rows visible.

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

`usePhotoTaxonDisplaySummary(photoId)` caches lightweight summaries for the
current view. A missing selection performs no query. Selecting another photo
loads only that photo, repeated selection reuses the cache, and mapping or
taxonomy mutations invalidate affected values. Unmapped, ambiguous, and
processing photos publish no summary. Status paths stay on one line and give
the finest node the highest space priority.
