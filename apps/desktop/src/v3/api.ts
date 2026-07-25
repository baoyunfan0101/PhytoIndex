import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export type Photo = {
  photo_id: number;
  directory_id: number;
  relative_path: string;
  filename: string;
  file_size: number;
  modified_at_ns: number;
  thumbnail_path: string | null;
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

export type Page<T> = {
  items: T[];
  next_cursor: string | null;
};

export type PhotoLibrary = {
  root_path: string;
  root_directory_id: number;
};

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

export type PhotoOperation = {
  operation_id: number;
  batch_id: number;
  row_number: number;
  status: "applied" | "reverted";
  photo_id: number;
  directory_relative_path: string;
  old_filename: string;
  new_filename: string;
  applied_at: string;
  reverted_at: string | null;
};

export type MappingMetadata = {
  mapped_photo_count: number;
  unmatched_photo_count: number;
  ambiguous_photo_count: number;
  processing_photo_count: number;
  mapping_taxa_count: number;
};

export type PhotoTaxonStatus =
  | "matched"
  | "unmatched"
  | "ambiguous"
  | "processing"
  | "stale";

export type MappingListStatus = PhotoTaxonStatus | "unmapped";

export type PhotoTaxonMapping = {
  photo_id: number;
  taxon_id: number | null;
  status: PhotoTaxonStatus;
};

export type PhotoMappingListItem = {
  photo: Photo;
  mapping: PhotoTaxonMapping | null;
};

export type TaxonDisplayNames = {
  scientific: string | null;
  english: string | null;
  chinese: string | null;
};

export type TaxonSummary = {
  taxon_id: number;
  rank: string;
  breadcrumb: Array<{
    taxon_id: number;
    rank: string;
    names: TaxonDisplayNames;
  }>;
  names: TaxonDisplayNames;
};

export type TaxonSearchResult = {
  summary: TaxonSummary;
  detail: {
    taxon_id: number;
    rank: string;
    parent_taxon_id: number | null;
    geological_range: string | null;
    names: {
      scientific: Array<{ name: string; is_accepted: boolean }>;
      english: Array<{ name: string; is_accepted: boolean }>;
      chinese: Array<{ name: string; is_accepted: boolean }>;
    };
    identifiers: Array<{ source: string; external_id: string }>;
  };
  matches: Array<{
    name_id: number;
    name_kind: string;
    name: string;
    is_accepted: boolean;
  }>;
};

export type PhotoTaxonCandidate = {
  summary: TaxonSummary;
  matched_names: Array<{
    name_id: number;
    name_kind: string;
    name: string;
    is_accepted: boolean;
  }>;
  accepted_names: TaxonDisplayNames;
};

export type PhotoTaxonMatch = {
  mapping: PhotoTaxonMapping;
  candidates: PhotoTaxonCandidate[];
};

export type TaxonomyOperation = {
  operation_id: number;
  batch_id: number;
  row_number: number;
  status: "applied" | "reverted";
  changeset_size: number;
  applied_at: string;
  reverted_at: string | null;
};

export function getPhotoLibrary(): Promise<PhotoLibrary | null> {
  return invoke("get_photo_library");
}

export function getPhotoLibraryCount(): Promise<number> {
  return invoke("get_photo_library_count");
}

export function openPhotoLibrary(root: string): Promise<PhotoLibrary> {
  return invoke("open_photo_library", { root });
}

export function browsePhotoDirectory(
  directoryId: number,
  cursor: string | null = null,
  limit = 300,
): Promise<Page<PhotoDirectoryItem>> {
  return invoke("browse_photo_directory", { directoryId, cursor, limit });
}

export function getPhotoDirectoryCounts(directoryId: number): Promise<DirectoryEntryCounts> {
  return invoke("get_photo_directory_counts", { directoryId });
}

export function refreshPhotoDirectory(directoryId: number): Promise<{ operation: OperationState }> {
  return invoke("refresh_photo_directory", { directoryId });
}

export function getOperationsStatus(): Promise<OperationsStatus> {
  return invoke("get_operations_status");
}

export function listPhotoOperations(limit = 100): Promise<Page<PhotoOperation>> {
  return invoke("list_photo_operations", { cursor: null, limit });
}

export function getMappingMetadata(): Promise<MappingMetadata> {
  return invoke("get_mapping_metadata");
}

export function listPhotosByMappingStatus(
  status: MappingListStatus,
  limit = 200,
): Promise<Page<PhotoMappingListItem>> {
  return invoke("list_photos_by_mapping_status", { status, cursor: null, limit });
}

export function getPhotoTaxonMatch(photoId: number): Promise<PhotoTaxonMatch> {
  return invoke("get_photo_taxon_match", { photoId });
}

export function startPhotoMapping(): Promise<{ operation: OperationState }> {
  return invoke("start_photo_mapping");
}

export function searchTaxa(query: string, limit = 100): Promise<TaxonSearchResult[]> {
  return invoke("search_taxa", { query, limit });
}

export function listTaxonomyOperations(limit = 100): Promise<Page<TaxonomyOperation>> {
  return invoke("list_taxonomy_operations", { cursor: null, limit });
}

export async function selectPhotoDirectory(): Promise<string | null> {
  const selected = await open({ directory: true, multiple: false });
  return typeof selected === "string" ? selected : null;
}

export function photoUrl(photo: Photo, thumbnail = false): string {
  const resource = thumbnail ? "thumbnail" : "photo";
  const version = `${photo.modified_at_ns}:${photo.file_size}`;
  return `${convertFileSrc(`${resource}/${photo.photo_id}`, "phytoindex")}?v=${version}`;
}

export async function waitForOperation(
  module: string,
  taskId: string | null,
  onProgress?: (operation: OperationState) => void,
): Promise<OperationState> {
  if (!taskId) {
    throw new Error("Operation did not return a task id");
  }
  while (true) {
    const operation = (await getOperationsStatus())[module];
    if (!operation || operation.task_id !== taskId) {
      throw new Error(`${module} operation is no longer available`);
    }
    onProgress?.(operation);
    if (!operation.running) {
      if (operation.error) {
        throw new Error(operation.error);
      }
      return operation;
    }
    await new Promise((resolve) => window.setTimeout(resolve, 250));
  }
}
