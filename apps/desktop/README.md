# Vividarium Desktop

This package contains the React interface and Tauri desktop adapter for Vividarium.

## Source Layout

```text
src/
  App.tsx                  Activity bar, tabs, global search, and PhotoSet routing
  styles.css               Workbench and component styles
  v3/
    api.ts                 Typed Tauri commands and browser-only demo data
    components.tsx         Virtual lists, virtual grids, viewers, and shared controls
    PhotoBrowser.tsx       Standard SearchPhotoSet and TaxonPhotoSet browser
    PhotoContextMenu.tsx   Shared photo context actions
    PhotoDetailView.tsx    Read-only photo detail tab
    PhotosView.tsx         Folder, photo taxonomy, map, and rename history views
    MappingEditor.tsx      Single-photo mapping editor
    MappingView.tsx        Mapping status workbench
    TaxonomyView.tsx       Search, updates, and taxonomy history views
    SettingsView.tsx       Metadata, map, naming, updater, and Rhai hook settings
src-tauri/
  capabilities/            Narrow desktop permissions
  icons/                   Icon source and generated platform formats
  src/                     Tauri IPC, state, paths, and media protocol
  tauri.conf.json          Shared application and bundle configuration
  tauri.macos.conf.json    Apple Silicon DMG and ad-hoc signing settings
  tauri.windows.conf.json  Windows NSIS and WebView2 settings
```

`PhotoSet` is a frontend-only tab model. A search PhotoSet loads
`search_photos`; a taxon PhotoSet loads `list_taxon_photos`. Both use the same
cursor-paged virtual list and virtual grid.

The interface never receives arbitrary file-system privileges. Photos are requested by database ID through the private `vividarium://` protocol, and Rust validates the configured photo root before reading a file.

## Develop

```bash
npm ci
cargo tauri dev
```

## Build

Use the platform scripts from the repository root:

```bash
./scripts/build-macos.sh
```

```powershell
.\scripts\build-windows.ps1
```

See [../../docs/BUILDING.md](../../docs/BUILDING.md) for complete release instructions.
