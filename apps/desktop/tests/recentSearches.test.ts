import assert from "node:assert/strict";
import test from "node:test";
import {
  RECENT_SEARCHES_KEY,
  addRecentSearch,
  loadRecentSearches,
  normalizeSearchQuery,
  removeRecentSearch,
  saveRecentSearches,
  type RecentSearchStorage,
} from "../src/features/photos/search/recentSearchStorage.ts";

function memoryStorage(initial: string | null = null): RecentSearchStorage {
  let value = initial;
  return {
    getItem: (key) => key === RECENT_SEARCHES_KEY ? value : null,
    setItem: (key, next) => {
      if (key === RECENT_SEARCHES_KEY) value = next;
    },
  };
}

test("normalizes only the submitted query boundary", () => {
  assert.equal(normalizeSearchQuery("  Panthera  leo  "), "Panthera  leo");
  assert.equal(normalizeSearchQuery("   "), "");
});

test("moves duplicate searches to the front and preserves recent spelling", () => {
  const searches = addRecentSearch(["Canis lupus", "Panthera leo"], "  panthera LEO ");
  assert.deepEqual(searches, ["panthera LEO", "Canis lupus"]);
  assert.deepEqual(addRecentSearch(searches, "   "), searches);
});

test("limits searches in most-recent-first order", () => {
  let searches: string[] = [];
  for (let index = 0; index < 14; index += 1) {
    searches = addRecentSearch(searches, `query ${index}`);
  }
  assert.equal(searches.length, 10);
  assert.equal(searches[0], "query 13");
  assert.equal(searches[9], "query 4");
});

test("loads sanitized storage and supports removal and clearing", () => {
  const storage = memoryStorage(JSON.stringify([" A ", "a", "", 3, "B"]));
  assert.deepEqual(loadRecentSearches(storage), ["A", "B"]);
  const next = removeRecentSearch(["A", "B"], " a ");
  assert.deepEqual(next, ["B"]);
  saveRecentSearches(next, storage);
  assert.deepEqual(loadRecentSearches(storage), ["B"]);
  saveRecentSearches([], storage);
  assert.deepEqual(loadRecentSearches(storage), []);
});

test("treats malformed local storage as empty", () => {
  assert.deepEqual(loadRecentSearches(memoryStorage("not-json")), []);
});
