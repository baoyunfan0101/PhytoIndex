import assert from "node:assert/strict";
import test from "node:test";
import {
  closeAllTabsState,
  closeTabState,
  getCurrentTabStatus,
  pruneTabStatuses,
  updateTabStatus,
} from "../src/app/tabState.ts";

const tab = (id: string) => ({ id });

test("closing the final tab leaves an empty workspace", () => {
  assert.deepEqual(closeTabState([tab("A")], "A", "A"), {
    tabs: [],
    activeId: null,
  });
});

test("closing the active tab prefers the previous active tab", () => {
  assert.deepEqual(closeTabState([tab("A"), tab("B"), tab("C")], "C", "C", "A"), {
    tabs: [tab("A"), tab("B")],
    activeId: "A",
  });
});

test("closing the active tab falls back to its previous neighbor", () => {
  assert.deepEqual(closeTabState([tab("A"), tab("B"), tab("C")], "B", "B"), {
    tabs: [tab("A"), tab("C")],
    activeId: "A",
  });
  assert.deepEqual(closeTabState([tab("A"), tab("B")], "A", "A"), {
    tabs: [tab("B")],
    activeId: "B",
  });
});

test("closing an inactive tab preserves the active tab", () => {
  assert.deepEqual(closeTabState([tab("A"), tab("B")], "B", "A"), {
    tabs: [tab("B")],
    activeId: "B",
  });
});

test("closing all tabs clears the active tab", () => {
  assert.deepEqual(closeAllTabsState(), { tabs: [], activeId: null });
});

test("keeps the latest status isolated by tab", () => {
  const afterFolders = updateTabStatus({}, "folders", "4 folders, 20 photos");
  const afterSql = updateTabStatus(afterFolders, "custom-sql", "Query completed");
  assert.equal(getCurrentTabStatus(afterSql, "folders"), "4 folders, 20 photos");
  assert.equal(getCurrentTabStatus(afterSql, "custom-sql"), "Query completed");
  assert.equal(getCurrentTabStatus(afterSql, "formatted-update"), "Ready");
  assert.equal(getCurrentTabStatus(afterSql, null), "Ready");
});

test("removes statuses belonging to closed tabs", () => {
  assert.deepEqual(pruneTabStatuses({ A: "Done", B: "Failed" }, new Set(["B"])), {
    B: "Failed",
  });
});
