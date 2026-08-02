import assert from "node:assert/strict";
import test from "node:test";
import { resolveSqlWorkbenchLoads } from "../src/features/taxonomy/sqlWorkbenchLoading.ts";

test("keeps loaded SQL when input sources fail", () => {
  const result = resolveSqlWorkbenchLoads(
    { status: "fulfilled", value: "SELECT 1;" },
    { status: "rejected", reason: new Error("inputs unavailable") },
  );

  assert.equal(result.sql, "SELECT 1;");
  assert.equal(result.inputs, undefined);
  assert.equal(result.error, "Input sources: inputs unavailable");
});

test("keeps loaded input sources when SQL fails", () => {
  const inputs = [{ alias: "source" }];
  const result = resolveSqlWorkbenchLoads(
    { status: "rejected", reason: new Error("script unavailable") },
    { status: "fulfilled", value: inputs },
  );

  assert.equal(result.sql, undefined);
  assert.equal(result.inputs, inputs);
  assert.equal(result.error, "SQL script: script unavailable");
});
