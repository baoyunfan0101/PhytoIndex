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
history also exports the replayable formatted inputs contained in the current
selection, combining them into one CSV and ignoring selected operations that
do not have formatted input. The action is disabled only when the selection has
no replayable operation. Export actions always open the native CSV destination
dialog.

Opening an operation replaces the list with its detail. The detail toolbar has
a back button and actions scoped to that operation. Audit before and after
values are displayed as syntax-highlighted, indented JSON in content-sized
editors without internal scrollbars. Each Before/After pair stretches to the
taller value so the two panels remain aligned. A successful
rollback removes the operation and refreshes the appropriate domain through
its mutation notification. Batch rollback runs selected operations from newest
to oldest.
