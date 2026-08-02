import type {
  WorkspaceSettingsSection,
  WorkspaceState,
  WorkspaceTab,
} from "../api/general";
import type { Photo } from "../api/photos";

export type AppTab = {
  id: string;
  kind: WorkspaceTab["kind"];
  title: string;
  query?: string;
  taxonId?: number;
  photo?: Photo;
  settingsSection?: WorkspaceSettingsSection;
};

const photoWorkspaceKinds = new Set<AppTab["kind"]>([
  "folders",
  "photo-taxonomy",
  "map",
  "photo-history",
  "mapping",
  "search-photos",
  "taxon-photos",
  "photo-detail",
  "mapping-editor",
]);

export function serializeWorkspaceState(tabs: AppTab[], activeId: string | null): WorkspaceState {
  return {
    opened_tabs: tabs.map((tab) => ({
      id: tab.id,
      kind: tab.kind,
      title: tab.title,
      query: tab.query ?? null,
      taxon_id: tab.taxonId ?? null,
      photo_id: tab.photo?.photo_id ?? null,
      settings_section: tab.settingsSection ?? null,
    })),
    active_tab: activeId !== null && tabs.some((tab) => tab.id === activeId) ? activeId : null,
  };
}

export async function restoreWorkspaceState(
  state: WorkspaceState,
  options: {
    getPhoto: (photoId: number) => Promise<Photo>;
    taxonExists: (taxonId: number) => Promise<boolean>;
    photoWorkspaceAvailable: boolean;
  },
): Promise<{ tabs: AppTab[]; activeId: string | null }> {
  const tabs: AppTab[] = [];
  const ids = new Set<string>();
  for (const saved of state.opened_tabs) {
    if (!saved.id.trim() || !saved.title.trim() || ids.has(saved.id)) continue;
    if (photoWorkspaceKinds.has(saved.kind) && !options.photoWorkspaceAvailable) continue;
    const tab = await restoreTab(saved, options);
    if (!tab) continue;
    ids.add(tab.id);
    tabs.push(tab);
  }
  return {
    tabs,
    activeId: state.active_tab !== null && ids.has(state.active_tab)
      ? state.active_tab
      : tabs[0]?.id ?? null,
  };
}

async function restoreTab(
  saved: WorkspaceTab,
  options: {
    getPhoto: (photoId: number) => Promise<Photo>;
    taxonExists: (taxonId: number) => Promise<boolean>;
  },
): Promise<AppTab | null> {
  if (saved.kind === "search-photos" && !saved.query?.trim()) return null;
  if ((saved.kind === "taxon-photos" || saved.kind === "taxon-detail") && saved.taxon_id === null) {
    return null;
  }
  if (saved.taxon_id !== null && !(await options.taxonExists(saved.taxon_id))) return null;
  let photo: Photo | undefined;
  if (saved.kind === "photo-detail" || saved.kind === "mapping-editor") {
    if (saved.photo_id === null) return null;
    try {
      photo = await options.getPhoto(saved.photo_id);
    } catch {
      return null;
    }
  }
  return {
    id: saved.id,
    kind: saved.kind,
    title: saved.title,
    ...(saved.query === null ? {} : { query: saved.query }),
    ...(saved.taxon_id === null ? {} : { taxonId: saved.taxon_id }),
    ...(photo === undefined ? {} : { photo }),
    ...(saved.settings_section === null ? {} : { settingsSection: saved.settings_section }),
  };
}
