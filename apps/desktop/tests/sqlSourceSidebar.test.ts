import assert from "node:assert/strict";
import test from "node:test";
import type { PersistentSqlInput, SqlSourceSchema } from "../src/api/customSql.ts";
import { accessibleSqlSchemas, toggleSqlSourceGroup } from "../src/features/taxonomy/sqlSourceSidebar.ts";

const schema = (alias: string): SqlSourceSchema => ({ alias, objects: [] });

test("opens one SQL source group at a time and allows collapsing it", () => {
  assert.equal(toggleSqlSourceGroup("inputs", "tables"), "tables");
  assert.equal(toggleSqlSourceGroup("tables", "inputs"), "inputs");
  assert.equal(toggleSqlSourceGroup("inputs", "inputs"), null);
});

test("lists database schemas before user input schemas", () => {
  const input: PersistentSqlInput = {
    kind: "sqlite",
    alias: "uploaded",
    original_path: "/tmp/uploaded.db",
    available: true,
    schema: schema("uploaded"),
  };

  assert.deepEqual(
    accessibleSqlSchemas([
      input,
      { ...input, alias: "missing", available: false, schema: schema("missing") },
    ], [schema("main")]).map((item) => item.alias),
    ["main", "uploaded"],
  );
});
