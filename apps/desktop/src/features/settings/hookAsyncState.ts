import type { NamingHookKind } from "../../api/settings";

export function canPresentHookResult(
  testedKind: NamingHookKind,
  activeKind: NamingHookKind,
  testedRevision: number,
  currentRevision: number,
): boolean {
  return testedKind === activeKind && testedRevision === currentRevision;
}
