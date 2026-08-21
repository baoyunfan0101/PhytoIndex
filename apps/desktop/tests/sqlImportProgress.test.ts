import assert from "node:assert/strict";
import test from "node:test";
import { describeSqlImportProgress, formatElapsed } from "../src/features/taxonomy/sqlImportProgress.ts";

test("describes SQL statement progress without inventing a percentage", () => {
  assert.equal(describeSqlImportProgress({
    stage: "executing_sql",
    current: 2,
    total: 7,
    unit: "statements",
  }), "Executing SQL: 2 / 7 statements");
});

test("describes known row counts and phase-only progress", () => {
  assert.equal(describeSqlImportProgress({
    stage: "normalizing_names",
    current: 120000,
    total: 850000,
    unit: "names",
  }), "Normalizing names: 120,000 / 850,000 names");
  assert.equal(describeSqlImportProgress({
    stage: "validating_taxonomy",
    current: null,
    total: null,
    unit: null,
  }), "Validating taxonomy");
});

test("formats elapsed duration", () => {
  assert.equal(formatElapsed(5900), "0:05");
  assert.equal(formatElapsed(65000), "1:05");
  assert.equal(formatElapsed(3661000), "1:01:01");
});
