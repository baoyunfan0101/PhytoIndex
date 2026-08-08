import assert from "node:assert/strict";
import test from "node:test";
import { taxonCommonNameLine } from "../src/features/taxonomy/taxonCardNames.ts";

const names = (zhName: string | null, enName: string | null) => ({
  sci_name: "Canis lupus",
  zh_name: zhName,
  en_name: enName,
});

test("joins Chinese and English taxon names with a middle dot", () => {
  assert.equal(taxonCommonNameLine(names("Wolf Chinese", "Wolf")), "Wolf Chinese \u00b7 Wolf");
});

test("omits missing common names and falls back to a dash", () => {
  assert.equal(taxonCommonNameLine(names("Wolf Chinese", null)), "Wolf Chinese");
  assert.equal(taxonCommonNameLine(names(null, "Wolf")), "Wolf");
  assert.equal(taxonCommonNameLine(names(null, null)), "-");
});
