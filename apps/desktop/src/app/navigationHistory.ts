export type NavigationEntry = {
  tabId: string;
};

export type NavigationHistory = {
  entries: NavigationEntry[];
  index: number;
};

export type NavigationTarget = {
  index: number;
  tabId: string;
};

export function createNavigationHistory(tabId: string | null): NavigationHistory {
  return tabId === null
    ? { entries: [], index: -1 }
    : { entries: [{ tabId }], index: 0 };
}

export function recordNavigation(
  history: NavigationHistory,
  tabId: string,
): NavigationHistory {
  if (history.entries[history.index]?.tabId === tabId) return history;
  const entries = history.entries.slice(0, history.index + 1);
  if (entries[entries.length - 1]?.tabId !== tabId) entries.push({ tabId });
  return { entries, index: entries.length - 1 };
}

export function findNavigationTarget(
  history: NavigationHistory,
  existingTabIds: ReadonlySet<string>,
  direction: -1 | 1,
): NavigationTarget | null {
  for (
    let index = history.index + direction;
    index >= 0 && index < history.entries.length;
    index += direction
  ) {
    const entry = history.entries[index];
    if (existingTabIds.has(entry.tabId)) return { index, tabId: entry.tabId };
  }
  return null;
}

export function pruneNavigationHistory(
  history: NavigationHistory,
  existingTabIds: ReadonlySet<string>,
  activeTabId: string | null,
): NavigationHistory {
  const retained: Array<{ entry: NavigationEntry; oldIndex: number }> = [];
  history.entries.forEach((entry, oldIndex) => {
    if (!existingTabIds.has(entry.tabId)) return;
    if (retained[retained.length - 1]?.entry.tabId === entry.tabId) return;
    retained.push({ entry, oldIndex });
  });
  if (activeTabId !== null && existingTabIds.has(activeTabId)) {
    const activeEntries = retained
      .map((item, index) => ({ ...item, index }))
      .filter((item) => item.entry.tabId === activeTabId);
    if (activeEntries.length === 0) {
      retained.push({ entry: { tabId: activeTabId }, oldIndex: history.index });
      return {
        entries: retained.map((item) => item.entry),
        index: retained.length - 1,
      };
    }
    const current = activeEntries.reduce((nearest, item) => (
      Math.abs(item.oldIndex - history.index) < Math.abs(nearest.oldIndex - history.index)
        ? item
        : nearest
    ));
    return {
      entries: retained.map((item) => item.entry),
      index: current.index,
    };
  }
  if (retained.length === 0) return createNavigationHistory(null);
  let nearestIndex = 0;
  retained.forEach((item, index) => {
    if (
      Math.abs(item.oldIndex - history.index)
      < Math.abs(retained[nearestIndex].oldIndex - history.index)
    ) {
      nearestIndex = index;
    }
  });
  return {
    entries: retained.map((item) => item.entry),
    index: nearestIndex,
  };
}
