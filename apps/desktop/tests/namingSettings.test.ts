import assert from "node:assert/strict";
import test from "node:test";
import {
  defaultPhotoFilenameFormatSettings,
  normalizePhotoFilenameFormatSettings,
  photoFilenameFormatChanged,
  photoFilenameFormatFields,
  photoNamePriorityChanged,
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

test("detects mapping priority changes independently from filename format changes", () => {
  const priority = ["species_sci", "species_zh", "genus_sci", "genus_zh", "family_sci", "family_zh"];
  assert.equal(photoNamePriorityChanged(priority, [...priority]), false);
  assert.equal(photoNamePriorityChanged(priority, [priority[1], priority[0], ...priority.slice(2)]), true);

  const savedFormat = defaultPhotoFilenameFormatSettings();
  assert.equal(photoFilenameFormatChanged(savedFormat, { ...savedFormat }), false);
  assert.equal(photoFilenameFormatChanged(savedFormat, { ...savedFormat, genus_sci: true }), true);
});
