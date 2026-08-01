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

Photo history displays the recorded path, old filename, and new filename.
Taxonomy history displays operation audit rows and indicates whether formatted
input can be exported for replay. The view supports one-operation export,
combined export, and rollback. A successful rollback removes the operation and
refreshes the appropriate domain through its mutation notification.
