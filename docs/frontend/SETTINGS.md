# Settings Domain

Location: `apps/desktop/src/features/settings`

The Settings domain owns user-editable application metadata and storage
configuration. It presents General, Storage, Photo Libraries, Taxonomy
Databases, Naming, Map, Hooks, Base Import, and About sections in one workspace.

## Public interface

### `SettingsView(props)`

Parameters include the selected settings section, a section-change callback,
and application callbacks used to reload Photo Libraries, switch the active
library, select a Taxonomy Database, and handle taxonomy replacement.

Returns: the complete settings workbench. Unsaved hook source, test cases, and
expected values remain in the view state while the tab is inactive.

## Metadata notification

The shared `emitMetadataChange(change)` publishes a committed metadata update.
`useMetadataChange(listener)` subscribes to those changes. The event contains
the changed metadata kind so dependent pages refresh only the values they use.

Rhai hook settings always display either the saved source or the backend
template. Test execution returns actual results for every configured case;
saving uses the backend test-and-save operation.
