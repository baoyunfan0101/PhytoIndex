import assert from "node:assert/strict";
import test from "node:test";
import { defaultGeneralSettings } from "../src/api/generalModel.ts";
import {
  applyTheme,
  normalizeGeneralSettings,
} from "../src/features/settings/generalSettings.ts";

test("normalizes missing and invalid general settings to defaults", () => {
  assert.deepEqual(normalizeGeneralSettings({}), {
    theme: "dark",
    restore_tabs: true,
    recent_searches_limit: 10,
    taxon_tree_name_parts: {
      sci_name: true,
      zh_name: true,
      en_name: true,
    },
  });
  assert.deepEqual(normalizeGeneralSettings({
    theme: "invalid" as never,
    restore_tabs: "yes" as never,
    recent_searches_limit: 51,
  }), defaultGeneralSettings());
});

test("preserves valid general settings", () => {
  assert.deepEqual(normalizeGeneralSettings({
    theme: "light",
    restore_tabs: false,
    recent_searches_limit: 1,
    taxon_tree_name_parts: {
      sci_name: false,
      zh_name: true,
      en_name: false,
    },
  }), {
    theme: "light",
    restore_tabs: false,
    recent_searches_limit: 1,
    taxon_tree_name_parts: {
      sci_name: false,
      zh_name: true,
      en_name: false,
    },
  });
});

test("keeps at least one taxon tree name part visible", () => {
  assert.deepEqual(normalizeGeneralSettings({
    taxon_tree_name_parts: {
      sci_name: false,
      zh_name: false,
      en_name: false,
    },
  }), defaultGeneralSettings());
});

test("applies forced themes and removes the override for system theme", () => {
  const root = { dataset: {} } as Pick<HTMLElement, "dataset">;
  applyTheme("dark", root);
  assert.equal(root.dataset.theme, "dark");
  applyTheme("light", root);
  assert.equal(root.dataset.theme, "light");
  applyTheme("system", root);
  assert.equal(root.dataset.theme, undefined);
});
