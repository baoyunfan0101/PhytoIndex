import assert from "node:assert/strict";
import test from "node:test";
import { closeAllTabsState, closeTabState } from "../src/app/tabState.ts";

const tab = (id: string) => ({ id });

test("closing the final tab leaves an empty workspace", () => {
  assert.deepEqual(closeTabState([tab("A")], "A", "A"), {
    tabs: [],
    activeId: null,
  });
});

test("closing the active tab prefers its previous neighbor", () => {
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
