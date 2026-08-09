import assert from "node:assert/strict";
import test from "node:test";
import { formatTaxonomyImportApplyMessage } from "../src/features/taxonomy/taxonomyImportMessages.ts";

test("formats apply success without warnings", () => {
  assert.equal(
    formatTaxonomyImportApplyMessage(undefined),
    "Taxonomy database replaced successfully. Photo mappings are being rebuilt in the background.",
  );
});

test("appends apply warnings", () => {
  assert.equal(
    formatTaxonomyImportApplyMessage(["Cleanup is queued."]),
    "Taxonomy database replaced successfully. Photo mappings are being rebuilt in the background. Cleanup is queued.",
  );
});
