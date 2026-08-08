export function moveSuggestionSelection(
  currentIndex: number,
  suggestionCount: number,
  direction: -1 | 1,
): number {
  if (suggestionCount <= 0) return -1;
  if (direction === 1) return Math.min(currentIndex + 1, suggestionCount - 1);
  return currentIndex <= 0 ? -1 : currentIndex - 1;
}
