import type { AppTab } from "./workspaceState";

const fixedTabNames: Partial<Record<AppTab["kind"], string>> = {
  folders: "Folders",
  "photo-taxonomy": "Taxon Tree",
  map: "Map",
  "photo-history": "Rename History",
  mapping: "Mapping",
  "taxonomy-search": "Taxonomy Search",
  "formatted-update": "Formatted Update",
  "custom-sql": "Custom SQL",
  "taxonomy-history": "Taxonomy History",
  settings: "Settings",
};

function withPrefix(prefix: string, value: string): string {
  const trimmed = value.trim();
  const existingPrefix = `${prefix}:`;
  const unprefixed = trimmed.toLocaleLowerCase().startsWith(existingPrefix.toLocaleLowerCase())
    ? trimmed.slice(existingPrefix.length).trim()
    : trimmed;
  return `${prefix}: ${unprefixed}`;
}

export function getTabName(tab: AppTab): string {
  const fixedName = fixedTabNames[tab.kind];
  if (fixedName) return fixedName;

  switch (tab.kind) {
    case "search-photos":
      return withPrefix("Search", tab.query ?? tab.title);
    case "taxon-photos":
      return withPrefix("Photos", tab.title);
    case "photo-detail":
      return withPrefix("Photo", tab.photo?.filename ?? tab.title);
    case "mapping-editor":
      return withPrefix("Mapping", tab.photo?.filename ?? tab.title);
    case "taxon-detail":
      return withPrefix("Taxon", tab.title);
    default:
      return tab.title;
  }
}
