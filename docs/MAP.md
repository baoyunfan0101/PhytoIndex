# Map Backend API

The `phytoindex_core::map` module owns map-photo queries and map settings. All
settings are stored through the shared internal metadata module; no generic
metadata command is exposed.

## Models

### `MapTileProvider`

Serialized values:

- `osm`
- `tianditu`

### `MapSettings`

| Field | Type | Description |
| --- | --- | --- |
| `provider` | `MapTileProvider` | Selected tile provider. Defaults to `osm`. |
| `tianditu_token` | `Option<String>` | Tianditu application token. Empty values are stored as `None`. |

### `MapBounds`

| Field | Type | Description |
| --- | --- | --- |
| `west` | `f64` | Western longitude in `-180..=180`. |
| `south` | `f64` | Southern latitude in `-90..=90`. |
| `east` | `f64` | Eastern longitude in `-180..=180`. |
| `north` | `f64` | Northern latitude in `-90..=90`. |

`south` must not exceed `north`. `west > east` represents a viewport crossing
the antimeridian.

### `MapPhoto`

| Field | Type | Description |
| --- | --- | --- |
| `photo` | `Photo` | Minimal indexed-photo record used by photo views. |
| `longitude` | `f64` | Stored photo longitude. |
| `latitude` | `f64` | Stored photo latitude. |

## Core Interfaces

### `get_map_settings`

```rust
pub fn get_map_settings(database: &Database) -> CoreResult<MapSettings>
```

Returns stored settings or the default OSM settings.

### `set_map_settings`

```rust
pub fn set_map_settings(
    database: &Database,
    settings: MapSettings,
) -> CoreResult<MapSettings>
```

Stores settings and returns the normalized value.

### `list_map_photos`

```rust
pub fn list_map_photos(
    database: &Database,
    bounds: Option<MapBounds>,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<MapPhoto>>
```

| Parameter | Description |
| --- | --- |
| `bounds` | Optional viewport. `None` includes every geotagged photo. |
| `cursor` | `None` for the first page; otherwise the previous `next_cursor`. |
| `limit` | Requested page size, clamped to `1..=500`. |

Only photos with both coordinates are returned. Results are ordered by
`photo_id`. The opaque cursor is bound to the exact bounds and cannot be reused
for another viewport.

## Desktop Commands

JavaScript invoke parameters use camel case. Returned object fields use snake
case.

| Command | Parameters | Return |
| --- | --- | --- |
| `get_map_settings` | none | `MapSettings` |
| `set_map_settings` | `settings: MapSettings` | normalized `MapSettings` |
| `list_map_photos` | optional `bounds: MapBounds`, optional `cursor: string`, optional `limit: number` | `PhotoPage<MapPhoto>` |

The desktop map-photo page limit defaults to `500`.
