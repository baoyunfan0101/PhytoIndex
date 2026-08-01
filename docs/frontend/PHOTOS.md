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

Returns: a photo stage and copyable photo, metadata, and taxon fields.

### `PhotoStage` and `PhotoThumb`

Parameters: a `Photo`, with display options appropriate to the full image or
thumbnail. Returns media UI using the desktop photo URI.

## Interaction contract

`PhotoOpenHandlers` contains callbacks for opening details, taxonomy, and the
mapping editor. `usePhotoInteraction` loads lightweight mapping state when a
context menu opens and routes context actions through those handlers.

`emitPhotoMutation(mutation)` broadcasts a committed photo or mapping change.
`usePhotoMutation(listener)` receives it immediately.
`useDeferredPhotoMutation(listener, active)` defers refresh work while a view
is hidden and delivers one invalidation when it becomes active.
