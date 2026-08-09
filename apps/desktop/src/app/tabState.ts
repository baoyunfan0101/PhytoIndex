export function closeTabState<T extends { id: string }>(
  tabs: T[],
  activeId: string | null,
  closingId: string,
  preferredActiveId: string | null = null,
): { tabs: T[]; activeId: string | null } {
  const closingIndex = tabs.findIndex((tab) => tab.id === closingId);
  if (closingIndex < 0) return { tabs, activeId };
  const remaining = tabs.filter((tab) => tab.id !== closingId);
  if (activeId !== closingId) return { tabs: remaining, activeId };
  if (preferredActiveId !== null && remaining.some((tab) => tab.id === preferredActiveId)) {
    return { tabs: remaining, activeId: preferredActiveId };
  }
  const nextIndex = Math.max(0, closingIndex - 1);
  return {
    tabs: remaining,
    activeId: remaining[nextIndex]?.id ?? null,
  };
}

export function closeAllTabsState<T>(): { tabs: T[]; activeId: null } {
  return { tabs: [], activeId: null };
}

export type TabStatusMap = Record<string, string>;

export function updateTabStatus(
  statuses: TabStatusMap,
  tabId: string,
  message: string,
): TabStatusMap {
  if (statuses[tabId] === message) return statuses;
  return { ...statuses, [tabId]: message };
}

export function getCurrentTabStatus(
  statuses: TabStatusMap,
  activeId: string | null,
): string {
  return activeId === null ? "Ready" : statuses[activeId] ?? "Ready";
}

export function pruneTabStatuses(
  statuses: TabStatusMap,
  existingTabIds: ReadonlySet<string>,
): TabStatusMap {
  const entries = Object.entries(statuses).filter(([tabId]) => existingTabIds.has(tabId));
  if (entries.length === Object.keys(statuses).length) return statuses;
  return Object.fromEntries(entries);
}
