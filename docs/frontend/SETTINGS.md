# Settings Domain

Location: `apps/desktop/src/features/settings`

The Settings domain presents application metadata and resource configuration
through one settings workbench.

## Public interface

### `SettingsView(props)`

| Parameter | Type | Description |
| --- | --- | --- |
| `section` | `SettingsSection` | Currently displayed settings page. |
| `onSectionChange` | `(section) => void` | Updates the settings page stored by the owning tab. |
| `onWorkspaceChanged` | optional callback | Refreshes application state after Photo Library changes. |
| `onTaxonomyImported` | optional callback | Refreshes taxonomy and mapping state after a completed import. |
| `generalSettings` | `GeneralSettings` | Current application-wide settings. |
| `onGeneralSettingsChange` | `(settings) => void` | Applies a committed General settings value to the application. |
| `generalSettingsLoadError` | optional string | Reports a load failure while the default settings remain usable. |

Returns the complete Settings workbench.

`SettingsSection` includes General, Storage, Photo Libraries, Taxonomy
Databases, SQL Import, Direct Import, Naming, Map, Filename Parser, Synonym
Splitter, and About.

## Pages

### General

Reads and updates the application theme, workspace-tab restoration preference,
recent-search limit, and global CSV delimiter. The delimiter choices are comma,
semicolon, tab, and pipe; comma is the default. It controls formatted-update
templates and imports, SQL CSV sources and exports, and every history CSV
export. Changes are persisted immediately; there is no page-level Save action.
Recent-search contents remain in browser-local storage.

### Storage

Shows the metadata database, current taxonomy database, default Photo Library
database directory, default taxonomy database directory, and current taxonomy
source metadata. Paths wrap as needed so the complete value remains visible.
Every database and directory path has an Open action that reveals a file or
opens a directory in the system file manager. Taxonomy database Move starts in
the configured default taxonomy directory. Both directory Change actions
reopen at their current value.

### Photo Libraries

Registers, opens, renames, rebinds, relocates, and removes Photo Library
resources. Creating or registering a library starts its database destination
dialog in the configured default Photo Library database directory. It does not
own photo browsing or mapping behavior.

### Taxonomy Databases

Expands to SQL Import and Direct Import pages.

SQL Import accepts persistent CSV and SQLite input sources plus SQL. `Validate`
executes the SQL and builds a candidate database through one background
operation. The completed report shows each validation message once and keeps
Apply disabled for invalid taxonomy data. SQL, SQLite, file, and
candidate-build failures use the page error state. `Apply` is enabled only for
the latest successful validation and replaces the taxonomy database through
the background operation API.

Direct Import selects one ready-to-use SQLite database from the configured
default taxonomy directory. The first `Import` action only inspects and
validates the selected file. The page then shows its normalized path, tables,
and columns in the same form as SQL Import input sources. The current taxonomy
remains unchanged until the user selects `Confirm import`, which starts a
`direct_import` background operation and shows an in-page running state until
it completes. Validation or replacement failure leaves the current taxonomy
unchanged and appears in the page error state. Success refreshes taxonomy
resources and schedules every registered Photo Library for remapping.

### Naming

Edits the six-field mapping priority, mapped-photo filename fields, and the
formatted-input multiple-name separator. Each value remains visible with a
usable default if loading fails.

### Map

Edits the tile provider and provider token. The token is editable only for
Tianditu; selecting another provider preserves the stored Tianditu value.

### Filename Parser and Synonym Splitter

Each Hook page edits one Rhai source and its ordered project tests. Tests are
numbered by array position and contain raw input, expected output, actual
output, and pass or failure state.

`Test` runs the current unsaved source and tests without persistence. `Save`
is enabled only after every test passes and the source and tests remain
unchanged. Saving persists the source and tests atomically.

### About

Shows the product name, application version, database schema, author, email,
and project GitHub link. Email and GitHub links open through their system
applications with exact URL scopes. It checks GitHub Releases for application
updates and installs an available update through the desktop updater.

## Metadata notification

`emitMetadataChange(change)` publishes a committed metadata update.
`useMetadataChange(listener)` subscribes to changes so dependent views refresh
only the values they consume. CSV delimiter changes invalidate an outstanding
formatted-update preview.
