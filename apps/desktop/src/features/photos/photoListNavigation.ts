export function nextListIndex(length: number, activeIndex: number, direction: -1 | 1): number {
  if (length <= 0) return -1;
  if (activeIndex < 0) return direction === 1 ? 0 : length - 1;
  return Math.min(Math.max(activeIndex + direction, 0), length - 1);
}

export function findTypeSelectIndex<T>(
  items: T[],
  query: string,
  labelsForItem: (item: T) => Array<string | null | undefined>,
  startIndex = 0,
): number {
  const normalizedQuery = normalizeTypeSelectText(query);
  if (!normalizedQuery || items.length === 0) return -1;
  const prefixMatch = findByMatcher(
    items,
    labelsForItem,
    normalizedQuery,
    startIndex,
    (label) => label.startsWith(normalizedQuery),
  );
  if (prefixMatch >= 0) return prefixMatch;
  return findByMatcher(
    items,
    labelsForItem,
    normalizedQuery,
    startIndex,
    (label) => label.includes(normalizedQuery),
  );
}

function findByMatcher<T>(
  items: T[],
  labelsForItem: (item: T) => Array<string | null | undefined>,
  query: string,
  startIndex: number,
  matches: (label: string) => boolean,
): number {
  for (let offset = 0; offset < items.length; offset += 1) {
    const index = (startIndex + offset) % items.length;
    const labels = labelsForItem(items[index]).map(normalizeTypeSelectText);
    if (labels.some(matches)) return index;
  }
  return -1;
}

function normalizeTypeSelectText(value: string | null | undefined): string {
  return (value ?? "").trim().toLocaleLowerCase();
}
