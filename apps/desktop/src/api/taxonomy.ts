import { call } from "./client";
import type { Page } from "./common";
import { demoPhotos, type Photo } from "./photos";

export type TaxonRank = "kingdom" | "order" | "family" | "genus" | "species";
export type TaxonomyNameType =
  | "sci_name"
  | "synonym"
  | "zh_name"
  | "zh_alias"
  | "en_name"
  | "en_alias";
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
  breadcrumb: TaxonBreadcrumbItem[];
  geological_range: string | null;
  names: {
    sci_name: TaxonNameDetail | null;
    synonyms: TaxonNameDetail[];
    zh_name: TaxonNameDetail | null;
    zh_aliases: TaxonNameDetail[];
    en_name: TaxonNameDetail | null;
    en_aliases: TaxonNameDetail[];
  };
};
export type TaxonNameMatch = { name_id: number; name_type: TaxonomyNameType; name: string };
export type TaxonSearchResult = {
  taxon_id: number;
  rank: TaxonRank;
  names: TaxonDisplayNames;
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

type DemoTaxonDefinition = {
  taxon_id: number;
  parent_taxon_id: number | null;
  rank: TaxonRank;
  scientific: string;
  english: string;
};

const demoTaxonomy: DemoTaxonDefinition[] = [
  { taxon_id: 1, parent_taxon_id: null, rank: "kingdom", scientific: "Animalia", english: "Animals" },
  { taxon_id: 2, parent_taxon_id: 1, rank: "order", scientific: "Carnivora", english: "Carnivorans" },
  { taxon_id: 3, parent_taxon_id: 2, rank: "family", scientific: "Canidae", english: "Canids" },
  { taxon_id: 4, parent_taxon_id: 3, rank: "genus", scientific: "Canis", english: "Canis" },
  { taxon_id: 5, parent_taxon_id: 2, rank: "family", scientific: "Felidae", english: "Felids" },
  { taxon_id: 6, parent_taxon_id: 5, rank: "genus", scientific: "Panthera", english: "Panthera" },
  { taxon_id: 7, parent_taxon_id: 3, rank: "genus", scientific: "Vulpes", english: "Vulpes" },
  { taxon_id: 8, parent_taxon_id: 2, rank: "family", scientific: "Ursidae", english: "Bears" },
  { taxon_id: 9, parent_taxon_id: 8, rank: "genus", scientific: "Ursus", english: "Ursus" },
  { taxon_id: 1001, parent_taxon_id: 4, rank: "species", scientific: "Canis lupus", english: "Wolf" },
  { taxon_id: 1002, parent_taxon_id: 6, rank: "species", scientific: "Panthera leo", english: "Lion" },
  { taxon_id: 1003, parent_taxon_id: 7, rank: "species", scientific: "Vulpes vulpes", english: "Red fox" },
  { taxon_id: 1004, parent_taxon_id: 9, rank: "species", scientific: "Ursus arctos", english: "Brown bear" },
];

const demoTaxonomyById = new Map(demoTaxonomy.map((taxon) => [taxon.taxon_id, taxon]));

export const demoTaxa: TaxonSearchResult[] = demoTaxonomy
  .filter((taxon) => taxon.rank === "species")
  .map(demoTaxon);

function demoTaxon(taxon: DemoTaxonDefinition): TaxonSearchResult {
  return {
    taxon_id: taxon.taxon_id,
    rank: taxon.rank,
    names: { sci_name: taxon.scientific, zh_name: null, en_name: taxon.english },
    matches: [{ name_id: taxon.taxon_id * 10, name_type: "sci_name", name: taxon.scientific }],
  };
}

function demoBreadcrumb(taxon: DemoTaxonDefinition): TaxonBreadcrumbItem[] {
  const breadcrumb: TaxonBreadcrumbItem[] = [];
  let parentTaxonId = taxon.parent_taxon_id;
  while (parentTaxonId !== null) {
    const parent = demoTaxonomyById.get(parentTaxonId);
    if (!parent) throw new Error(`taxon ${taxon.taxon_id} references missing parent ${parentTaxonId}`);
    breadcrumb.push({
      taxon_id: parent.taxon_id,
      rank: parent.rank,
      names: { sci_name: parent.scientific, zh_name: null, en_name: parent.english },
    });
    parentTaxonId = parent.parent_taxon_id;
  }
  return breadcrumb.reverse();
}

const demoTaxonDetails = new Map<number, TaxonDetail>(demoTaxonomy.map((taxon) => [
  taxon.taxon_id,
  {
    taxon_id: taxon.taxon_id,
    rank: taxon.rank,
    parent_taxon_id: taxon.parent_taxon_id,
    breadcrumb: demoBreadcrumb(taxon),
    geological_range: "Recent",
    names: {
      sci_name: {
        name_id: taxon.taxon_id * 10,
        name: taxon.scientific,
        authority_year: null,
        source: "Demo",
      },
      synonyms: taxon.taxon_id === 1001 ? [{
        name_id: taxon.taxon_id * 10 + 2,
        name: "Canis lycaon",
        authority_year: "Schreber, 1775",
        source: "Demo synonym index",
      }] : [],
      zh_name: null,
      zh_aliases: [],
      en_name: {
        name_id: taxon.taxon_id * 10 + 1,
        name: taxon.english,
        authority_year: null,
        source: "Demo",
      },
      en_aliases: taxon.taxon_id === 1003 ? [{
        name_id: taxon.taxon_id * 10 + 2,
        name: "Common fox",
        authority_year: null,
        source: "Demo vernacular index",
      }] : [],
    },
  },
]));

const demoChildrenByParent = new Map<number, TaxonChild[]>();
for (const taxon of demoTaxonomy) {
  if (taxon.parent_taxon_id === null) continue;
  const children = demoChildrenByParent.get(taxon.parent_taxon_id) ?? [];
  children.push({
    taxon_id: taxon.taxon_id,
    rank: taxon.rank,
    names: { sci_name: taxon.scientific, zh_name: null, en_name: taxon.english },
  });
  demoChildrenByParent.set(taxon.parent_taxon_id, children);
}
const demoRankOrder: Record<TaxonRank, number> = { kingdom: 1, order: 2, family: 3, genus: 4, species: 5 };
for (const children of demoChildrenByParent.values()) {
  children.sort((left, right) => demoRankOrder[left.rank] - demoRankOrder[right.rank] || left.taxon_id - right.taxon_id);
}

export function displayTaxon(summary: { taxon_id: number; names: TaxonDisplayNames }): string {
  return summary.names.sci_name ?? summary.names.zh_name ?? summary.names.en_name ?? `Taxon ${summary.taxon_id}`;
}

export function displayTaxonDetail(detail: Pick<TaxonDetail, "taxon_id" | "names">): string {
  return detail.names.sci_name?.name
    ?? detail.names.zh_name?.name
    ?? detail.names.en_name?.name
    ?? `Taxon ${detail.taxon_id}`;
}

export function demoTaxonSummary(taxon: TaxonSearchResult): TaxonSummary {
  return {
    taxon_id: taxon.taxon_id,
    rank: taxon.rank,
    breadcrumb: demoTaxonDetails.get(taxon.taxon_id)?.breadcrumb ?? [],
    names: taxon.names,
  };
}

export const searchTaxa = (query: string, limit = 80) =>
  call<TaxonSearchResult[]>("search_taxa", { query, limit }, () =>
    demoTaxa.flatMap((taxon) => {
      const result = demoSearchResult(taxon, query);
      return result ? [result] : [];
    }).slice(0, limit));
export const suggestTaxa = (query: string, limit = 10) =>
  call<TaxonSuggestion[]>("suggest_taxa", { query, limit }, () =>
    demoTaxa
      .flatMap((taxon) => {
        const result = demoSearchResult(taxon, query);
        return result ? [result] : [];
      })
      .slice(0, limit)
      .map((taxon) => ({ ...taxon })));
export const suggestPhotoTaxa = (query: string, limit = 10) =>
  call<TaxonSuggestion[]>("suggest_photo_taxa", { query, limit }, () => suggestTaxa(query, limit));
export const getTaxonDetail = (taxonId: number) =>
  call<TaxonDetail>("get_taxon_detail", { taxonId }, () => {
    const detail = demoTaxonDetails.get(taxonId);
    if (!detail) throw new Error(`taxon ${taxonId} not found`);
    return detail;
  });
export const listTaxonChildren = (taxonId: number, cursor: string | null = null, limit = 80) =>
  call<Page<TaxonChild>>("list_taxon_children", { taxonId, cursor, limit }, () => {
    const cursorPrefix = `demo-taxonomy-children:${taxonId}:`;
    const cursorOffset = cursor && cursor.length > 0 && cursor.startsWith(cursorPrefix)
      ? cursor.slice(cursorPrefix.length)
      : null;
    const offset = cursorOffset !== null && /^\d+$/.test(cursorOffset)
      ? Number(cursorOffset)
      : cursor ? Number.NaN : 0;
    if (!Number.isSafeInteger(offset) || offset < 0) throw new Error("invalid taxonomy cursor");
    const pageLimit = Math.min(Math.max(Math.trunc(limit), 1), 500);
    const children = demoChildrenByParent.get(taxonId) ?? [];
    const items = children.slice(offset, offset + pageLimit);
    const nextOffset = offset + items.length;
    return {
      items,
      next_cursor: nextOffset < children.length ? `${cursorPrefix}${nextOffset}` : null,
    };
  });
export const listTaxonPhotos = (taxonId: number, cursor: string | null = null, limit = 80) =>
  call<Page<Photo>>("list_taxon_photos", { taxonId, cursor, limit }, () => ({
    items: demoPhotos.slice((taxonId % 4) * 8, (taxonId % 4) * 8 + limit),
    next_cursor: null,
  }));

function demoSearchResult(taxon: TaxonSearchResult, query: string): TaxonSearchResult | null {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return null;
  const detail = demoTaxonDetails.get(taxon.taxon_id);
  if (!detail) return null;
  const candidates: Array<[TaxonomyNameType, TaxonNameDetail]> = [];
  if (detail.names.sci_name) candidates.push(["sci_name", detail.names.sci_name]);
  detail.names.synonyms.forEach((name) => candidates.push(["synonym", name]));
  if (detail.names.zh_name) candidates.push(["zh_name", detail.names.zh_name]);
  detail.names.zh_aliases.forEach((name) => candidates.push(["zh_alias", name]));
  if (detail.names.en_name) candidates.push(["en_name", detail.names.en_name]);
  detail.names.en_aliases.forEach((name) => candidates.push(["en_alias", name]));
  const matches = candidates
    .filter(([, name]) => name.name.toLowerCase().includes(normalized))
    .map(([nameType, name]) => ({
      name_id: name.name_id,
      name_type: nameType,
      name: name.name,
    }));
  return matches.length > 0 ? { ...taxon, matches } : null;
}

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
