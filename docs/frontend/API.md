# API Modules

Location: `apps/desktop/src/api`

API modules provide typed calls from feature code to Tauri commands. Each file
owns one backend domain, its request types, and response types.

## Modules

| Module | Interface group |
| --- | --- |
| `photos` | Photo records, directories, refresh, search, rename, metadata, media URLs, and file-manager reveal or open actions. |
| `mapping` | Mapping states, candidates, assignment, remapping, mapping queues, and photographed-taxonomy browsing. |
| `taxonomy` | Taxon views, search, suggestions, children, taxon photos, formatted input, preview, and apply. |
| `operations` | Photo and taxonomy operation summaries, audit pages, rollback, and exports. |
| `customSql` | Custom SQL execution, result sets, managed SQL inputs, and full-query export. |
| `baseImport` | Base-import workspace, validation, apply, metadata, and managed inputs. |
| `storage` | Database locations, Taxonomy Database selection, and Photo Library registration. |
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

Base Import validation returns a `base_import` operation handle. Its structured
progress contains a stage and optional row counts or SQL statement indexes.
The completed result distinguishes a valid candidate from structured taxonomy
validation issues; execution failures remain operation errors.
