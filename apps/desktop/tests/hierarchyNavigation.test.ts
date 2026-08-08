import assert from "node:assert/strict";
import test from "node:test";
import {
  createHierarchyNavigationState,
  currentTaxonForRoot,
  hierarchyNavigationReducer,
  reconcileSelectedRoot,
  recordHierarchyPosition,
  taxonSearchMatchExplanation,
} from "../src/features/taxonomy/hierarchyNavigation.ts";

test("stores an independent current taxon for each search root", () => {
  let positions = {};
  assert.equal(currentTaxonForRoot(10, positions), 10);
  positions = recordHierarchyPosition(positions, 10, 11);
  assert.equal(currentTaxonForRoot(20, positions), 20);
  positions = recordHierarchyPosition(positions, 20, 21);
  assert.equal(currentTaxonForRoot(10, positions), 11);
  assert.equal(currentTaxonForRoot(20, positions), 21);
});

test("reconciles the selected root without resetting a surviving result", () => {
  assert.equal(reconcileSelectedRoot(20, [10, 20, 30]), 20);
  assert.equal(reconcileSelectedRoot(20, [10, 30]), 10);
  assert.equal(reconcileSelectedRoot(null, [10, 30]), 10);
  assert.equal(reconcileSelectedRoot(20, []), null);
});

test("loads children only after expansion and resets them on navigation", () => {
  let state = createHierarchyNavigationState(10);
  assert.equal(state.childrenExpanded, false);
  assert.equal(state.childrenRequested, false);
  state = hierarchyNavigationReducer(state, { type: "toggle-children" });
  assert.equal(state.childrenExpanded, true);
  assert.equal(state.childrenRequested, true);
  state = hierarchyNavigationReducer(state, { type: "toggle-children" });
  assert.equal(state.childrenExpanded, false);
  assert.equal(state.childrenRequested, true);
  state = hierarchyNavigationReducer(state, { type: "navigate", taxonId: 11 });
  assert.deepEqual(state, createHierarchyNavigationState(11));
  state = hierarchyNavigationReducer(
    hierarchyNavigationReducer(state, { type: "toggle-children" }),
    { type: "reset", taxonId: 11 },
  );
  assert.deepEqual(state, createHierarchyNavigationState(11));
});

test("explains alias matches but not accepted-name matches", () => {
  const result = {
    taxon_id: 10,
    rank: "species" as const,
    names: { sci_name: "Canis lupus", zh_name: null, en_name: "Wolf" },
    matches: [{ name_id: 2, name_type: "synonym" as const, name: "Canis lycaon" }],
  };
  assert.equal(taxonSearchMatchExplanation(result), "Matched synonym: Canis lycaon");
  assert.equal(taxonSearchMatchExplanation({
    ...result,
    matches: [{ name_id: 1, name_type: "sci_name", name: "Canis lupus" }],
  }), null);
  assert.equal(taxonSearchMatchExplanation({
    ...result,
    matches: [
      { name_id: 1, name_type: "sci_name", name: "Canis lupus" },
      { name_id: 2, name_type: "synonym", name: "Canis lycaon" },
    ],
  }), null);
});
