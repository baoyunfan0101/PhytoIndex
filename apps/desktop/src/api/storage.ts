import { call } from "./client";
import type { PhotoLibrary } from "./photos";

export type DatabaseLocations = {
  metadata_database: string;
  taxonomy_database: string;
  default_taxonomy_directory: string;
  default_photo_library_directory: string;
  active_photo_library_uuid: string | null;
};
export type PhotoLibraryRegistration = {
  library_uuid: string;
  display_name: string;
  root_path: string;
  db_path: string;
  last_opened_at: string;
};
export type PhotoLibraryWorkspace = PhotoLibraryRegistration & {
  active: boolean;
  root_available: boolean;
  database_available: boolean;
};

const demoLibrary = (): PhotoLibraryWorkspace => ({
  library_uuid: "demo-library",
  display_name: "Demo Library",
  root_path: "/Demo/Vividarium Photos",
  db_path: "/Demo/Vividarium/Photo Libraries/demo.db",
  last_opened_at: new Date().toISOString(),
  active: true,
  root_available: true,
  database_available: true,
});

export const openPhotoLibrary = (root: string) =>
  call<PhotoLibrary>("open_photo_library", { root }, () => ({ root_path: root, root_directory_id: 1 }));
export const getDatabaseLocations = () =>
  call<DatabaseLocations>("get_database_locations", undefined, () => ({
    metadata_database: "/Demo/Vividarium/metadata.db",
    taxonomy_database: "/Demo/Vividarium/taxonomy.db",
    default_taxonomy_directory: "/Demo/Vividarium",
    default_photo_library_directory: "/Demo/Vividarium/Photo Libraries",
    active_photo_library_uuid: "demo-library",
  }));
export const listPhotoLibraries = () =>
  call<PhotoLibraryWorkspace[]>("list_photo_libraries", undefined, () => [demoLibrary()]);
export const registerPhotoLibrary = (rootPath: string, databasePath: string, displayName: string | null) =>
  call<PhotoLibraryRegistration>("register_photo_library", { rootPath, databasePath, displayName }, () => ({
    library_uuid: crypto.randomUUID(),
    display_name: displayName || "Photo Library",
    root_path: rootPath,
    db_path: databasePath,
    last_opened_at: new Date().toISOString(),
  }));
export const switchPhotoLibrary = (libraryUuid: string) =>
  call<PhotoLibraryRegistration>("switch_photo_library", { libraryUuid }, () => ({ ...demoLibrary(), library_uuid: libraryUuid }));
export const renamePhotoLibrary = (libraryUuid: string, displayName: string) =>
  call<PhotoLibraryRegistration>("rename_photo_library", { libraryUuid, displayName }, () => ({
    ...demoLibrary(), library_uuid: libraryUuid, display_name: displayName,
  }));
export const rebindPhotoLibraryRoot = (libraryUuid: string, rootPath: string) =>
  call<PhotoLibraryRegistration>("rebind_photo_library_root", { libraryUuid, rootPath }, () => ({
    ...demoLibrary(), library_uuid: libraryUuid, root_path: rootPath,
  }));
export const rebindPhotoLibraryDatabase = (libraryUuid: string, databasePath: string) =>
  call<PhotoLibraryRegistration>("rebind_photo_library_database", { libraryUuid, databasePath }, () => ({
    ...demoLibrary(), library_uuid: libraryUuid, db_path: databasePath,
  }));
export const relocatePhotoLibraryDatabase = (libraryUuid: string, databasePath: string) =>
  call<PhotoLibraryRegistration>("relocate_photo_library_database", { libraryUuid, databasePath }, () => ({
    ...demoLibrary(), library_uuid: libraryUuid, db_path: databasePath,
  }));
export const removePhotoLibrary = (libraryUuid: string) =>
  call<void>("remove_photo_library", { libraryUuid }, () => undefined);
export const relocateTaxonomyDatabase = (databasePath: string) =>
  call<DatabaseLocations>("relocate_taxonomy_database", { databasePath }, getDatabaseLocations);
export const setDefaultTaxonomyDatabaseDirectory = (directory: string) =>
  call<DatabaseLocations>("set_default_taxonomy_database_directory", { directory }, getDatabaseLocations);
export const setDefaultPhotoLibraryDatabaseDirectory = (directory: string) =>
  call<DatabaseLocations>("set_default_photo_library_database_directory", { directory }, getDatabaseLocations);

export function photoLibraryAvailabilityLabel(library: PhotoLibraryWorkspace): string {
  if (!library.database_available && !library.root_available) return "Database and photo root missing";
  if (!library.database_available) return "Database missing";
  if (!library.root_available) return "Photo root missing";
  return "Available";
}
