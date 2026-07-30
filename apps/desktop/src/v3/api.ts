import { Channel, convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import defaultPhotoFilenameHook from "../../../../crates/vividarium-core/src/naming/templates/photo_filename.rhai?raw";
import defaultSynonymAuthorityHook from "../../../../crates/vividarium-core/src/naming/templates/synonym_authority.rhai?raw";

export type Page<T> = { items: T[]; next_cursor: string | null };

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

export type PhotoLibrary = {
  root_path: string;
  root_directory_id: number;
};

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

export type PhotoDirectory = {
  directory_id: number;
  parent_directory_id: number | null;
  name: string;
  relative_path: string;
};

export type PhotoDirectoryItem =
  | { kind: "directory"; directory: PhotoDirectory }
  | { kind: "photo"; photo: Photo };

export type DirectoryEntryCounts = {
  directory_count: number;
  file_count: number;
};

export type OperationState = {
  module: string;
  task_id: string | null;
  operation: string | null;
  running: boolean;
  started_at: string | null;
  finished_at: string | null;
  message: string;
  processed: number;
  total: number | null;
  result: unknown;
  error: string | null;
};

export type OperationsStatus = Record<string, OperationState>;

export type PhotoTaxonStatus =
  | "matched"
  | "ambiguous"
  | "unmatched"
  | "processing";

export type PhotoMappingSummary = {
  photo_id: number;
  taxon_id: number | null;
  status: PhotoTaxonStatus;
};

export type PhotoMappingListItem = {
  photo: Photo;
  mapping: PhotoMappingSummary;
};

export type TaxonDisplayNames = {
  sci_name: string | null;
  zh_name: string | null;
  en_name: string | null;
};

export type TaxonBreadcrumbItem = {
  taxon_id: number;
  rank: TaxonRank;
  names: TaxonDisplayNames;
};

export type TaxonRank = "kingdom" | "order" | "family" | "genus" | "species";

export type TaxonSummary = {
  taxon_id: number;
  rank: TaxonRank;
  breadcrumb: TaxonBreadcrumbItem[];
  names: TaxonDisplayNames;
};

export type TaxonNameDetail = {
  name_id: number;
  name: string;
  authority_year: string | null;
  source: string | null;
};

export type TaxonDetail = {
  taxon_id: number;
  rank: TaxonRank;
  parent_taxon_id: number | null;
  geological_range: string | null;
  names: {
    sci_name: TaxonNameDetail;
    synonyms: TaxonNameDetail[];
    zh_name: TaxonNameDetail | null;
    zh_aliases: TaxonNameDetail[];
    en_name: TaxonNameDetail | null;
    en_aliases: TaxonNameDetail[];
  };
};

export type TaxonNameMatch = {
  name_id: number;
  name_type: string;
  name: string;
};

export type TaxonSearchResult = {
  summary: TaxonSummary;
  detail: TaxonDetail;
  matches: TaxonNameMatch[];
};

export type TaxonSuggestion = {
  taxon_id: number;
  rank: TaxonRank;
  names: TaxonDisplayNames;
  matches: TaxonNameMatch[];
};

export type TaxonChild = {
  taxon_id: number;
  rank: TaxonRank;
  names: TaxonDisplayNames;
};

export type TaxonDetailNode = {
  summary: TaxonSummary;
  detail: TaxonDetail;
  children: Page<TaxonChild>;
};

export type PhotoMatchedName = {
  name_id: number;
  name_type: string;
  name: string;
};

export type PhotoTaxonCandidate = {
  summary: TaxonSummary;
  matched_names: PhotoMatchedName[];
  accepted_names: TaxonDisplayNames;
};

export type PhotoMappingDetail = {
  mapping: PhotoMappingSummary;
  candidates: PhotoTaxonCandidate[];
};

export type MappingMetadata = {
  mapped_photo_count: number;
  unmatched_photo_count: number;
  ambiguous_photo_count: number;
  processing_photo_count: number;
  mapping_taxa_count: number;
};

export type PhotoTaxonUsage = {
  taxon_id: number;
  rank: TaxonRank;
  names: TaxonDisplayNames;
  direct_photo_count: number;
  subtree_photo_count: number;
};

export type PhotoTaxonNode = {
  taxon: PhotoTaxonUsage | null;
  subtree_photo_count: number;
};

export type PhotoTaxonItem =
  | { kind: "taxon"; taxon: PhotoTaxonUsage }
  | { kind: "photo"; photo: Photo };

export type TaxonInputRow = {
  selected_taxon_id?: number | null;
  kingdom?: string | null;
  order?: string | null;
  family?: string | null;
  genus?: string | null;
  species?: string | null;
  authority_year?: string | null;
  synonyms?: string[];
  zh_name?: string | null;
  zh_alias?: string[];
  en_name?: string | null;
  en_alias?: string[];
  geological_range?: string | null;
  source?: string | null;
};

export type TaxonRowOutcome = {
  row_number: number;
  operation_types: string[];
  message: string;
  target: TaxonSummary | null;
  parent: TaxonSummary | null;
  candidates: TaxonSummary[];
  changes: Array<{
    kind: string;
    field: string;
    old_value: string | null;
    new_value: string | null;
  }>;
};

export type TaxonomyPreviewResult = {
  delimiter: string;
  encoding: string;
  rows: TaxonRowOutcome[];
};

export type TaxonomyOperationResult = TaxonomyPreviewResult & {
  operation_id: number;
  total_rows: number;
  succeeded_rows: number;
  failed_rows: number;
};

export type OperationSummary = {
  operation_id: number;
  kind: string;
  source: string;
  applied_at: string;
  total_items: number;
  succeeded_items: number;
  failed_items: number;
  rollbackable: boolean;
  has_formatted_input: boolean;
};

export type OperationAuditRow = {
  operation_id: number;
  sequence: number;
  entity_type: string;
  entity_id: string | null;
  action: string;
  before_json: unknown;
  after_json: unknown;
  succeeded: boolean;
  message: string;
};

export type SqlDataSource =
  | { kind: "csv"; alias: string; path: string }
  | { kind: "sqlite"; alias: string; path: string };

export type SqlValue =
  | { type: "null" }
  | { type: "integer"; value: number }
  | { type: "real"; value: number }
  | { type: "text"; value: string }
  | { type: "blob"; value: string };

export type SqlColumn = {
  name: string;
  declared_type: string | null;
};

export type SqlResultSet = {
  statement_index: number;
  columns: SqlColumn[];
  rows: SqlValue[][];
  truncated: boolean;
};

export type SqlStatementMessage = {
  statement_index: number;
  affected_rows: number | null;
  message: string;
};

export type CustomSqlExecutionResult = {
  operation_id: number | null;
  changeset_size: number;
  result_sets: SqlResultSet[];
  messages: SqlStatementMessage[];
};

export type SqlExportResult = {
  path: string;
  row_count: number;
};

export type SqlSourceObject = {
  name: string;
  object_type: "table" | "view" | "virtual_table";
  columns: SqlColumn[];
};

export type SqlSourceSchema = {
  alias: string;
  objects: SqlSourceObject[];
};

export type BaseImportSession = {
  session_id: string;
};

export type BaseImportExecutionResult = {
  statements_executed: number;
  session_revision: number;
};

export type BaseImportIssue = {
  code: string;
  message: string;
  table: string | null;
  row_identifier: string | null;
};

export type BaseImportValidationResult = {
  can_apply: boolean;
  taxa_count: number;
  name_counts: Array<{ name_type: string; count: number }>;
  normalization_changes: number;
  total_warning_count: number;
  total_error_count: number;
  warnings: BaseImportIssue[];
  errors: BaseImportIssue[];
};

export type TaxonomyBaseMetadata = {
  source_path: string;
  taxa_count: number;
  taxon_names_count: number;
  imported_at: string;
};

export type MapSettings = {
  provider: "osm" | "tianditu";
  tianditu_token: string | null;
};

export type MapBounds = {
  west: number;
  south: number;
  east: number;
  north: number;
};

export type MapPhoto = {
  photo: Photo;
  longitude: number;
  latitude: number;
};

export type NamingHookKind = "photo_filename" | "synonym_authority";
export type NamingHookSettings = {
  photo_filename: string | null;
  synonym_authority: string | null;
};
export type NamingHookTemplates = {
  photo_filename: string;
  synonym_authority: string;
};
export type NamingHookTestResult =
  | { kind: "photo_filename"; output: unknown }
  | { kind: "synonym_authority"; output: unknown };
export type NamingHookTestCase = {
  name: string;
  input: string;
  expected: NamingHookTestResult;
};
export type NamingHookTestCases = {
  photo_filename: NamingHookTestCase[];
  synonym_authority: NamingHookTestCase[];
};
export type NamingHookTestReport = {
  kind: NamingHookKind;
  passed: number;
  failed: number;
  cases: Array<NamingHookTestCase & {
    actual: NamingHookTestResult | null;
    passed: boolean;
    error: string | null;
  }>;
};

export type PhotoNameField =
  | "species_sci"
  | "species_zh"
  | "genus_sci"
  | "genus_zh"
  | "family_sci"
  | "family_zh";
export type PhotoNameMatchSettings = { priority: PhotoNameField[] };
export type PhotoFilenameFormatSettings = {
  family_zh: boolean;
  family_sci: boolean;
  genus_zh: boolean;
  genus_sci: boolean;
  species_zh: boolean;
  species_sci: boolean;
};
export type AppUpdateInfo = {
  current_version: string;
  version: string;
  notes: string | null;
  published_at: string | null;
};
export type AppUpdateEvent =
  | { event: "started"; data: { content_length: number | null } }
  | { event: "progress"; data: { chunk_length: number; downloaded: number } }
  | { event: "finished" };

const desktopRuntime = "__TAURI_INTERNALS__" in window;

const demoPhotos: Photo[] = Array.from({ length: 96 }, (_, index) => {
  const species = ["Canis lupus", "Panthera leo", "Vulpes vulpes", "Ursus arctos"][index % 4];
  return {
    photo_id: index + 1,
    directory_id: (index % 4) + 2,
    relative_path: `Mammalia/Field ${Math.floor(index / 24) + 1}/${species.split(" ").join("_")}_${String(index + 1).padStart(3, "0")}.jpg`,
    filename: `${species.split(" ").join("_")}_${String(index + 1).padStart(3, "0")}.jpg`,
    file_size: 1_200_000 + index * 14_311,
    modified_at_ns: index + 1,
    thumbnail_path: null,
  };
});

const demoTaxa: TaxonSearchResult[] = [
  demoTaxon(1001, "species", "Canis lupus", "Wolf"),
  demoTaxon(1002, "species", "Panthera leo", "Lion"),
  demoTaxon(1003, "species", "Vulpes vulpes", "Red fox"),
  demoTaxon(1004, "species", "Ursus arctos", "Brown bear"),
];

let demoMappings = new Map<number, PhotoMappingSummary>(
  demoPhotos.map((photo, index) => [
    photo.photo_id,
    {
      photo_id: photo.photo_id,
      taxon_id: index % 5 === 3 ? null : demoTaxa[index % demoTaxa.length].summary.taxon_id,
      status: index % 11 === 0 ? "processing" : index % 7 === 0 ? "ambiguous" : index % 5 === 3 ? "unmatched" : "matched",
    },
  ]),
);
function demoTaxon(
  taxonId: number,
  rank: TaxonRank,
  scientific: string,
  english: string,
): TaxonSearchResult {
  const sciName = { name_id: taxonId * 10, name: scientific, authority_year: null, source: "Demo" };
  const names = { sci_name: scientific, zh_name: null, en_name: english };
  return {
    summary: {
      taxon_id: taxonId,
      rank,
      breadcrumb: [
        { taxon_id: 1, rank: "kingdom", names: { sci_name: "Animalia", zh_name: null, en_name: "Animals" } },
        { taxon_id: 2, rank: "order", names: { sci_name: "Carnivora", zh_name: null, en_name: "Carnivorans" } },
      ],
      names,
    },
    detail: {
      taxon_id: taxonId,
      rank,
      parent_taxon_id: 2,
      geological_range: "Recent",
      names: {
        sci_name: sciName,
        synonyms: [],
        zh_name: null,
        zh_aliases: [],
        en_name: { name_id: taxonId * 10 + 1, name: english, authority_year: null, source: "Demo" },
        en_aliases: [],
      },
    },
    matches: [{ name_id: sciName.name_id, name_type: "sci_name", name: scientific }],
  };
}

async function call<T>(
  command: string,
  args: Record<string, unknown> | undefined,
  demo: () => T | Promise<T>,
): Promise<T> {
  if (desktopRuntime) {
    return invoke<T>(command, args);
  }
  await new Promise((resolve) => window.setTimeout(resolve, 40));
  return demo();
}

export function photoUrl(photo: Photo, thumbnail = false): string {
  if (!desktopRuntime) {
    const hue = (photo.photo_id * 43) % 360;
    const label = photo.filename.replace(/\.[^.]+$/, "").split("_").join(" ");
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="900" height="650"><defs><linearGradient id="g" x2="1" y2="1"><stop stop-color="hsl(${hue} 35% 24%)"/><stop offset="1" stop-color="hsl(${(hue + 70) % 360} 42% 9%)"/></linearGradient></defs><rect width="100%" height="100%" fill="url(#g)"/><circle cx="450" cy="285" r="118" fill="none" stroke="rgba(255,255,255,.18)" stroke-width="2"/><path d="M330 360 420 250l62 71 45-50 83 89Z" fill="rgba(255,255,255,.18)"/><text x="450" y="520" text-anchor="middle" fill="rgba(255,255,255,.72)" font-family="system-ui" font-size="24">${label}</text></svg>`;
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

export const getAppVersion = () =>
  call<string>("get_app_version", undefined, () => "3.0.0");
export const checkAppUpdate = () =>
  call<AppUpdateInfo | null>("check_app_update", undefined, () => null);
export async function installAppUpdate(onEvent: (event: AppUpdateEvent) => void): Promise<void> {
  if (!desktopRuntime) return;
  const onEventChannel = new Channel<AppUpdateEvent>();
  onEventChannel.onmessage = onEvent;
  await invoke("install_app_update", { onEvent: onEventChannel });
}

export const getPhotoLibraryCount = () =>
  call<number>("get_photo_library_count", undefined, () => demoPhotos.length);

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
  call<PhotoLibraryWorkspace[]>("list_photo_libraries", undefined, () => [{
    library_uuid: "demo-library",
    display_name: "Demo Library",
    root_path: "/Demo/Vividarium Photos",
    db_path: "/Demo/Vividarium/Photo Libraries/demo.db",
    last_opened_at: new Date().toISOString(),
    active: true,
    root_available: true,
    database_available: true,
  }]);

export const registerPhotoLibrary = (
  rootPath: string,
  databasePath: string,
  displayName: string | null,
) => call<PhotoLibraryRegistration>(
  "register_photo_library",
  { rootPath, databasePath, displayName },
  () => ({
    library_uuid: crypto.randomUUID(),
    display_name: displayName || "Photo Library",
    root_path: rootPath,
    db_path: databasePath,
    last_opened_at: new Date().toISOString(),
  }),
);

export const switchPhotoLibrary = (libraryUuid: string) =>
  call<PhotoLibraryRegistration>("switch_photo_library", { libraryUuid }, async () => ({
    ...(await listPhotoLibraries())[0],
    library_uuid: libraryUuid,
  }));

export const renamePhotoLibrary = (libraryUuid: string, displayName: string) =>
  call<PhotoLibraryRegistration>("rename_photo_library", { libraryUuid, displayName }, async () => ({
    ...(await listPhotoLibraries())[0],
    library_uuid: libraryUuid,
    display_name: displayName,
  }));

export const rebindPhotoLibraryRoot = (libraryUuid: string, rootPath: string) =>
  call<PhotoLibraryRegistration>("rebind_photo_library_root", { libraryUuid, rootPath }, async () => ({
    ...(await listPhotoLibraries())[0],
    library_uuid: libraryUuid,
    root_path: rootPath,
  }));

export const rebindPhotoLibraryDatabase = (libraryUuid: string, databasePath: string) =>
  call<PhotoLibraryRegistration>("rebind_photo_library_database", { libraryUuid, databasePath }, async () => ({
    ...(await listPhotoLibraries())[0],
    library_uuid: libraryUuid,
    db_path: databasePath,
  }));

export const relocatePhotoLibraryDatabase = (libraryUuid: string, databasePath: string) =>
  call<PhotoLibraryRegistration>("relocate_photo_library_database", { libraryUuid, databasePath }, async () => ({
    ...(await listPhotoLibraries())[0],
    library_uuid: libraryUuid,
    db_path: databasePath,
  }));

export const removePhotoLibrary = (libraryUuid: string) =>
  call<void>("remove_photo_library", { libraryUuid }, () => undefined);

export const relocateTaxonomyDatabase = (databasePath: string) =>
  call<DatabaseLocations>("relocate_taxonomy_database", { databasePath }, getDatabaseLocations);

export const setDefaultTaxonomyDatabaseDirectory = (directory: string) =>
  call<DatabaseLocations>("set_default_taxonomy_database_directory", { directory }, getDatabaseLocations);

export const setDefaultPhotoLibraryDatabaseDirectory = (directory: string) =>
  call<DatabaseLocations>("set_default_photo_library_database_directory", { directory }, getDatabaseLocations);

export async function selectPhotoDirectory(): Promise<string | null> {
  if (!desktopRuntime) {
    return "/Demo/Vividarium Photos";
  }
  const selected = await open({ directory: true, multiple: false });
  return typeof selected === "string" ? selected : null;
}

export async function selectSqliteDatabase(): Promise<string | null> {
  if (!desktopRuntime) return "/Demo/Vividarium/source.db";
  const selected = await open({
    directory: false,
    multiple: false,
    filters: [{ name: "SQLite database", extensions: ["db", "sqlite", "sqlite3"] }],
  });
  return typeof selected === "string" ? selected : null;
}

export async function selectCsvFile(): Promise<string | null> {
  if (!desktopRuntime) return "/Demo/Vividarium/source.csv";
  const selected = await open({
    directory: false,
    multiple: false,
    filters: [{ name: "CSV", extensions: ["csv"] }],
  });
  return typeof selected === "string" ? selected : null;
}

export async function selectDatabaseDestination(defaultPath?: string): Promise<string | null> {
  if (!desktopRuntime) return defaultPath ?? "/Demo/Vividarium/destination.db";
  return save({
    defaultPath,
    filters: [{ name: "SQLite database", extensions: ["db", "sqlite", "sqlite3"] }],
  });
}

export async function selectCsvDestination(defaultPath?: string): Promise<string | null> {
  if (!desktopRuntime) return defaultPath ?? "/Demo/Vividarium/export.csv";
  return save({ defaultPath, filters: [{ name: "CSV", extensions: ["csv"] }] });
}

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

export const startPhotoMapping = () =>
  call<{ operation: OperationState }>("start_photo_mapping", undefined, () => ({
    operation: demoOperation("mapping", "Mapping complete"),
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

export const renamePhotoFromTaxon = (photoId: number) =>
  call<Photo>("rename_photo_from_taxon", { photoId }, () => getPhoto(photoId));

export const revealPhotoInFileManager = (photoId: number) =>
  call<void>("reveal_photo_in_file_manager", { photoId }, () => undefined);

export const getPhotoMapping = (photoId: number) =>
  call<PhotoMappingSummary>("get_photo_mapping", { photoId }, () =>
    demoMappings.get(photoId) ?? { photo_id: photoId, taxon_id: null, status: "unmatched" });

export const getPhotoMappingCandidates = (photoId: number) =>
  call<PhotoTaxonCandidate[]>("get_photo_mapping_candidates", { photoId }, () => {
    const mapping = demoMappings.get(photoId);
    return mapping?.status === "ambiguous"
      ? demoTaxa.slice(0, 3).map((taxon) => ({
          summary: taxon.summary,
          matched_names: taxon.matches,
          accepted_names: taxon.summary.names,
        }))
      : [];
  });

export const clearPhotoMapping = (photoId: number) =>
  call<PhotoMappingSummary>("clear_photo_mapping", { photoId }, () => {
    const mapping: PhotoMappingSummary = { photo_id: photoId, taxon_id: null, status: "unmatched" };
    demoMappings.set(photoId, mapping);
    return mapping;
  });

export const setPhotoMapping = (photoId: number, taxonId: number) =>
  call<PhotoMappingSummary>("set_photo_mapping", { photoId, taxonId }, () => {
    const mapping: PhotoMappingSummary = { photo_id: photoId, taxon_id: taxonId, status: "matched" };
    demoMappings.set(photoId, mapping);
    return mapping;
  });

export const remapPhoto = (photoId: number) =>
  call<PhotoMappingSummary>("remap_photo", { photoId }, () => getPhotoMapping(photoId));

export const getMappingMetadata = () =>
  call<MappingMetadata>("get_mapping_metadata", undefined, () => ({
    mapped_photo_count: 68,
    unmatched_photo_count: 14,
    ambiguous_photo_count: 8,
    processing_photo_count: 6,
    mapping_taxa_count: 21,
  }));

export const listPhotosByMappingStatus = (status: PhotoTaxonStatus, cursor: string | null = null, limit = 80) =>
  call<Page<PhotoMappingListItem>>("list_photos_by_mapping_status", { status, cursor, limit }, () => ({
    items: demoPhotos
      .map((photo) => ({ photo, mapping: demoMappings.get(photo.photo_id)! }))
      .filter((item) => item.mapping.status === status)
      .slice(0, limit),
    next_cursor: null,
  }));

export const searchPhotosByMappingStatus = (
  status: PhotoTaxonStatus,
  query: string,
  cursor: string | null = null,
  limit = 80,
) =>
  call<Page<PhotoMappingListItem>>("search_photos_by_mapping_status", { status, query, cursor, limit }, () => ({
    items: demoPhotos
      .map((photo) => ({ photo, mapping: demoMappings.get(photo.photo_id)! }))
      .filter((item) => item.mapping.status === status && item.photo.filename.toLowerCase().includes(query.toLowerCase()))
      .slice(0, limit),
    next_cursor: null,
  }));

export const searchTaxa = (query: string, limit = 80) =>
  call<TaxonSearchResult[]>("search_taxa", { query, limit }, () =>
    demoTaxa.filter((taxon) => displayTaxon(taxon.summary).toLowerCase().includes(query.toLowerCase())).slice(0, limit));

export const suggestTaxa = (query: string, limit = 10) =>
  call<TaxonSuggestion[]>("suggest_taxa", { query, limit }, () =>
    demoTaxa
      .filter((taxon) => displayTaxon(taxon.summary).toLowerCase().includes(query.toLowerCase()))
      .slice(0, limit)
      .map((taxon) => ({ ...taxon.summary, matches: taxon.matches })));

export const suggestPhotoTaxa = (query: string, limit = 10) =>
  call<TaxonSuggestion[]>("suggest_photo_taxa", { query, limit }, () => suggestTaxa(query, limit));

export const getTaxonDetailNode = (taxonId: number, childrenCursor: string | null = null, childrenLimit = 80) =>
  call<TaxonDetailNode>("get_taxon_detail_node", { taxonId, childrenCursor, childrenLimit }, () => {
    const taxon = demoTaxa.find((item) => item.summary.taxon_id === taxonId) ?? demoTaxa[0];
    return { summary: taxon.summary, detail: taxon.detail, children: { items: [], next_cursor: null } };
  });

export const listTaxonChildren = (taxonId: number, cursor: string | null = null, limit = 80) =>
  call<Page<TaxonChild>>("list_taxon_children", { taxonId, cursor, limit }, () => ({ items: [], next_cursor: null }));

export const listTaxonPhotos = (taxonId: number, cursor: string | null = null, limit = 80) =>
  call<Page<Photo>>("list_taxon_photos", { taxonId, cursor, limit }, () => ({
    items: demoPhotos.filter((photo) => demoMappings.get(photo.photo_id)?.taxon_id === taxonId).slice(0, limit),
    next_cursor: null,
  }));

export const getPhotoTaxonNode = (taxonId: number | null, showEmpty = false) =>
  call<PhotoTaxonNode>("get_photo_taxon_node", { taxonId, showEmpty }, () => ({
    taxon: taxonId
      ? { taxon_id: taxonId, rank: "species", names: demoTaxa[0].summary.names, direct_photo_count: 12, subtree_photo_count: 12 }
      : null,
    subtree_photo_count: demoPhotos.length,
  }));

export const browsePhotoTaxon = (
  taxonId: number | null,
  showEmpty = false,
  includeDescendants = true,
  cursor: string | null = null,
  limit = 80,
) =>
  call<Page<PhotoTaxonItem>>("browse_photo_taxon", { taxonId, showEmpty, includeDescendants, cursor, limit }, () => ({
    items: taxonId === null
      ? demoTaxa.map((taxon) => ({
          kind: "taxon" as const,
          taxon: {
            taxon_id: taxon.summary.taxon_id,
            rank: taxon.summary.rank,
            names: taxon.summary.names,
            direct_photo_count: 12,
            subtree_photo_count: 24,
          },
        }))
      : demoPhotos.slice(0, limit).map((photo) => ({ kind: "photo" as const, photo })),
    next_cursor: null,
  }));

function demoOperationSummaries(domain: "photo" | "taxonomy"): OperationSummary[] {
  return [1, 2, 3].map((operationId) => ({
    operation_id: operationId,
    kind: domain === "photo" ? "rename" : operationId === 3 ? "custom_sql" : "formatted_update",
    source: domain === "photo" ? "manual_rename" : operationId === 3 ? "custom_sql" : "formatted_update",
    applied_at: `2026-07-${20 + operationId} 10:30:00`,
    total_items: operationId + 1,
    succeeded_items: operationId + 1,
    failed_items: 0,
    rollbackable: true,
    has_formatted_input: domain === "taxonomy" && operationId !== 3,
  }));
}

export const listPhotoOperationSummaries = (cursor: string | null = null, limit = 80) =>
  call<Page<OperationSummary>>("list_photo_operations", { cursor, limit }, () => ({
    items: demoOperationSummaries("photo"),
    next_cursor: null,
  }));

export const listPhotoOperationAudit = (operationId: number, cursor: string | null = null, limit = 80) =>
  call<Page<OperationAuditRow>>("list_photo_operation_audit", { operationId, cursor, limit }, () => ({
    items: [{
      operation_id: operationId,
      sequence: 1,
      entity_type: "photo",
      entity_id: "1",
      action: "rename",
      before_json: { directory_relative_path: "Mammalia", filename: "before.jpg" },
      after_json: { directory_relative_path: "Mammalia", filename: "after.jpg" },
      succeeded: true,
      message: "Renamed",
    }],
    next_cursor: null,
  }));

export const rollbackPhotoOperation = (operationId: number) =>
  call<void>("rollback_photo_operation", { operationId }, () => undefined);

export const exportPhotoOperationAudit = (operationId: number, destinationPath: string) =>
  call<void>("export_photo_operation_audit", { operationId, destinationPath }, () => undefined);

export const exportAllPhotoOperationAudit = (destinationPath: string) =>
  call<void>("export_all_photo_operation_audit", { destinationPath }, () => undefined);

export const listTaxonomyOperationSummaries = (cursor: string | null = null, limit = 80) =>
  call<Page<OperationSummary>>("list_taxonomy_operations", { cursor, limit }, () => ({
    items: demoOperationSummaries("taxonomy"),
    next_cursor: null,
  }));

export const listTaxonomyOperationAudit = (operationId: number, cursor: string | null = null, limit = 80) =>
  call<Page<OperationAuditRow>>("list_taxonomy_operation_audit", { operationId, cursor, limit }, () => ({
    items: [{
      operation_id: operationId,
      sequence: 1,
      entity_type: "taxon_name",
      entity_id: "10",
      action: "update",
      before_json: { name: "Before" },
      after_json: { name: "After" },
      succeeded: true,
      message: "Updated",
    }],
    next_cursor: null,
  }));

export const rollbackTaxonomyOperation = (operationId: number) =>
  call<void>("rollback_taxonomy_operation", { operationId }, () => undefined);

export const exportTaxonomyOperationAudit = (operationId: number, destinationPath: string) =>
  call<void>("export_taxonomy_operation_audit", { operationId, destinationPath }, () => undefined);

export const exportAllTaxonomyOperationAudit = (destinationPath: string) =>
  call<void>("export_all_taxonomy_operation_audit", { destinationPath }, () => undefined);

export const exportTaxonomyOperationInput = (operationId: number) =>
  call<string>("export_taxonomy_operation_input", { operationId }, () => getTaxonomyTemplate());

export const exportAllReplayableTaxonomyInputs = () =>
  call<string>("export_all_replayable_taxonomy_inputs", undefined, () => getTaxonomyTemplate());

export const getTaxonomyTemplate = () =>
  call<string>("get_taxonomy_formatted_update_template", undefined, () => "kingdom|order|family|genus|species|authority_year|synonyms|zh_name|zh_alias|en_name|en_alias|geological_range|source\n");
export const parseTaxonomyCsv = (input: string) =>
  call<TaxonInputRow[]>("parse_taxonomy_input_csv", { input }, () => parseDemoTaxonomyCsv(input));
export const previewTaxonomyRows = (rows: TaxonInputRow[]) =>
  call<TaxonomyPreviewResult>("preview_taxonomy_rows", { rows }, () => ({
    delimiter: "|",
    encoding: "UTF-8",
    rows: rows.map((row, index) => ({
      row_number: index + 1,
      operation_types: ["new_name"],
      message: "Ready to apply",
      target: null,
      parent: null,
      candidates: [],
      changes: [{ kind: "append_name", field: "species", old_value: null, new_value: row.species ?? null }],
    })),
  }));
export const applyTaxonomyRows = (rows: TaxonInputRow[]) =>
  call<TaxonomyOperationResult>("apply_taxonomy_rows", { rows }, async () => ({
    ...(await previewTaxonomyRows(rows)),
    operation_id: 100,
    total_rows: rows.length,
    succeeded_rows: rows.length,
    failed_rows: 0,
  }));

export const executeCustomSql = (
  sql: string,
  sources: SqlDataSource[],
  maximumResultRows: number | null = 1000,
) => call<CustomSqlExecutionResult>("execute_custom_taxonomy_sql", {
  request: {
    sql,
    sources,
    maximum_result_rows: maximumResultRows,
  },
}, () => ({
  operation_id: null,
  changeset_size: 0,
  result_sets: [{
    statement_index: 1,
    columns: [{ name: "demo", declared_type: "TEXT" }],
    rows: [[{ type: "text", value: "Demo result" }]],
    truncated: false,
  }],
  messages: [{ statement_index: 1, affected_rows: null, message: "Query completed" }],
}));

export const inspectSqlDataSource = (source: SqlDataSource) =>
  call<SqlSourceSchema>("inspect_sql_data_source", { source }, () => ({
    alias: source.alias,
    objects: [{
      name: source.kind === "csv" ? source.alias : "taxa",
      object_type: "table",
      columns: [{ name: "value", declared_type: "TEXT" }],
    }],
  }));

export const exportCustomSqlQuery = (
  sql: string,
  sources: SqlDataSource[],
  destinationPath: string,
) => call<SqlExportResult>("export_custom_taxonomy_query", {
  request: { sql, sources, destination_path: destinationPath },
}, () => ({ path: destinationPath, row_count: 1 }));

export const createBaseImportSession = () =>
  call<BaseImportSession>("create_base_import_session", undefined, () => ({ session_id: crypto.randomUUID() }));

export const addBaseImportCsvSource = (sessionId: string, tableName: string, path: string) =>
  call<SqlSourceSchema>("add_base_import_csv_source", {
    request: { session_id: sessionId, table_name: tableName, path },
  }, () => ({
    alias: "main",
    objects: [{ name: tableName, object_type: "table", columns: [{ name: "value", declared_type: "TEXT" }] }],
  }));

export const addBaseImportSqliteSource = (sessionId: string, path: string) =>
  call<SqlSourceSchema>("add_base_import_sqlite_source", {
    request: { session_id: sessionId, path },
  }, () => ({
    alias: "main",
    objects: [{ name: "source_taxa", object_type: "table", columns: [{ name: "taxon_id", declared_type: "INTEGER" }] }],
  }));

export const inspectBaseImportSources = (sessionId: string) =>
  call<SqlSourceSchema[]>("inspect_base_import_sources", { sessionId }, () => []);

export const executeBaseImportSql = (sessionId: string, sql: string) =>
  call<BaseImportExecutionResult>("execute_base_import_sql", {
    request: { session_id: sessionId, sql },
  }, () => ({ statements_executed: sql.split(";").filter(Boolean).length, session_revision: 1 }));

export const validateBaseImport = (sessionId: string) =>
  call<BaseImportValidationResult>("validate_base_import", { sessionId }, () => ({
    can_apply: true,
    taxa_count: 125000,
    name_counts: [
      { name_type: "sci_name", count: 125000 },
      { name_type: "synonym", count: 60000 },
    ],
    normalization_changes: 0,
    total_warning_count: 0,
    total_error_count: 0,
    warnings: [],
    errors: [],
  }));

export const applyBaseImport = (sessionId: string) =>
  call<OperationState>("apply_base_import", { sessionId }, () => demoOperation("mapping", "Base import applied"));

export const discardBaseImportSession = (sessionId: string) =>
  call<void>("discard_base_import_session", { sessionId }, () => undefined);

export const getDefaultBaseImportSql = () =>
  call<string>("get_default_base_import_sql", undefined, () => [
    "ATTACH DATABASE '' AS base;",
    "CREATE TABLE base.taxa AS SELECT * FROM main.source_taxa;",
    "CREATE TABLE base.taxon_names AS SELECT * FROM main.source_names;",
  ].join("\n"));

export const saveDefaultBaseImportSql = (sql: string) =>
  call<void>("save_default_base_import_sql", { sql }, () => undefined);

export const resetDefaultBaseImportSql = () =>
  call<string>("reset_default_base_import_sql", undefined, getDefaultBaseImportSql);
export const getTaxonomyBaseMetadata = () =>
  call<TaxonomyBaseMetadata | null>("get_taxonomy_base_metadata", undefined, () => null);

export const getMapSettings = () =>
  call<MapSettings>("get_map_settings", undefined, () => ({ provider: "osm", tianditu_token: null }));
export const setMapSettings = (settings: MapSettings) =>
  call<MapSettings>("set_map_settings", { settings }, () => settings);
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
    return {
      items,
      next_cursor: offset + items.length < matches.length ? String(offset + items.length) : null,
    };
  });

export const getNamingHookSettings = () =>
  call<NamingHookSettings>("get_naming_hook_settings", undefined, () => ({ photo_filename: null, synonym_authority: null }));
export const getNamingHookTemplates = () =>
  call<NamingHookTemplates>("get_naming_hook_templates", undefined, () => ({
    photo_filename: defaultPhotoFilenameHook,
    synonym_authority: defaultSynonymAuthorityHook,
  }));
export const setNamingHook = (kind: NamingHookKind, script: string | null) =>
  call<void>("set_naming_hook", { kind, script }, () => undefined);
export const getNamingHookTestCases = () =>
  call<NamingHookTestCases>("get_naming_hook_test_cases", undefined, defaultNamingHookTestCases);
export const setNamingHookTestCases = (kind: NamingHookKind, cases: NamingHookTestCase[]) =>
  call<void>("set_naming_hook_test_cases", { kind, cases }, () => undefined);
export const runNamingHookTests = (kind: NamingHookKind, script: string | null) =>
  call<NamingHookTestReport>("run_naming_hook_tests", { kind, script }, async () => {
    const cases = (await getNamingHookTestCases())[kind];
    return { kind, passed: cases.length, failed: 0, cases: cases.map((item) => ({ ...item, actual: item.expected, passed: true, error: null })) };
  });

export const getPhotoNameMatchSettings = () =>
  call<PhotoNameMatchSettings>("get_photo_name_match_settings", undefined, () => ({
    priority: ["species_sci", "species_zh", "genus_sci", "genus_zh", "family_sci", "family_zh"],
  }));
export const setPhotoNameMatchSettings = (settings: PhotoNameMatchSettings) =>
  call<void>("set_photo_name_match_settings", { settings }, () => undefined);
export const getPhotoFilenameFormatSettings = () =>
  call<PhotoFilenameFormatSettings>("get_photo_filename_format_settings", undefined, () => ({
    family_zh: false,
    family_sci: false,
    genus_zh: false,
    genus_sci: false,
    species_zh: false,
    species_sci: true,
  }));
export const setPhotoFilenameFormatSettings = (settings: PhotoFilenameFormatSettings) =>
  call<void>("set_photo_filename_format_settings", { settings }, () => undefined);
export const getTaxonomyNameSeparator = () =>
  call<string>("get_taxonomy_name_separator", undefined, () => ";");
export const setTaxonomyNameSeparator = (separator: string) =>
  call<void>("set_taxonomy_name_separator", { separator }, () => undefined);

export const getOperationsStatus = () =>
  call<OperationsStatus>("get_operations_status", undefined, () => ({}));

export async function waitForOperation(
  module: string,
  taskId: string | null,
  onChange?: (operation: OperationState) => void,
): Promise<OperationState> {
  if (!taskId) {
    return demoOperation(module, "Complete");
  }
  while (true) {
    const operation = (await getOperationsStatus())[module];
    if (operation) onChange?.(operation);
    if (!operation || operation.task_id !== taskId || !operation.running) {
      return operation ?? demoOperation(module, "Complete");
    }
    await new Promise((resolve) => window.setTimeout(resolve, 250));
  }
}

export function downloadCsv(filename: string, content: string) {
  const url = URL.createObjectURL(new Blob([content], { type: "text/csv;charset=utf-8" }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}

export function displayTaxon(summary: Pick<TaxonSummary, "taxon_id" | "names">): string {
  return summary.names.sci_name ?? summary.names.zh_name ?? summary.names.en_name ?? `Taxon ${summary.taxon_id}`;
}

export function photoLibraryAvailabilityLabel(library: PhotoLibraryWorkspace): string {
  if (!library.database_available && !library.root_available) return "Database and photo root missing";
  if (!library.database_available) return "Database missing";
  if (!library.root_available) return "Photo root missing";
  return "Available";
}

export function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function demoOperation(module: string, message: string): OperationState {
  return {
    module,
    task_id: null,
    operation: null,
    running: false,
    started_at: null,
    finished_at: null,
    message,
    processed: 0,
    total: null,
    result: null,
    error: null,
  };
}

function parseDemoTaxonomyCsv(input: string): TaxonInputRow[] {
  const lines = input.trim().split(/\r?\n/);
  if (lines.length < 2) return [];
  const headers = lines[0].split("|");
  return lines.slice(1).filter(Boolean).map((line) => {
    const values = line.split("|");
    const row: Record<string, unknown> = {};
    headers.forEach((header, index) => {
      const value = values[index] ?? "";
      row[header] = ["synonyms", "zh_alias", "en_alias"].includes(header)
        ? value.split(";").filter(Boolean)
        : value || null;
    });
    return row as TaxonInputRow;
  });
}

function defaultNamingHookTestCases(): NamingHookTestCases {
  const photoCase = (
    name: string,
    input: string,
    info: Partial<Record<
      "family_sci" | "genus_sci" | "species_sci" | "family_zh" | "genus_zh" | "species_zh",
      string
    >>,
    suffix: string,
  ): NamingHookTestCase => ({
    name,
    input,
    expected: {
      kind: "photo_filename",
      output: {
        info: {
          family_sci: null,
          genus_sci: null,
          species_sci: null,
          family_zh: null,
          genus_zh: null,
          species_zh: null,
          ...info,
        },
        suffix,
      },
    },
  });
  const synonymCase = (
    name: string,
    input: string,
    splitName: string,
    authorityYear: string,
  ): NamingHookTestCase => ({
    name,
    input,
    expected: {
      kind: "synonym_authority",
      output: { name: splitName, authority_year: authorityYear },
    },
  });

  return {
    photo_filename: [
      photoCase("family suffix", "Herbertaceae003.jpg", { family_sci: "Herbertaceae" }, "003.jpg"),
      photoCase("genus", "Herbertus005.jpg", { genus_sci: "Herbertus" }, "005.jpg"),
      photoCase("species", "Herbertus dicranus010.jpg", { genus_sci: "Herbertus", species_sci: "Herbertus dicranus" }, "010.jpg"),
      photoCase("quoted internal apostrophe", "Iris 'a'b'030.jpg", { genus_sci: "Iris", species_sci: "Iris 'a'b'" }, "030.jpg"),
      photoCase("curly double quotes", "Iris \u201cBlue\u201d030.jpg", { genus_sci: "Iris", species_sci: "Iris 'Blue'" }, "030.jpg"),
      photoCase("cultivar conversion", "Hosta cv. blue_eyes030.jpg", { genus_sci: "Hosta 'Blue", species_sci: "Hosta 'Blue Eyes'" }, "030.jpg"),
      photoCase("sex marker", "Herbertus dicranusM010.jpg", { genus_sci: "Herbertus", species_sci: "Herbertus dicranus" }, "M010.jpg"),
      photoCase("doubtful marker", "Herbertus dicranusYN010.jpg", { genus_sci: "Herbertus", species_sci: "Herbertus dicranus" }, "YN010.jpg"),
      photoCase("leading hybrid genus", "\u00d7 Gasteraloe030.jpg", { genus_sci: "x Gasteraloe" }, "030.jpg"),
      photoCase("leading hybrid species", "\u00d7 Gasteraloe beguinii030.jpg", { genus_sci: "x Gasteraloe", species_sci: "x Gasteraloe beguinii" }, "030.jpg"),
      photoCase("infix hybrid species", "Pinus X pekinensis030.jpg", { genus_sci: "Pinus x", species_sci: "Pinus x pekinensis" }, "030.jpg"),
      photoCase("family genus species Chinese", "\u9999\u79d1\u9999\u5c5e\u9999\u79cd Canis lupus020.jpg", {
        genus_sci: "Canis",
        species_sci: "Canis lupus",
        family_zh: "\u9999\u79d1",
        genus_zh: "\u9999\u5c5e",
        species_zh: "\u9999\u79cd",
      }, "020.jpg"),
      photoCase("ke ke shu exception", "\u9999\u79d1\u79d1\u5c5e020.jpg", { genus_zh: "\u9999\u79d1\u79d1\u5c5e" }, "020.jpg"),
      photoCase("quoted ASCII inside Chinese species", "\u9999\u79d1\u9999\u5c5e\u9999'abc' Gasteraloe 'Wonder'030.jpg", {
        genus_sci: "Gasteraloe",
        species_sci: "Gasteraloe 'Wonder'",
        family_zh: "\u9999\u79d1",
        genus_zh: "\u9999\u5c5e",
        species_zh: "\u9999'abc'",
      }, "030.jpg"),
      photoCase("parenthesized ASCII inside Chinese species", "\u9999\u79d1\u9999\u5c5e\u9999(abc) Gasteraloe beguinii030.jpg", {
        genus_sci: "Gasteraloe",
        species_sci: "Gasteraloe beguinii",
        family_zh: "\u9999\u79d1",
        genus_zh: "\u9999\u5c5e",
        species_zh: "\u9999(abc)",
      }, "030.jpg"),
    ],
    synonym_authority: [
      synonymCase("parenthesized authority", "Canis lupus (Linnaeus, 1758)", "Canis lupus", "(Linnaeus, 1758)"),
      synonymCase("lowercase authority prefix", "Canis lupus de Silva, 1900", "Canis lupus", "de Silva, 1900"),
      synonymCase("de authority", "\u200cPaidia moabitica de Freina, 2004", "\u200cPaidia moabitica", "de Freina, 2004"),
      synonymCase("apostrophe authority with year", "\u200cSedum eriocarpum subsp. spathulifolium 't Hart, 1995", "\u200cSedum eriocarpum subsp. spathulifolium", "'t Hart, 1995"),
      synonymCase("apostrophe authority", "Sedum fragrans 't Hart", "Sedum fragrans", "'t Hart"),
      synonymCase("von authority", "Hippocampus natalensis von Bonde, 1923", "Hippocampus natalensis", "von Bonde, 1923"),
      synonymCase("van authority", "Hylophilus moxensis van Els, T. Wijpkema, J.T. Wijpkema, Avalos & Montenegro-Avila, 2026", "Hylophilus moxensis", "van Els, T. Wijpkema, J.T. Wijpkema, Avalos & Montenegro-Avila, 2026"),
    ],
  };
}
