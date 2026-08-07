import { convertFileSrc } from "@tauri-apps/api/core";
import { call, desktopRuntime } from "./client";
import type { Page } from "./common";
import { demoOperation, type OperationState } from "./tasks";

export type Photo = {
  photo_id: number;
  directory_id: number;
  relative_path: string;
  filename: string;
  file_size: number;
  modified_at_ns: number;
  thumbnail_path: string | null;
};

export type PhotoMetadata = {
  photo_id: number;
  captured_at: string | null;
  camera: string | null;
  width: number | null;
  height: number | null;
  longitude: number | null;
  latitude: number | null;
  exif_json: string | null;
};

export type PhotoRenameRowStatus = "applied" | "no_change" | "failed";
export type PhotoRenameRowOutcome = {
  row_number: number;
  photo_id: number;
  operation_id: number | null;
  status: PhotoRenameRowStatus;
  message: string;
  photo: Photo | null;
};
export type PhotoRenameOperationResult = {
  operation_id: number | null;
  rows: PhotoRenameRowOutcome[];
};

export type PhotoLibrary = { root_path: string; root_directory_id: number };
export type PhotoDirectory = {
  directory_id: number;
  parent_directory_id: number | null;
  name: string;
  relative_path: string;
};
export type PhotoDirectoryItem =
  | { kind: "directory"; directory: PhotoDirectory }
  | { kind: "photo"; photo: Photo };
export type DirectoryEntryCounts = { directory_count: number; file_count: number };

export const demoPhotos: Photo[] = Array.from({ length: 96 }, (_, index) => {
  const species = ["Canis lupus", "Panthera leo", "Vulpes vulpes", "Ursus arctos"][index % 4];
  const filename = `${species.split(" ").join("_")}_${String(index + 1).padStart(3, "0")}.jpg`;
  return {
    photo_id: index + 1,
    directory_id: (index % 4) + 2,
    relative_path: `Mammalia/Field ${Math.floor(index / 24) + 1}/${filename}`,
    filename,
    file_size: 1_200_000 + index * 14_311,
    modified_at_ns: index + 1,
    thumbnail_path: null,
  };
});

export function photoUrl(photo: Photo, thumbnail = false): string {
  if (!desktopRuntime) {
    const hue = (photo.photo_id * 43) % 360;
    const label = photo.filename.replace(/\.[^.]+$/, "").split("_").join(" ");
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="900" height="650"><rect width="100%" height="100%" fill="hsl(${hue} 35% 18%)"/><text x="450" y="340" text-anchor="middle" fill="white" font-family="system-ui" font-size="24">${label}</text></svg>`;
    return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  }
  const resource = thumbnail ? "thumbnail" : "photo";
  return `${convertFileSrc(`${resource}/${photo.photo_id}`, "vividarium")}?v=${photo.modified_at_ns}:${photo.file_size}`;
}

export const getPhotoLibrary = () =>
  call<PhotoLibrary | null>("get_photo_library", undefined, () => ({
    root_path: "/Demo/Vividarium Photos",
    root_directory_id: 1,
  }));
export const getPhotoLibraryCount = () =>
  call<number>("get_photo_library_count", undefined, () => demoPhotos.length);
export const browsePhotoDirectory = (directoryId: number, cursor: string | null = null, limit = 100) =>
  call<Page<PhotoDirectoryItem>>("browse_photo_directory", { directoryId, cursor, limit }, () => {
    const directories: PhotoDirectoryItem[] = directoryId === 1
      ? ["Mammalia", "Aves", "Plantae"].map((name, index) => ({
          kind: "directory",
          directory: { directory_id: index + 2, parent_directory_id: 1, name, relative_path: name },
        }))
      : [];
    return {
      items: [...directories, ...demoPhotos.slice(0, limit).map((photo) => ({ kind: "photo" as const, photo }))],
      next_cursor: null,
    };
  });
export const getPhotoDirectoryCounts = (directoryId: number) =>
  call<DirectoryEntryCounts>("get_photo_directory_counts", { directoryId }, () => ({
    directory_count: directoryId === 1 ? 3 : 0,
    file_count: demoPhotos.length,
  }));
export const refreshPhotoDirectory = (directoryId: number) =>
  call<{ operation: OperationState }>("refresh_photo_directory", { directoryId }, () => ({
    operation: demoOperation("photos", "Refresh complete"),
  }));
export const getPhoto = (photoId: number) =>
  call<Photo>("get_photo", { photoId }, () => demoPhotos.find((photo) => photo.photo_id === photoId) ?? demoPhotos[0]);
export const getPhotoMetadata = (photoId: number) =>
  call<PhotoMetadata>("get_photo_metadata", { photoId }, () => ({
    photo_id: photoId,
    captured_at: "2026-07-25 08:14:22",
    camera: "Vividarium Demo Camera",
    width: 6048,
    height: 4024,
    longitude: 116.391 + photoId * 0.002,
    latitude: 39.907 + photoId * 0.001,
    exif_json: JSON.stringify({ lens: "50mm", exposure: "1/400", iso: 200 }, null, 2),
  }));
export const searchPhotos = (query: string, cursor: string | null = null, limit = 80) =>
  call<Page<Photo>>("search_photos", { query, cursor, limit }, () => ({
    items: demoPhotos.filter((photo) => photo.filename.toLowerCase().includes(query.toLowerCase())).slice(0, limit),
    next_cursor: null,
  }));
export const searchPhotosByFilename = (query: string, cursor: string | null = null, limit = 80) =>
  call<Page<Photo>>("search_photos_by_filename", { query, cursor, limit }, () => ({
    items: demoPhotos.filter((photo) => photo.filename.toLowerCase().includes(query.toLowerCase())).slice(0, limit),
    next_cursor: null,
  }));
export const renamePhoto = (photoId: number, newFilename: string) =>
  call<Photo>("rename_photo", { photoId, newFilename }, () => {
    const photo = demoPhotos.find((item) => item.photo_id === photoId) ?? demoPhotos[0];
    photo.filename = newFilename;
    return { ...photo };
  });
export const renamePhotoDirectory = (directoryId: number, newName: string) =>
  call<PhotoDirectory>("rename_photo_directory", { directoryId, newName }, () => ({
    directory_id: directoryId,
    parent_directory_id: 1,
    name: newName,
    relative_path: newName,
  }));
export const renamePhotoFromTaxon = (photoId: number) =>
  call<Photo>("rename_photo_from_taxon", { photoId }, () => getPhoto(photoId));
export const renamePhotosInDirectoryFromTaxa = (directoryId: number, includeDescendants = true) =>
  call<PhotoRenameOperationResult>(
    "rename_photos_in_directory_from_taxa",
    { directoryId, includeDescendants },
    () => ({ operation_id: null, rows: [] }),
  );
export const revealPhotoInFileManager = (photoId: number) =>
  call<void>("reveal_photo_in_file_manager", { photoId }, () => undefined);
export const openPhotoDirectoryInFileManager = (directoryId: number) =>
  call<void>("open_photo_directory_in_file_manager", { directoryId }, () => undefined);
