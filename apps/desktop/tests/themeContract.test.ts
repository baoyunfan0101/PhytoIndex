import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const theme = readFileSync(new URL("../src/styles/theme.css", import.meta.url), "utf8");
const photoStyles = readFileSync(new URL("../src/styles/photos.css", import.meta.url), "utf8");
const sharedStyles = readFileSync(new URL("../src/styles/shared.css", import.meta.url), "utf8");
const taxonomyStyles = readFileSync(new URL("../src/styles/taxonomy.css", import.meta.url), "utf8");
const codeEditor = readFileSync(new URL("../src/shared/CodeEditor.tsx", import.meta.url), "utf8");

const requiredThemeTokens = [
  "button-plain-hover",
  "button-plain-active",
  "overlay-surface",
  "overlay-surface-text",
  "code-selection",
  "code-keyword",
  "code-string",
  "code-number",
  "code-comment",
  "code-function",
  "code-variable",
  "code-operator",
];

function themeBlock(start: string, end: string | null) {
  const startIndex = theme.indexOf(start);
  assert.notEqual(startIndex, -1, `missing ${start}`);
  return theme.slice(startIndex, end === null ? undefined : theme.indexOf(end, startIndex));
}

test("defines application and editor tokens for every Light and Dark theme path", () => {
  const dark = themeBlock(":root {", ":root[data-theme=\"light\"]");
  const explicitLight = themeBlock(":root[data-theme=\"light\"]", "@media (prefers-color-scheme: light)");
  const systemLight = themeBlock(":root:not([data-theme])", "}\n}\n\n*");

  for (const block of [dark, explicitLight, systemLight]) {
    for (const token of requiredThemeTokens) {
      assert.match(block, new RegExp(`--${token}:`));
    }
  }
});

test("uses theme tokens for menus, overlays, buttons, logs, and CodeMirror", () => {
  assert.match(photoStyles, /\.context-menu \{[^}]*border: 1px solid var\(--line-strong\)[^}]*background: var\(--popover\)[^}]*box-shadow: 0 10px 30px var\(--shadow\)/);
  assert.match(photoStyles, /\.context-menu > button:hover:not\(:disabled\) \{ background: var\(--hover\); \}/);
  assert.match(photoStyles, /\.pane-overlay[^}]*color: var\(--overlay-surface-text\)[^}]*background: var\(--overlay-surface\)/);
  assert.match(photoStyles, /\.map-count[^}]*color: var\(--overlay-surface-text\)[^}]*background: var\(--overlay-surface\)/);
  assert.match(sharedStyles, /\.floating-progress[^}]*background: var\(--popover\)/);
  assert.match(theme, /button:hover:not\(:disabled\), \.button:hover \{ background-color: var\(--button-plain-hover\)/);
  assert.match(theme, /button:active:not\(:disabled\), \.button:active \{ background-color: var\(--button-plain-active\)/);
  assert.match(taxonomyStyles, /\.log-row code \{[^}]*color: var\(--muted\)/);
  assert.match(codeEditor, /var\(--code-selection\)/);
  for (const token of requiredThemeTokens.filter((token) => token.startsWith("code-"))) {
    assert.match(codeEditor, new RegExp(`var\\(--${token}\\)`));
  }
});
