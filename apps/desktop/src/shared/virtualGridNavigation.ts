export type GridMoveDirection = "left" | "right" | "up" | "down";

export function nextGridIndex(
  length: number,
  activeIndex: number,
  columns: number,
  direction: GridMoveDirection,
): number {
  if (length <= 0) return -1;
  if (activeIndex < 0 || activeIndex >= length) return 0;
  const safeColumns = Math.max(1, columns);
  if (direction === "left") return Math.max(0, activeIndex - 1);
  if (direction === "right") return Math.min(length - 1, activeIndex + 1);
  const candidate = activeIndex + (direction === "down" ? safeColumns : -safeColumns);
  return candidate >= 0 && candidate < length ? candidate : activeIndex;
}
