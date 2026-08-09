# Operations Domain

Location: `apps/desktop/src/features/operations`

The Operations domain presents audit history for photo rename and taxonomy
mutation operations.

## Public interface

### `OperationHistoryView(props)`

Parameters:

- `domain`: `"photo"` or `"taxonomy"`.
- `onStatus`: callback receiving user-facing completion or error text.

Returns: cursor-backed operation summaries and audit rows.

Both history lists use the full available width, keep each operation summary on
one row, and support selecting individual loaded operations or all loaded
operations. Audit export and rollback apply only to the selection. Taxonomy
history also exports replayable formatted input when every selected operation
supports it. Export actions always open the native CSV destination dialog.

Opening an operation replaces the list with its detail. The detail toolbar has
a back button and actions scoped to that operation. Audit before and after
values are displayed as indented JSON in scrollable panels. A successful
rollback removes the operation and refreshes the appropriate domain through
its mutation notification. Batch rollback runs selected operations from newest
to oldest.
