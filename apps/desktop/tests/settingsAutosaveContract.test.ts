import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { renameFromTaxonomyStatus } from "../src/features/photos/photoRenameStatus.ts";
import { formatPhotoRenameSummary } from "../src/features/photos/photoOperation.ts";

function source(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("ordinary Settings sections autosave through the tab status bar", () => {
  const settings = source("../src/features/settings/SettingsView.tsx");
  const shell = source("../src/app/DesktopShell.tsx");
  assert.match(settings, /onStatus: \(message: string\) => void;/);
  assert.match(shell, /<SettingsView[^;]+onStatus=\{onStatus\}/);
  assert.doesNotMatch(settings, /<Save size=\{13\} \/>Save/);
  assert.doesNotMatch(settings, /startPhotoMapping/);
  assert.match(settings, /onStatus\("Settings saved\."\)/);
  assert.match(settings, /onStatus\("Naming settings saved\."\)/);
  assert.match(settings, /onStatus\("Map settings saved\."\)/);
  assert.doesNotMatch(settings, /Naming settings saved\.<\/div>/);
  assert.doesNotMatch(settings, /Map metadata saved/);
});

test("text and numeric preferences save only when editing completes", () => {
  const settings = source("../src/features/settings/SettingsView.tsx");
  assert.match(settings, /value=\{recentDraft\}[\s\S]*onChange=\{\(event\) => setRecentDraft\(event\.target\.value\)\}[\s\S]*onBlur=\{saveRecentSearchLimit\}/);
  assert.match(settings, /value=\{separator\}[\s\S]*onBlur=\{\(\) => void saveSeparator\(\)\}/);
  assert.match(settings, /value=\{tokenDraft\}[\s\S]*onBlur=\{saveToken\}/);
});

test("passing Hook tests save the captured snapshot without a second action", () => {
  const settings = source("../src/features/settings/SettingsView.tsx");
  assert.match(settings, /const snapshot = \{\s*script: scripts\[testedKind\],\s*cases: cases\[testedKind\],\s*\};/);
  assert.match(settings, /runNamingHookTests\(testedKind, snapshot\.script, snapshot\.cases\)/);
  assert.match(settings, /if \(next\.failed === 0\) \{[\s\S]*saveNamingHook\(testedKind, snapshot\.script, snapshot\.cases\)/);
  assert.doesNotMatch(settings, /testedSnapshot/);
  assert.doesNotMatch(settings, /onClick=\{\(\) => void save\(\)\}/);
});

test("Rename from taxonomy reports changed and unchanged outcomes", () => {
  assert.equal(renameFromTaxonomyStatus("old.jpg", "Canis lupus.jpg"), "Renamed to Canis lupus.jpg");
  assert.equal(
    renameFromTaxonomyStatus("Canis lupus.jpg", "Canis lupus.jpg"),
    "Filename already matches the naming settings.",
  );
  const menu = source("../src/features/photos/PhotoContextMenu.tsx");
  assert.match(menu, /onStatus\(renameFromTaxonomyStatus\(photo\.filename, renamed\.filename\)\)/);
  assert.equal(
    formatPhotoRenameSummary({ operation_id: 7, total: 23, applied: 18, no_change: 4, failed: 1 }),
    "Renamed 18 photos, 4 unchanged, 1 failed",
  );
});
