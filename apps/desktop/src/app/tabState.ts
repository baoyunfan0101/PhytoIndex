export function closeTabState<T extends { id: string }>(
  tabs: T[],
  activeId: string | null,
  closingId: string,
): { tabs: T[]; activeId: string | null } {
  const closingIndex = tabs.findIndex((tab) => tab.id === closingId);
  if (closingIndex < 0) return { tabs, activeId };
  const remaining = tabs.filter((tab) => tab.id !== closingId);
  if (activeId !== closingId) return { tabs: remaining, activeId };
  const nextIndex = Math.max(0, closingIndex - 1);
  return {
    tabs: remaining,
    activeId: remaining[nextIndex]?.id ?? null,
  };
}
