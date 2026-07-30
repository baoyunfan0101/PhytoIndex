# Vividarium

Vividarium is a local-first desktop application for indexing biological photos, managing a taxonomy knowledge base, and mapping photos to taxa.

Current development version: `v3.0.0`

Vividarium is a Tauri 2 desktop application. The user interface uses React and TypeScript, while application services, SQLite access, file scanning, and imports run in Rust.

## Supported Platforms

| Platform | Minimum system | Release artifact | First launch |
| --- | --- | --- | --- |
| macOS Apple Silicon | macOS 11 | `Vividarium_3.0.0_aarch64.dmg` | Allow the app in Privacy and Security |
| Windows x64 | Windows 10 or 11 | `Vividarium_3.0.0_x64-setup.exe` | Confirm the SmartScreen warning |

Release builds do not require Python, Node.js, Rust, a database server, or other development tools on the destination computer. Windows downloads WebView2 during installation only when the runtime is missing.

Packages and signed in-app updates are available from [GitHub Releases](https://github.com/baoyunfan0101/Vividarium/releases).

## Features

- Register multiple independent photo libraries and activate one at a time.
- Replace the taxonomy base database and apply structured taxonomy updates.
- Map indexed photos to taxa through configurable six-field filename matching.
- Browse large photo collections with cursor-based pagination.
- Browse and search the photographed taxonomy tree.
- Display GPS-enabled photos on a MapLibre map with OpenStreetMap or Tianditu tiles.
- Export taxonomy rebase inputs and photo rename audit operations as UTF-8 CSV files.
- Keep photos, thumbnails, and the SQLite database on the local computer.

## Architecture

```text
apps/
  desktop/
    src/                    React and TypeScript user interface
    src-tauri/              Tauri adapter, IPC commands, and platform config
crates/
  vividarium-core/          Rust domain services, SQLite, scanning, and imports
docs/
  README.md                 Backend API module index
  BUILDING.md               Local and GitHub release instructions
  MAP.md                    Map query and settings backend API
  MAPPING.md                Photo-to-taxon mapping backend API
  NAMING.md                 Name normalization and Rhai hook backend API
  OPERATIONS.md             Shared operation and audit backend API
  PHOTOS.md                 Photos library backend API
  STORAGE.md                Database locations and library registry API
  TAXONOMY.md               Taxonomy knowledge base backend API
  UPDATING.md               Application update backend API
scripts/
  build-macos.sh            Apple Silicon DMG build
  build-windows.ps1         Windows x64 NSIS build
.github/workflows/
  release.yml               Two-platform GitHub release pipeline
Cargo.toml                  Rust workspace and release profile
```

The React application calls typed Rust commands through Tauri IPC. Original photos and generated thumbnails are served through a private `vividarium://` protocol. The core crate does not depend on Tauri, so its services can be tested separately from the desktop shell.

See [docs/README.md](docs/README.md) for the public backend module index.

See [docs/TAXONOMY.md](docs/TAXONOMY.md) for the public taxonomy knowledge base backend models, Rust APIs, and Tauri commands.

See [docs/PHOTOS.md](docs/PHOTOS.md) for the public photo-library backend API.

See [docs/MAPPING.md](docs/MAPPING.md) for automatic photo-to-taxon mapping and photographed-taxonomy browsing.

See [docs/STORAGE.md](docs/STORAGE.md) for metadata, taxonomy, and photo library database location APIs.

See [docs/OPERATIONS.md](docs/OPERATIONS.md) for the shared photo and taxonomy operation history contract.

See [docs/NAMING.md](docs/NAMING.md) for canonical name normalization, Rhai hooks, and project hook tests.

See [docs/MAP.md](docs/MAP.md) for map-photo pagination and map-provider settings.

See [docs/UPDATING.md](docs/UPDATING.md) for the application update backend commands, models, and release endpoint.

## Development

Prerequisites:

- Rust 1.85 or newer
- Node.js 20 or newer and npm
- Tauri 2 platform prerequisites
- Tauri CLI 2

Install the Tauri CLI and frontend dependencies:

```bash
cargo install tauri-cli --version "^2.0" --locked
cd apps/desktop
npm ci
```

Run the desktop application:

```bash
cd apps/desktop
cargo tauri dev
```

Development builds store application data in the repository `data/` directory. Set `VIVIDARIUM_DATA_DIR` to override that location.

## Map Providers

OpenStreetMap is available without configuration. Tianditu can be selected in `Admin > Map` for networks where OpenStreetMap tiles are unavailable. Tianditu requires a browser-side application token (`tk`) from the Tianditu developer platform. The token is stored in local application metadata, masked in the interface, and must not be committed to the repository.

## Test

```bash
cargo test --workspace

cd apps/desktop
npm run build
```

## Build and Release

Build the Apple Silicon DMG on macOS:

```bash
./scripts/build-macos.sh
```

Build the Windows x64 installer from PowerShell on Windows:

```powershell
.\scripts\build-windows.ps1
```

The repository also includes a GitHub Actions workflow that builds both platforms and creates a GitHub release. See [docs/BUILDING.md](docs/BUILDING.md) for prerequisites, package locations, verification, first-launch instructions, and the complete release procedure.

## Data Compatibility

Release builds use the operating system application-data directory. The metadata, taxonomy, and photo library databases all use schema version `2`. Databases with any other schema version are incompatible and rejected; no migration interface is provided.

The permanent application identifier is:

```text
io.github.baoyunfan0101.vividarium
```

## License

[MIT](LICENSE)
