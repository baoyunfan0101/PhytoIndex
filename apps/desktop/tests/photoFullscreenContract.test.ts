import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

function source(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("full image zoom uses the two-dimensional transform path", () => {
  const media = source("../src/features/photos/PhotoMedia.tsx");
  const styles = source("../src/styles/photos.css");
  assert.match(media, /translate\(-50%, -50%\) translate\(/);
  assert.doesNotMatch(media, /translate3d|scale3d/);
  assert.doesNotMatch(styles, /backface-visibility|will-change:\s*transform/);
});

test("image keyboard handling gives fullscreen Escape priority", () => {
  const display = source("../src/features/photos/PhotoDisplay.tsx");
  assert.match(display, /modeRef\.current === "image" && event\.key === "Enter"/);
  assert.match(display, /onEnterFullscreenRef\.current\?\.\(\)/);
  assert.match(display, /if \(isPhotoFullscreenActive\(\)\) return;/);
  assert.match(display, /setMode\("thumbnails"\)/);
});

test("native fullscreen capability and shared presentation are wired", () => {
  const capability = source("../src-tauri/capabilities/desktop.json");
  const shell = source("../src/app/DesktopShell.tsx");
  assert.match(capability, /core:window:allow-set-fullscreen/);
  assert.match(shell, /getCurrentWindow\(\)\.setFullscreen\(true\)/);
  assert.match(shell, /getCurrentWindow\(\)\.setFullscreen\(false\)/);
  assert.match(shell, /<PhotoFullscreenPresentation/);
});
