import assert from "node:assert/strict";
import test from "node:test";
import {
  createNavigationHistory,
  findNavigationTarget,
  pruneNavigationHistory,
  recordNavigation,
  type NavigationHistory,
} from "../src/v3/navigationHistory.ts";

function ids(history: NavigationHistory) {
  return history.entries.map((entry) => entry.tabId);
}

test("navigates A, B, C backward and forward without recording", () => {
  let history = createNavigationHistory("A");
  history = recordNavigation(history, "B");
  history = recordNavigation(history, "C");
  const tabs = new Set(["A", "B", "C"]);
  assert.deepEqual(findNavigationTarget(history, tabs, -1), { index: 1, tabId: "B" });
  history = { ...history, index: 1 };
  assert.deepEqual(findNavigationTarget(history, tabs, 1), { index: 2, tabId: "C" });
  assert.deepEqual(ids(history), ["A", "B", "C"]);
});

test("opening D after Back truncates the old forward history", () => {
  const history = recordNavigation(
    { entries: [{ tabId: "A" }, { tabId: "B" }, { tabId: "C" }], index: 1 },
    "D",
  );
  assert.deepEqual(ids(history), ["A", "B", "D"]);
  assert.equal(history.index, 2);
});

test("prunes current and non-current closed tabs while preserving order", () => {
  const history = {
    entries: ["A", "B", "C"].map((tabId) => ({ tabId })),
    index: 1,
  };
  const currentClosed = pruneNavigationHistory(history, new Set(["A", "C"]), "C");
  assert.deepEqual(ids(currentClosed), ["A", "C"]);
  assert.equal(currentClosed.index, 1);
  const otherClosed = pruneNavigationHistory(history, new Set(["B", "C"]), "B");
  assert.deepEqual(ids(otherClosed), ["B", "C"]);
  assert.equal(otherClosed.index, 0);
});

test("skips invalid entries in either direction", () => {
  const history = {
    entries: ["A", "missing-1", "B", "missing-2", "C"].map((tabId) => ({ tabId })),
    index: 2,
  };
  const tabs = new Set(["A", "B", "C"]);
  assert.deepEqual(findNavigationTarget(history, tabs, -1), { index: 0, tabId: "A" });
  assert.deepEqual(findNavigationTarget(history, tabs, 1), { index: 4, tabId: "C" });
});

test("workspace and taxonomy resets cannot retain invalid resource tabs", () => {
  const history = {
    entries: ["settings", "folder:A", "taxon:A", "mapping:A"].map((tabId) => ({ tabId })),
    index: 3,
  };
  const workspaceReset = pruneNavigationHistory(
    history,
    new Set(["settings", "folders"]),
    "folders",
  );
  assert.deepEqual(ids(workspaceReset), ["settings", "folders"]);
  const taxonomyReset = pruneNavigationHistory(
    history,
    new Set(["settings", "folder:A"]),
    "settings",
  );
  assert.deepEqual(ids(taxonomyReset), ["settings", "folder:A"]);
  assert.equal(taxonomyReset.index, 0);
});

test("recording or pruning does not create consecutive duplicates", () => {
  const recorded = recordNavigation(createNavigationHistory("A"), "A");
  assert.deepEqual(ids(recorded), ["A"]);
  const pruned = pruneNavigationHistory({
    entries: ["A", "A", "B", "B"].map((tabId) => ({ tabId })),
    index: 3,
  }, new Set(["A", "B"]), "B");
  assert.deepEqual(ids(pruned), ["A", "B"]);
});
