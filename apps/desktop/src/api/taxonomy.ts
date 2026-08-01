import { call } from "./client";
import type { Page } from "./common";
import { demoPhotos, type Photo } from "./photos";

export type TaxonRank = "kingdom" | "order" | "family" | "genus" | "species";
export type TaxonDisplayNames = { sci_name: string | null; zh_name: string | null; en_name: string | null };
export type TaxonBreadcrumbItem = { taxon_id: number; rank: TaxonRank; names: TaxonDisplayNames };
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
export type TaxonNameMatch = { name_id: number; name_type: string; name: string };
export type TaxonSearchResult = { summary: TaxonSummary; detail: TaxonDetail; matches: TaxonNameMatch[] };
export type TaxonSuggestion = TaxonBreadcrumbItem & { matches: TaxonNameMatch[] };
export type TaxonChild = TaxonBreadcrumbItem;
export type TaxonDetailNode = { summary: TaxonSummary; detail: TaxonDetail; children: Page<TaxonChild> };

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
export type TaxonomyPreviewResult = { delimiter: string; encoding: string; rows: TaxonRowOutcome[] };
export type TaxonomyOperationResult = TaxonomyPreviewResult & {
  operation_id: number;
  total_rows: number;
  succeeded_rows: number;
  failed_rows: number;
};

export const demoTaxa: TaxonSearchResult[] = [
  demoTaxon(1001, "Canis lupus", "Wolf"),
  demoTaxon(1002, "Panthera leo", "Lion"),
  demoTaxon(1003, "Vulpes vulpes", "Red fox"),
  demoTaxon(1004, "Ursus arctos", "Brown bear"),
];

function demoTaxon(taxonId: number, scientific: string, english: string): TaxonSearchResult {
  const sciName = { name_id: taxonId * 10, name: scientific, authority_year: null, source: "Demo" };
  const names = { sci_name: scientific, zh_name: null, en_name: english };
  return {
    summary: {
      taxon_id: taxonId,
      rank: "species",
      breadcrumb: [
        { taxon_id: 1, rank: "kingdom", names: { sci_name: "Animalia", zh_name: null, en_name: "Animals" } },
        { taxon_id: 2, rank: "order", names: { sci_name: "Carnivora", zh_name: null, en_name: "Carnivorans" } },
      ],
      names,
    },
    detail: {
      taxon_id: taxonId,
      rank: "species",
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

export function displayTaxon(summary: Pick<TaxonSummary, "taxon_id" | "names">): string {
  return summary.names.sci_name ?? summary.names.zh_name ?? summary.names.en_name ?? `Taxon ${summary.taxon_id}`;
}

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
    items: demoPhotos.slice((taxonId % 4) * 8, (taxonId % 4) * 8 + limit),
    next_cursor: null,
  }));

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
