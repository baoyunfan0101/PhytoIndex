import assert from "node:assert/strict";
import test from "node:test";
import {
  defaultPhotoFilenameFormatSettings,
  normalizePhotoFilenameFormatSettings,
  photoFilenameFormatFields,
} from "../src/features/settings/namingSettings.ts";

test("exposes all six photo filename format fields in display order", () => {
  assert.deepEqual(photoFilenameFormatFields, [
    { field: "family_zh", label: "Family Chinese" },
    { field: "family_sci", label: "Family scientific" },
    { field: "genus_zh", label: "Genus Chinese" },
    { field: "genus_sci", label: "Genus scientific" },
    { field: "species_zh", label: "Species Chinese" },
    { field: "species_sci", label: "Species scientific" },
  ]);
});

test("keeps a usable default format when loaded data is incomplete", () => {
  assert.deepEqual(normalizePhotoFilenameFormatSettings({ family_zh: true }), {
    ...defaultPhotoFilenameFormatSettings(),
    family_zh: true,
  });
  assert.deepEqual(normalizePhotoFilenameFormatSettings({ species_sci: "yes" }),
    defaultPhotoFilenameFormatSettings());
});
