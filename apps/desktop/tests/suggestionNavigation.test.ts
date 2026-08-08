import assert from "node:assert/strict";
import test from "node:test";
import { moveSuggestionSelection } from "../src/shared/suggestionNavigation.ts";

test("moves down from the input into suggestions without wrapping", () => {
  assert.equal(moveSuggestionSelection(-1, 3, 1), 0);
  assert.equal(moveSuggestionSelection(0, 3, 1), 1);
  assert.equal(moveSuggestionSelection(2, 3, 1), 2);
});

test("moves up from the first suggestion back to the input", () => {
  assert.equal(moveSuggestionSelection(2, 3, -1), 1);
  assert.equal(moveSuggestionSelection(1, 3, -1), 0);
  assert.equal(moveSuggestionSelection(0, 3, -1), -1);
  assert.equal(moveSuggestionSelection(-1, 3, -1), -1);
});

test("keeps the input selected when there are no suggestions", () => {
  assert.equal(moveSuggestionSelection(0, 0, 1), -1);
  assert.equal(moveSuggestionSelection(0, 0, -1), -1);
});
