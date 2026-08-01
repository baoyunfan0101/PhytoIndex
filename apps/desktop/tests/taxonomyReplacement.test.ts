import assert from "node:assert/strict";
import test from "node:test";
import { retainTabsAfterTaxonomyReplacement } from "../src/app/taxonomyReplacement.ts";

test("taxonomy replacement closes identity-bound tabs and preserves global work", () => {
  const tabs = [
    "taxonomy-search",
    "taxon-detail",
    "taxonomy-history",
    "taxon-photos",
    "photo-taxonomy",
    "mapping-editor",
    "folders",
    "photo-history",
    "settings",
    "custom-sql",
    "formatted-update",
    "mapping",
    "photo-detail",
    "search-photos",
    "map",
  ].map((kind) => ({ id: kind, kind }));
  assert.deepEqual(
    retainTabsAfterTaxonomyReplacement(tabs).map((tab) => tab.kind),
    [
      "folders",
      "photo-history",
      "settings",
      "custom-sql",
      "formatted-update",
      "mapping",
      "photo-detail",
      "search-photos",
      "map",
    ],
  );
});
