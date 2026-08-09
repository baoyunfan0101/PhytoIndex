import assert from "node:assert/strict";
import test from "node:test";
import { treeArrowAction } from "../src/features/photos/photoListNavigation.ts";

test("right expands only a collapsed tree item", () => {
  assert.equal(treeArrowAction(false, 1), "expand");
  assert.equal(treeArrowAction(true, 1), null);
});

test("left collapses only an expanded tree item", () => {
  assert.equal(treeArrowAction(true, -1), "collapse");
  assert.equal(treeArrowAction(false, -1), null);
});
