# Desktop Frontend

The desktop frontend is a React application organized around user-facing
domains. Each domain owns its pages, interactions, and domain-specific UI.
The application shell composes those domains into tabs and global navigation.

See [ARCHITECTURE.md](ARCHITECTURE.md) for the complete frontend, Tauri
adapter, core service, and storage layering.

## Domain guide

| Domain | Frontend use |
| --- | --- |
| [Application](frontend/APP.md) | Window shell, tabs, navigation history, global search, and domain assembly. |
| [API](frontend/API.md) | Typed Tauri calls grouped by backend domain. |
| [Photos](frontend/PHOTOS.md) | Photo sets, folders, photographed taxonomy, map browsing, media, details, and photo actions. |
| [Mapping](frontend/MAPPING.md) | Mapping-state queues and the photo-to-taxon editor. |
| [Taxonomy](frontend/TAXONOMY.md) | Taxon search, formatted updates, Custom SQL, SQL Import, and Direct Import. |
| [Operations](frontend/OPERATIONS.md) | Photo and taxonomy operation history, audit rows, export, and rollback. |
| [Settings](frontend/SETTINGS.md) | Storage, libraries, naming, map, hooks, taxonomy imports, and application settings. |
| [Shared](frontend/SHARED.md) | Reusable UI, cursor controllers, and view-state persistence used across domains. |
| [Styles](frontend/STYLES.md) | Theme, shell, shared, and domain style entry points. |

## Using the domains

`App` mounts `DesktopShell`. The shell owns application-wide navigation and
passes explicit handlers into feature pages. Feature pages call only the API
modules they need and use shared components for behavior that is common across
multiple domains.

Photo-library selection defines the workspace for Photos, Mapping, and Map
pages. Taxonomy pages can be used independently. Cross-domain actions are
expressed through typed handlers or mutation notifications so a feature does
not need to know how another page is mounted.

Imports follow one direction:

```text
App -> app shell -> feature domains -> API domains
                         |
                         +------------> shared
```

API modules do not import React or feature code. Shared modules do not import
feature modules. A feature may expose a focused component or event for another
feature when that behavior belongs to the providing domain.
