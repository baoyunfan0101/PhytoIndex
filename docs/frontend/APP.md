# Application Module

Location: `apps/desktop/src/app`

The application module assembles feature domains into the desktop window. It
owns the activity bar, toolbar, tab instances, active Photo Library, global
search, navigation history, and operation-status display.

## Public interfaces

### `App()`

Location: `apps/desktop/src/App.tsx`

Parameters: none.

Returns: the React application tree containing `DesktopShell`.

`App` is the composition entry point and contains no page or domain logic.

### `DesktopShell()`

Parameters: none.

Returns: the complete desktop workspace.

The shell creates tab instances and supplies domain pages with navigation and
status callbacks. A taxon tab has an instance ID independent of its current
taxon, allowing in-tab navigation without conflating the tab with a taxon ID.

### Navigation history

`createNavigationHistory(tabId)` creates history for the initial tab.
`recordNavigation(history, tabId)` returns history containing the new target.
`findNavigationTarget(history, direction, openTabIds)` returns the next open
target and updated history. `pruneNavigationHistory(history, openTabIds)`
returns history with closed targets removed.

### `useOperationObserver()`

Parameters: none.

Returns: the latest `OperationsStatus` plus observer state used by the shell.
It also publishes photo invalidation after mapping work completes.

## Feature integration

The shell imports top-level pages from `features/*`. It passes callbacks for
opening photo details, taxonomy records, mapping editors, and Photo Sets. Page
components do not create or manage application tabs directly.
