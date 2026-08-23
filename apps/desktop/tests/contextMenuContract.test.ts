import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

function source(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("the application suppresses native context menus without blocking bubbling", () => {
  const app = source("../src/App.tsx");
  const shell = source("../src/app/DesktopShell.tsx");
  assert.match(app, /document\.addEventListener\("contextmenu", preventNativeContextMenu\)/);
  assert.match(app, /document\.removeEventListener\("contextmenu", preventNativeContextMenu\)/);
  assert.match(app, /event\.preventDefault\(\)/);
  assert.doesNotMatch(app, /stop(?:Immediate)?Propagation/);
  assert.doesNotMatch(shell, /desktop-shell" onContextMenu/);
});

test("standalone mapping editor owns the shared photo context menu", () => {
  const editor = source("../src/features/mapping/MappingEditor.tsx");
  assert.match(editor, /function StandaloneMappingPhotoPane/);
  assert.match(editor, /usePhotoInteraction/);
  assert.match(editor, /<PhotoStage photo=\{photo\} onContextMenu=\{interaction\.openContextMenu\}/);
  assert.match(editor, /\{interaction\.contextMenu\}/);
  assert.match(editor, /if \(embedded\) return/);
});

test("every interactive full-photo view supplies the shared context menu handler", () => {
  for (const path of [
    "../src/features/photos/PhotoBrowser.tsx",
    "../src/features/photos/PhotosView.tsx",
    "../src/features/photos/PhotoDetailView.tsx",
    "../src/features/mapping/MappingView.tsx",
  ]) {
    assert.match(source(path), /onContextMenu=\{interaction\.openContextMenu\}/);
  }
});
