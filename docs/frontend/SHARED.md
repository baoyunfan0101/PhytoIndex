# Shared Module

Location: `apps/desktop/src/shared`

Shared contains code with established use across multiple feature domains. It
does not own business data or import feature modules.

## UI interfaces

`VirtualList<T>` and `VirtualGrid<T>` render windowed collections. Their
parameters include items, dimensions, a stable item key, a renderer, and an
optional end-of-list callback.

`SectionHeader`, `EmptyState`, `Modal`, `Segmented`, `Disclosure`, and `Busy`
provide domain-neutral presentation. `CodeEditor` provides the common SQL and
Rhai editing surface with syntax highlighting.

## Cursor interfaces

`useCursorPage<T, P>(options)` accepts initial parameters and a loader returning
`Page<T>`. It returns `items`, `cursor`, `loading`, `error`, `hasMore`, `load`,
`reload`, and controlled item setters.

`useCursorTree<T, K>(options)` accepts a node key and child-page loader. It
returns expansion state, cached children, loading state, and load-more actions
for expandable folder and taxonomy trees.

## View-state interfaces

`ViewStateProvider({ store, children })` supplies a tab-owned state store.
`useViewState(key, initialValue)` returns a React state pair whose current value
survives page unmounting. Each tab instance receives its own store, preventing
state from leaking between tabs of the same kind.

## Metadata notification

`emitMetadataChange(change)` and `useMetadataChange(listener)` carry committed
metadata invalidations between Settings and the feature domains that consume
those values.
