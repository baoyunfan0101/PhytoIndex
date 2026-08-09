# API Modules

Location: `apps/desktop/src/api`

API modules provide typed calls from feature code to Tauri commands. Each file
owns one backend domain, its request types, and response types.

## Modules

| Module | Interface group |
| --- | --- |
| `photos` | Photo records, directories, refresh, search, rename, metadata, media URLs, and file-manager reveal or open actions. |
| `mapping` | Mapping states, candidates, assignment, remapping, mapping queues, and photographed-taxonomy browsing. |
| `taxonomy` | Taxon views, search, suggestions, children, taxon photos, formatted input, staged preview tokens, and prepared apply. |
| `operations` | Photo and taxonomy operation summaries, audit pages, rollback, and single or selected-operation CSV exports to an absolute destination path. |
| `customSql` | Custom SQL execution, result sets, managed SQL inputs, and full-query export. |
| `sqlImport` | SQL Import workspace, validation, apply, and managed inputs. |
| `directImport` | Direct Import database inspection and confirmed replacement. |
| `taxonomyImport` | Shared taxonomy import metadata and result types. |
| `general` | Application-wide theme, workspace, search, taxon-tree display, and CSV delimiter settings. |
| `storage` | Database locations, Taxonomy Database selection, Photo Library registration, and file-manager opening for displayed storage paths. |
| `settings` | Naming settings, Rhai hooks, hook tests, and taxonomy name separator. |
| `map` | Map settings, viewport bounds, and geotagged photo pages. |
| `tasks` | Background operation status and completion waiting. |
| `updater` | Application version, update check, and installation. |
| `dialogs` | Native file, directory, and destination selection. |
| `common` | Cursor page shape, error text, byte formatting, and browser CSV download. |

## Call contract

Exported API functions accept domain values rather than UI events and return
typed promises. Cursor interfaces use:

```ts
type Page<T> = {
  items: T[];
  next_cursor: string | null;
};
```

The first request passes a null cursor. Subsequent requests pass the returned
`next_cursor` without interpreting it. A null next cursor means the page is
complete.

Mutating calls return the committed backend result or an operation handle.
Feature code uses the returned state as authoritative and presents warnings
without converting a committed operation into a failure.

SQL Import validation returns a `sql_import` operation handle. Its structured
progress contains a stage and optional row counts or SQL statement indexes.
The completed result distinguishes a valid candidate from structured taxonomy
validation issues; execution failures remain operation errors.
Direct Import first calls `inspect_direct_import_database`, which performs a
read-only validation and returns the normalized path plus table and column
metadata. It does not replace the current taxonomy. Only the subsequent
`apply_direct_import` call starts a `direct_import` operation. Its completed
result is `TaxonomyImportResult`; schema, integrity, and taxonomy validation
failures are operation errors and leave the current taxonomy unchanged.
