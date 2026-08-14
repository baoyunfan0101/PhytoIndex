export function shouldSwitchPhotoLibrary(
  active: boolean,
  available: boolean,
  busy: boolean,
): boolean {
  return !active && available && !busy;
}
