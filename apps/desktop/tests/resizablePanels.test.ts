import assert from "node:assert/strict";
import test from "node:test";
import { clampPanelSize } from "../src/shared/panelSizing.ts";

test("clamps a panel size to both panel minimums", () => {
  assert.equal(clampPanelSize(40, 800, 160, 320), 160);
  assert.equal(clampPanelSize(700, 800, 160, 320), 473);
  assert.equal(clampPanelSize(260, 800, 160, 320), 260);
});

test("preserves the first panel minimum when the container is too small", () => {
  assert.equal(clampPanelSize(200, 300, 180, 180), 180);
});
