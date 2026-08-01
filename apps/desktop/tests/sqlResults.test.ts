import assert from "node:assert/strict";
import test from "node:test";
import { canExportFullQuery } from "../src/features/taxonomy/sqlResults.ts";

function preview(truncated: boolean, affectedRows: number | null) {
  return {
    result_sets: [{
      statement_index: 1,
      columns: [],
      rows: [],
      truncated,
    }],
    messages: [{
      statement_index: 1,
      affected_rows: affectedRows,
      message: "complete",
    }],
  };
}

test("offers full export for a truncated read-only query", () => {
  assert.equal(canExportFullQuery(preview(true, null)), true);
});

test("does not offer full export for truncated mutation returning rows", () => {
  assert.equal(canExportFullQuery(preview(true, 100)), false);
});

test("does not offer full export when a read-only preview is complete", () => {
  assert.equal(canExportFullQuery(preview(false, null)), false);
});
