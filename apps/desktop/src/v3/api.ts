import { Channel, convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import defaultPhotoFilenameHook from "../../../../crates/phytoindex-core/src/naming/templates/photo_filename.rhai?raw";
import defaultSynonymAuthorityHook from "../../../../crates/phytoindex-core/src/naming/templates/synonym_authority.rhai?raw";

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

export type PhotoTaxonMapping = {
  photo_id: number;
  taxon_id: number | null;
  status: PhotoTaxonStatus;
};

export type PhotoMappingListItem = {
  photo: Photo;
  mapping: PhotoTaxonMapping;
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

export type PhotoTaxonMatch = {
  mapping: PhotoTaxonMapping;
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

export type PhotoOperation = {
  operation_id: number;
  source: "manual_rename" | "taxon_rename" | "taxon_selection_rename";
  root_path: string;
  input: Array<{
    row_number: number;
    photo_id: number;
    requested_filename: string | null;
  }>;
  items: Array<{
    row_number: number;
    photo_id: number;
    directory_relative_path: string;
    old_filename: string;
    new_filename: string;
  }>;
  applied_at: string;
};

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

export type TaxonomyOperation = {
  operation_id: number;
  source: "formatted_update";
  input: TaxonInputRow[];
  result: TaxonomyOperationResult;
  changeset_size: number;
  applied_at: string;
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

let demoMappings = new Map<number, PhotoTaxonMapping>(
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
  return `${convertFileSrc(`${resource}/${photo.photo_id}`, "phytoindex")}?v=${photo.modified_at_ns}:${photo.file_size}`;
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

export async function selectPhotoDirectory(): Promise<string | null> {
  if (!desktopRuntime) {
    return "/Demo/Vividarium Photos";
  }
  const selected = await open({ directory: true, multiple: false });
  return typeof selected === "string" ? selected : null;
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
  call<PhotoTaxonMapping | null>("get_photo_mapping", { photoId }, () => demoMappings.get(photoId) ?? null);

export const getPhotoTaxonMatch = (photoId: number) =>
  call<PhotoTaxonMatch>("get_photo_taxon_match", { photoId }, () => {
    const mapping = demoMappings.get(photoId) ?? { photo_id: photoId, taxon_id: null, status: "unmatched" };
    return {
      mapping,
      candidates: mapping.status === "ambiguous"
        ? demoTaxa.slice(0, 3).map((taxon) => ({
            summary: taxon.summary,
            matched_names: taxon.matches,
            accepted_names: taxon.summary.names,
          }))
        : [],
    };
  });

export const clearPhotoMapping = (photoId: number) =>
  call<PhotoTaxonMapping>("clear_photo_mapping", { photoId }, () => {
    const mapping: PhotoTaxonMapping = { photo_id: photoId, taxon_id: null, status: "unmatched" };
    demoMappings.set(photoId, mapping);
    return mapping;
  });

export const setPhotoMapping = (photoId: number, taxonId: number) =>
  call<PhotoTaxonMapping>("set_photo_mapping", { photoId, taxonId }, () => {
    const mapping: PhotoTaxonMapping = { photo_id: photoId, taxon_id: taxonId, status: "matched" };
    demoMappings.set(photoId, mapping);
    return mapping;
  });

export const selectPhotoTaxon = (photoId: number, taxonId: number) =>
  call<PhotoTaxonMapping>("select_photo_taxon", { photoId, taxonId }, () => setPhotoMapping(photoId, taxonId));

export const remapPhoto = (photoId: number) =>
  call<PhotoTaxonMatch>("remap_photo", { photoId }, () => getPhotoTaxonMatch(photoId));

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

export const listPhotoOperations = (cursor: string | null = null, limit = 80) =>
  call<Page<PhotoOperation>>("list_photo_operations", { cursor, limit }, () => ({
    items: [1, 2, 3].map((id) => ({
      operation_id: id,
      source: id === 1 ? "manual_rename" : "taxon_rename",
      root_path: "/Demo/Vividarium Photos",
      input: [{ row_number: 1, photo_id: id, requested_filename: id === 1 ? `renamed_${id}.jpg` : null }],
      items: [{
        row_number: 1,
        photo_id: id,
        directory_relative_path: "Mammalia",
        old_filename: `before_${id}.jpg`,
        new_filename: `after_${id}.jpg`,
      }],
      applied_at: `2026-07-${20 + id} 10:30:00`,
    })),
    next_cursor: null,
  }));

export const revertPhotoOperation = (operationId: number) =>
  call<void>("revert_photo_operation", { operationId }, () => undefined);
export const exportPhotoOperationCsv = (operationId: number) =>
  call<string>("export_photo_operation_csv", { operationId }, () => "operation_id|old_filename|new_filename\n1|before.jpg|after.jpg\n");
export const exportAllPhotoOperationsCsv = () =>
  call<string>("export_all_photo_operations_csv", undefined, () => "operation_id|old_filename|new_filename\n1|before.jpg|after.jpg\n");

export const listTaxonomyOperations = (cursor: string | null = null, limit = 80) =>
  call<Page<TaxonomyOperation>>("list_taxonomy_operations", { cursor, limit }, () => ({
    items: [1, 2].map((id) => ({
      operation_id: id,
      source: "formatted_update",
      input: [{ species: id === 1 ? "Canis lupus" : "Panthera leo" }],
      result: {
        operation_id: id,
        total_rows: 1,
        succeeded_rows: 1,
        failed_rows: 0,
        delimiter: "|",
        encoding: "UTF-8",
        rows: [],
      },
      changeset_size: 420 + id,
      applied_at: `2026-07-${22 + id} 14:10:00`,
    })),
    next_cursor: null,
  }));

export const revertTaxonomyOperation = (operationId: number) =>
  call<void>("revert_taxonomy_operation", { operationId }, () => undefined);
export const exportTaxonomyOperationCsv = (operationId: number) =>
  call<string>("export_taxonomy_operation_csv", { operationId }, () => `kingdom|order|family|genus|species|authority_year|synonyms|zh_name|zh_alias|en_name|en_alias|geological_range|source\n||||Canis lupus||||||||\n`);
export const exportAllTaxonomyOperationsCsv = () =>
  call<string>("export_all_taxonomy_operations_csv", undefined, () => `kingdom|order|family|genus|species|authority_year|synonyms|zh_name|zh_alias|en_name|en_alias|geological_range|source\n||||Canis lupus||||||||\n`);

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
export const executeCustomTaxonomySql = (sql: string, input: { columns: string[]; rows: string[][] } | null) =>
  call<{ changeset_size: number }>("execute_custom_taxonomy_sql", { sql, input }, () => ({ changeset_size: sql.length + (input?.rows.length ?? 0) }));

export const getMapSettings = () =>
  call<MapSettings>("get_map_settings", undefined, () => ({ provider: "osm", tianditu_token: null }));
export const setMapSettings = (settings: MapSettings) =>
  call<MapSettings>("set_map_settings", { settings }, () => settings);
export const listMapPhotos = (bounds: MapBounds | null = null, cursor: string | null = null, limit = 500) =>
  call<Page<MapPhoto>>("list_map_photos", { bounds, cursor, limit }, () => ({
    items: demoPhotos.slice(0, 24).map((photo, index) => ({
      photo,
      longitude: 116.25 + (index % 8) * 0.08,
      latitude: 39.75 + Math.floor(index / 8) * 0.08,
    })),
    next_cursor: null,
  }));

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
