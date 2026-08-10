# Vividarium Documentation

## Start here

| Guide | Purpose |
| --- | --- |
| [Project README](../README.md) | User-facing overview, installation, quick start, data sources, privacy, and development commands. |
| [Architecture](ARCHITECTURE.md) | Frontend, desktop adapter, core service, storage, and mutation boundaries. |
| [Desktop Frontend](DESKTOP.md) | Index of React application and feature-domain contracts. |
| [Building and Releasing](BUILDING.md) | Local packages, signing variables, verification, and GitHub release workflow. |
| [Changelog](../CHANGELOG.md) | Release-level added, changed, fixed, and removed behavior. |

## Backend API contracts

The Rust backend is exposed through `vividarium_core`. Public interfaces are
grouped by domain:

| Module | Contract |
| --- | --- |
| `storage` | Database locations, Photo Library registration, activation, and lifecycle. See [STORAGE.md](STORAGE.md). |
| `photos` | Initial indexing, refresh, cursor browsing, lazy media, rename, and rename history. See [PHOTOS.md](PHOTOS.md). |
| `mapping` | Photo-to-taxon state, matching, and photographed-taxonomy browsing. See [MAPPING.md](MAPPING.md). |
| `taxonomy` | Taxonomy views, search, mutations, import, SQL, synchronization, and history. See [TAXONOMY.md](TAXONOMY.md). |
| `naming` | Name normalization, Rhai hooks, templates, and hook tests. See [NAMING.md](NAMING.md). |
| `map` | Map settings and geotagged photo pages. See [MAP.md](MAP.md). |
| `operations` | Shared operation summaries, audit rows, and cursor pages. See [OPERATIONS.md](OPERATIONS.md). |
| `models` | Shared serializable DTOs; each type is documented with the domain that returns it. |
| `error` | `CoreError` and the `CoreResult<T>` return type used by core interfaces. |

Desktop updater commands are documented in [UPDATING.md](UPDATING.md).

Backend API documents describe only public models and callable interfaces.
Implementation files, private helpers, database connection composition, and
internal scheduling structures are intentionally omitted. Brief behavioral
notes are included only where callers need them to interpret parameters,
returns, validation, pagination, or side effects.
