import assert from "node:assert/strict";
import test from "node:test";
import { clampVirtualScrollTop } from "../src/shared/virtualScroll.ts";

test("keeps a valid virtual-list scroll position", () => {
  assert.equal(clampVirtualScrollTop(120, 20, 30, 240), 120);
});

test("clamps stale search scroll after the result set shrinks", () => {
  assert.equal(clampVirtualScrollTop(900, 7, 60, 240), 180);
  assert.equal(clampVirtualScrollTop(900, 3, 60, 240), 0);
});
