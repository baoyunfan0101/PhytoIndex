export function clampPanelSize(
  requested: number,
  available: number,
  minimumFirst: number,
  minimumSecond: number,
): number {
  const safeAvailable = Math.max(available, minimumFirst + minimumSecond + 7);
  const maximumFirst = Math.max(minimumFirst, safeAvailable - minimumSecond - 7);
  const safeRequested = Number.isFinite(requested) ? requested : minimumFirst;
  return Math.min(maximumFirst, Math.max(minimumFirst, safeRequested));
}
