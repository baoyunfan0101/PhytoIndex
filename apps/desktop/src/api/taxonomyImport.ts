import { call } from "./client";

export type TaxonomyImportMetadata = {
  source_path: string;
  taxa_count: number;
  taxon_names_count: number;
  imported_at: string;
};

export type TaxonomyImportResult = {
  metadata: TaxonomyImportMetadata;
  warnings: string[];
};

export const getTaxonomyImportMetadata = () =>
  call<TaxonomyImportMetadata | null>("get_taxonomy_import_metadata", undefined, () => null);
