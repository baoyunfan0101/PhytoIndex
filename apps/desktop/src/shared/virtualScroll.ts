export function clampVirtualScrollTop(
  scrollTop: number,
  itemCount: number,
  itemHeight: number,
  viewportHeight: number,
): number {
  const contentHeight = Math.max(0, itemCount) * Math.max(0, itemHeight);
  const maximum = Math.max(0, contentHeight - Math.max(0, viewportHeight));
  return Math.min(Math.max(0, scrollTop), maximum);
}
