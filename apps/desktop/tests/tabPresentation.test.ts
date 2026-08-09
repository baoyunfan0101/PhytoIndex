import assert from "node:assert/strict";
import test from "node:test";
import type { AppTab } from "../src/app/workspaceState";
import { getTabName } from "../src/app/tabPresentation.ts";

function tab(kind: AppTab["kind"], title = "Legacy title"): AppTab {
  return { id: kind, kind, title };
}

test("uses the canonical name for fixed tabs", () => {
  const expected: Array<[AppTab["kind"], string]> = [
    ["folders", "Folders"],
    ["photo-taxonomy", "Taxon Tree"],
    ["map", "Map"],
    ["photo-history", "Rename History"],
    ["mapping", "Mapping"],
    ["taxonomy-search", "Taxonomy Search"],
    ["formatted-update", "Formatted Update"],
    ["custom-sql", "Custom SQL"],
    ["taxonomy-history", "Taxonomy History"],
    ["settings", "Settings"],
  ];

  for (const [kind, name] of expected) assert.equal(getTabName(tab(kind)), name);
});

test("builds dynamic tab names from their current data", () => {
  assert.equal(getTabName({ ...tab("search-photos"), query: "beetles" }), "Search: beetles");
  assert.equal(getTabName(tab("taxon-photos", "Coleoptera")), "Photos: Coleoptera");
  assert.equal(getTabName(tab("taxon-detail", "Coleoptera")), "Taxon: Coleoptera");
  assert.equal(getTabName({ ...tab("photo-detail"), photo: { filename: "sample.jpg" } as AppTab["photo"] }), "Photo: sample.jpg");
  assert.equal(getTabName({ ...tab("mapping-editor"), photo: { filename: "sample.jpg" } as AppTab["photo"] }), "Mapping: sample.jpg");
});

test("does not duplicate a prefix from an older saved workspace", () => {
  assert.equal(getTabName(tab("taxon-photos", "Photos: Coleoptera")), "Photos: Coleoptera");
  assert.equal(getTabName(tab("taxon-detail", "Taxon: Coleoptera")), "Taxon: Coleoptera");
});
