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
| `onBaseReplaced` | optional callback | Refreshes taxonomy and mapping state after replacement. |
| `generalSettings` | `GeneralSettings` | Current application-wide settings. |
| `onGeneralSettingsChange` | `(settings) => void` | Applies a committed General settings value to the application. |
| `generalSettingsLoadError` | optional string | Reports a load failure while the default settings remain usable. |

Returns the complete Settings workbench.

`SettingsSection` includes General, Storage, Photo Libraries, Taxonomy
Databases, Naming, Map, Filename Parser, Synonym Splitter, and About.

## Pages

### General

Reads and updates the application theme, workspace-tab restoration preference,
and recent-search limit. Changes are persisted immediately; there is no
page-level Save action. Recent-search contents remain in browser-local storage.

### Storage

Shows the metadata database, current taxonomy database, default Photo Library
database directory, default taxonomy database directory, and current taxonomy
source metadata. Long paths remain on one line and expose the complete value
through a tooltip.

### Photo Libraries

Registers, opens, renames, rebinds, relocates, and removes Photo Library
resources. It does not own photo browsing or mapping behavior.

### Taxonomy Databases

Accepts persistent CSV and SQLite input sources plus SQL. `Validate` executes
the SQL, builds a candidate database, and returns SQL messages and the
validation report. `Apply` is enabled only for the latest successful
validation and replaces the taxonomy database through the background
operation API.

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

Shows the product name, application version, database schema, author, and
project GitHub link. It checks GitHub Releases for application updates and
installs an available update through the desktop updater.

## Metadata notification

`emitMetadataChange(change)` publishes a committed metadata update.
`useMetadataChange(listener)` subscribes to changes so dependent views refresh
only the values they consume.
