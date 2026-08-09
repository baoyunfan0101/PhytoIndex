import assert from "node:assert/strict";
import test from "node:test";
import type { SqlSourceSchema } from "../src/api/customSql.ts";
import { internalDatabaseSchemas, toggleSqlSourceGroup } from "../src/features/taxonomy/sqlSourceSidebar.ts";

const schema = (alias: string): SqlSourceSchema => ({ alias, objects: [] });

test("opens one SQL source group at a time and allows collapsing it", () => {
  assert.equal(toggleSqlSourceGroup("inputs", "tables"), "tables");
  assert.equal(toggleSqlSourceGroup("tables", "inputs"), "inputs");
  assert.equal(toggleSqlSourceGroup("inputs", "inputs"), null);
});

test("lists only internal database schemas as accessible tables", () => {
  assert.deepEqual(
    internalDatabaseSchemas([schema("main")]).map((item) => item.alias),
    ["main"],
  );
});
