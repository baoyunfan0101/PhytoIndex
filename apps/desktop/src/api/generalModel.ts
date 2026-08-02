export type ThemePreference = "system" | "light" | "dark";

export type GeneralSettings = {
  theme: ThemePreference;
  restore_tabs: boolean;
  recent_searches_limit: number;
};

export type WorkspaceTabKind =
  | "folders"
  | "photo-taxonomy"
  | "map"
  | "photo-history"
  | "mapping"
  | "taxonomy-search"
  | "formatted-update"
  | "custom-sql"
  | "taxonomy-history"
  | "settings"
  | "search-photos"
  | "taxon-photos"
  | "photo-detail"
  | "mapping-editor"
  | "taxon-detail";

export type WorkspaceSettingsSection =
  | "General"
  | "Storage"
  | "Photo Libraries"
  | "Taxonomy Databases"
  | "Naming"
  | "Map"
  | "Filename Parser"
  | "Synonym Splitter"
  | "About";

export type WorkspaceTab = {
  id: string;
  kind: WorkspaceTabKind;
  title: string;
  query: string | null;
  taxon_id: number | null;
  photo_id: number | null;
  settings_section: WorkspaceSettingsSection | null;
};

export type WorkspaceState = {
  opened_tabs: WorkspaceTab[];
  active_tab: string | null;
};

export const defaultGeneralSettings = (): GeneralSettings => ({
  theme: "system",
  restore_tabs: true,
  recent_searches_limit: 10,
});
