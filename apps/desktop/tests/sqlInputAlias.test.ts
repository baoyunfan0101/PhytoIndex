import assert from "node:assert/strict";
import test from "node:test";
import type { PersistentSqlInput } from "../src/api/customSql.ts";
import { sqlInputAliasError, suggestedSqlInputAlias } from "../src/features/taxonomy/sqlInputAlias.ts";

const input = (alias: string): PersistentSqlInput => ({
  kind: "sqlite",
  alias,
  original_path: `/tmp/${alias}.db`,
  available: true,
  schema: { alias, objects: [] },
});

test("suggests an editable SQL alias from the selected filename", () => {
  assert.equal(suggestedSqlInputAlias("/data/Bio Lib-2026.db", []), "Bio_Lib_2026");
  assert.equal(suggestedSqlInputAlias("C:\\data\\123 source.sqlite", []), "_123_source");
});

test("avoids existing and reserved SQL aliases", () => {
  assert.equal(suggestedSqlInputAlias("/data/source.db", [input("source")]), "source_2");
  assert.equal(suggestedSqlInputAlias("/data/main.db", []), "main_2");
  assert.equal(suggestedSqlInputAlias("/data/sql_import.db", []), "sql_import_2");
});

test("validates edited SQL aliases before import", () => {
  assert.equal(sqlInputAliasError("valid_name", []), "");
  assert.match(sqlInputAliasError("bad name", []), /letters, numbers, and underscores/);
  assert.match(sqlInputAliasError("taxonomy", []), /reserved/);
  assert.match(sqlInputAliasError("SOURCE", [input("source")]), /already in use/);
});
