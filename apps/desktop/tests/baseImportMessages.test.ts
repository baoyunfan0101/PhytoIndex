import assert from "node:assert/strict";
import test from "node:test";
import { formatBaseImportApplyMessage } from "../src/features/taxonomy/baseImportMessages.ts";

test("formats apply success without warnings", () => {
  assert.equal(
    formatBaseImportApplyMessage(undefined),
    "Taxonomy database replaced successfully. Photo mappings are being rebuilt in the background.",
  );
});

test("appends apply warnings", () => {
  assert.equal(
    formatBaseImportApplyMessage(["Cleanup is queued."]),
    "Taxonomy database replaced successfully. Photo mappings are being rebuilt in the background. Cleanup is queued.",
  );
});
