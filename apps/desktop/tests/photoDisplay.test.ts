import assert from "node:assert/strict";
import test from "node:test";
import { nextGridIndex } from "../src/shared/virtualGridNavigation.ts";

test("moves through a photo grid without vertical wrapping", () => {
  assert.equal(nextGridIndex(10, 4, 4, "left"), 3);
  assert.equal(nextGridIndex(10, 3, 4, "right"), 4);
  assert.equal(nextGridIndex(10, 5, 4, "up"), 1);
  assert.equal(nextGridIndex(10, 5, 4, "down"), 9);
  assert.equal(nextGridIndex(10, 1, 4, "up"), 1);
  assert.equal(nextGridIndex(10, 7, 4, "down"), 7);
});

test("selects the first grid photo when keyboard navigation starts without a selection", () => {
  assert.equal(nextGridIndex(4, -1, 2, "right"), 0);
  assert.equal(nextGridIndex(0, -1, 2, "right"), -1);
});
