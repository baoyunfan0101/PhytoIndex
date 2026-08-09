export const RECENT_SEARCHES_KEY = "vividarium.recent-photo-searches";
export const RECENT_SEARCHES_LIMIT = 10;

export type RecentSearchStorage = Pick<Storage, "getItem" | "setItem">;

export function normalizeSearchQuery(query: string): string {
  return query.trim();
}

export function addRecentSearch(
  searches: string[],
  query: string,
  limit = RECENT_SEARCHES_LIMIT,
): string[] {
  const normalized = normalizeSearchQuery(query);
  if (!normalized) return searches;
  const key = normalized.toLocaleLowerCase();
  return [
    normalized,
    ...searches.filter((item) => normalizeSearchQuery(item).toLocaleLowerCase() !== key),
  ].slice(0, limit);
}

export function removeRecentSearch(searches: string[], query: string): string[] {
  const key = normalizeSearchQuery(query).toLocaleLowerCase();
  return searches.filter((item) => normalizeSearchQuery(item).toLocaleLowerCase() !== key);
}

export function trimRecentSearches(searches: string[], limit: number): string[] {
  return searches.slice(0, limit);
}

export function loadRecentSearches(
  storage: RecentSearchStorage | null = browserStorage(),
  limit = RECENT_SEARCHES_LIMIT,
): string[] {
  if (!storage) return [];
  try {
    const stored: unknown = JSON.parse(storage.getItem(RECENT_SEARCHES_KEY) ?? "[]");
    if (!Array.isArray(stored)) return [];
    const searches: string[] = [];
    const seen = new Set<string>();
    for (const item of stored) {
      if (typeof item !== "string") continue;
      const normalized = normalizeSearchQuery(item);
      const key = normalized.toLocaleLowerCase();
      if (!normalized || seen.has(key)) continue;
      searches.push(normalized);
      seen.add(key);
      if (searches.length === limit) break;
    }
    return searches;
  } catch {
    return [];
  }
}

export function saveRecentSearches(
  searches: string[],
  storage: RecentSearchStorage | null = browserStorage(),
): void {
  if (!storage) return;
  try {
    storage.setItem(RECENT_SEARCHES_KEY, JSON.stringify(searches));
  } catch {
    // Search remains usable when local storage is unavailable.
  }
}

function browserStorage(): RecentSearchStorage | null {
  if (typeof window === "undefined") return null;
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}
