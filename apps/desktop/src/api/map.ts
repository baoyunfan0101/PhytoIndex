import { call } from "./client";
import type { Page } from "./common";
import { demoPhotos, type Photo } from "./photos";

export type MapSettings = { provider: "osm" | "tianditu"; tianditu_token: string | null };
export type MapBounds = { west: number; south: number; east: number; north: number };
export type MapPhoto = { photo: Photo; longitude: number; latitude: number };

export const getMapSettings = () =>
  call<MapSettings>("get_map_settings", undefined, () => ({ provider: "osm", tianditu_token: null }));
export const setMapSettings = (settings: MapSettings) =>
  call<MapSettings>("set_map_settings", { settings }, () => settings);
export const getMapPhotoBounds = () =>
  call<MapBounds | null>("get_map_photo_bounds", undefined, () => ({
    west: 116.25,
    south: 39.75,
    east: 116.81,
    north: 40.63,
  }));
export const listMapPhotos = (bounds: MapBounds | null = null, cursor: string | null = null, limit = 500) =>
  call<Page<MapPhoto>>("list_map_photos", { bounds, cursor, limit }, () => {
    const offset = cursor ? Number(cursor) : 0;
    const matches = demoPhotos.map((photo, index) => ({
      photo,
      longitude: 116.25 + (index % 8) * 0.08,
      latitude: 39.75 + Math.floor(index / 8) * 0.08,
    })).filter((item) => {
      if (!bounds) return true;
      const longitudeMatches = bounds.west <= bounds.east
        ? item.longitude >= bounds.west && item.longitude <= bounds.east
        : item.longitude >= bounds.west || item.longitude <= bounds.east;
      return longitudeMatches && item.latitude >= bounds.south && item.latitude <= bounds.north;
    });
    const items = matches.slice(offset, offset + limit);
    return { items, next_cursor: offset + items.length < matches.length ? String(offset + items.length) : null };
  });
