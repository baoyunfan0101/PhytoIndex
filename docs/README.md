# Backend API Documentation

The Rust backend is exposed through `vividarium_core`. Public interfaces are
grouped by domain:

| Module | Contract |
| --- | --- |
| `storage` | Database locations and photo-library registration. See [STORAGE.md](STORAGE.md). |
| `photos` | Photo indexing, browsing, media, rename, and rename history. See [PHOTOS.md](PHOTOS.md). |
| `mapping` | Photo-to-taxon state, matching, and photographed-taxonomy browsing. See [MAPPING.md](MAPPING.md). |
| `taxonomy` | Taxonomy views, search, mutations, import, SQL, synchronization, and history. See [TAXONOMY.md](TAXONOMY.md). |
| `naming` | Name normalization, Rhai hooks, templates, and hook tests. See [NAMING.md](NAMING.md). |
| `map` | Map settings and geotagged photo pages. See [MAP.md](MAP.md). |
| `operations` | Shared operation summaries, audit rows, and cursor pages. See [OPERATIONS.md](OPERATIONS.md). |
| `models` | Shared serializable DTOs; each type is documented with the domain that returns it. |
| `error` | `CoreError` and the `CoreResult<T>` return type used by core interfaces. |

Desktop updater commands are documented in [UPDATING.md](UPDATING.md).
Build and release procedures are documented separately in
[BUILDING.md](BUILDING.md).

Backend API documents describe only public models and callable interfaces.
Implementation files, private helpers, database connection composition, and
internal scheduling structures are intentionally omitted. Brief behavioral
notes are included only where callers need them to interpret parameters,
returns, validation, pagination, or side effects.
