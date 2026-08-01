const taxonomyReplacementInvalidKinds = new Set([
  "taxonomy-search",
  "taxon-detail",
  "taxonomy-history",
  "taxon-photos",
  "photo-taxonomy",
  "mapping-editor",
]);

export function dependsOnReplacedTaxonomy(kind: string) {
  return taxonomyReplacementInvalidKinds.has(kind);
}

export function retainTabsAfterTaxonomyReplacement<T extends { kind: string }>(tabs: T[]) {
  return tabs.filter((tab) => !dependsOnReplacedTaxonomy(tab.kind));
}
