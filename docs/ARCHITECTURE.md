# Vividarium Architecture

Vividarium separates UI composition, native desktop integration, and domain
logic so each layer can be understood and tested independently.

## Layers

```text
apps/desktop/src
  React pages, interactions, shared UI, and typed API wrappers
        |
        | Tauri IPC and events
        v
apps/desktop/src-tauri
  Native dialogs, updater, file-manager integration, media protocol,
  command adapters, and background-operation coordination
        |
        | typed Rust calls
        v
crates/vividarium-core
  Storage, photos, mapping, taxonomy, naming, map, and operation services
        |
        v
Local SQLite databases and photo files
```

## Frontend boundaries

`apps/desktop/src/app` owns tabs, navigation history, workspace restoration,
the activity bar, and application-wide status. Feature domains under
`apps/desktop/src/features` own user-facing workflows:

| Domain | Responsibility |
| --- | --- |
| `photos` | Folders, Taxon Tree, Photo Sets, map, photo media, detail, and file actions. |
| `mapping` | Mapping queues, candidates, and explicit photo-to-taxon assignment. |
| `taxonomy` | Search, hierarchy, name editing, formatted update, Custom SQL, SQL Import, and Direct Import. |
| `operations` | Photo and taxonomy history, audit display, export, and rollback. |
| `settings` | General metadata, storage, libraries, naming, hooks, map, import, update, and contact settings. |

Typed wrappers under `apps/desktop/src/api` are the only frontend layer that
knows Tauri command names. API modules do not import React. Shared UI and state
helpers do not import feature domains. Cross-domain navigation is passed as a
typed handler rather than performed directly by a feature.

## Desktop adapter boundaries

`apps/desktop/src-tauri` contains platform-specific behavior and thin command
adapters. `commands.rs` owns the shared adapter surface, while focused command
groups may live under `commands/` when a domain has an independent request and
response boundary. Command adapters validate desktop-only inputs, translate
errors to IPC strings, and delegate business behavior to `vividarium-core`.

Long-running work is registered with the desktop operation coordinator and is
reported through structured progress. Its task-keyed status map is the single
source for the bottom-right Background UI. Photo Library lifecycle changes and
photo or mapping task startup share one coordinator lock, while each task runs
on a blocking worker in bounded database batches and yields between batches.
Foreground queries remain ordinary asynchronous commands. Native paths,
dialogs, the private
`vividarium://` media protocol, updates, and system application opening remain
outside the core crate.

## Core domain boundaries

`vividarium-core` has no Tauri dependency. Its public modules are grouped by
domain:

| Module | Responsibility |
| --- | --- |
| `storage` | Database locations and Photo Library registry. |
| `photos` | Indexing, browsing, metadata, thumbnails, rename, and availability. |
| `mapping` | Persistent mapping state, filename matching, candidates, and photographed taxonomy. |
| `taxonomy` | Search, hierarchy, mutations, imports, SQL, validation, synchronization, and taxonomy history. |
| `naming` | Name normalization, filename parsing, synonym parsing, Rhai hooks, and project tests. |
| `map` | Tile-provider settings, aggregate photo bounds, and viewport photo pages. |
| `operations` | Shared operation summaries, audit records, and cursor pagination. |

The core returns typed results and `CoreError`. It owns transactions and domain
invariants; the frontend does not reproduce them.

## Storage roles

Vividarium uses separate SQLite roles:

- The metadata database stores application settings, registered resources,
  workspace state, and durable cross-library synchronization events.
- The taxonomy database stores taxa, names, search structures, source
  metadata, and taxonomy operations.
- Each Photo Library database stores its directory tree, indexed photos,
  extracted metadata, thumbnail references, durable initial-index state,
  mapping state, and rename operations.

One taxonomy can therefore serve several independently registered Photo
Libraries. A taxonomy identity change schedules remapping for every registered
library without requiring all libraries to be online at the same time.
Thumbnail files live in library-UUID cache namespaces, and media requests carry
that identity so independently numbered photos cannot share cached content.

## Mutation flow

1. A feature calls one typed frontend API wrapper.
2. The Tauri command delegates to a core domain service or schedules a
   background operation.
3. The core commits one transaction and records audit state when applicable.
4. The desktop publishes completion or structured progress.
5. The frontend emits a domain mutation notification and refreshes only
   affected views.

Formatted taxonomy updates are preview-first. Preview prepares a candidate and
Apply consumes that prepared state, subject to taxonomy revision validation.
SQL Import and Direct Import also separate inspection or validation from the
final replacement action.

## Verification

Backend domain behavior is covered by Rust workspace tests. Frontend pure
state and interaction helpers use Node tests, while TypeScript and production
bundling are checked by the desktop build.

```bash
cargo test --workspace --locked

cd apps/desktop
npm run test:desktop
npm run build
```
