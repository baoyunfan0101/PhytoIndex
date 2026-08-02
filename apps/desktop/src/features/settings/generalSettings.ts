import type { GeneralSettings, ThemePreference } from "../../api/generalModel";

export function normalizeGeneralSettings(value: Partial<GeneralSettings>): GeneralSettings {
  const fallback: GeneralSettings = {
    theme: "dark",
    restore_tabs: true,
    recent_searches_limit: 10,
  };
  const theme = isTheme(value.theme) ? value.theme : fallback.theme;
  const recentSearchesLimit = Number.isInteger(value.recent_searches_limit)
    && value.recent_searches_limit! >= 1
    && value.recent_searches_limit! <= 50
    ? value.recent_searches_limit!
    : fallback.recent_searches_limit;
  return {
    theme,
    restore_tabs: typeof value.restore_tabs === "boolean" ? value.restore_tabs : fallback.restore_tabs,
    recent_searches_limit: recentSearchesLimit,
  };
}

export function applyTheme(
  theme: ThemePreference,
  root: Pick<HTMLElement, "dataset"> = document.documentElement,
): void {
  if (theme === "system") delete root.dataset.theme;
  else root.dataset.theme = theme;
}

function isTheme(value: unknown): value is ThemePreference {
  return value === "system" || value === "light" || value === "dark";
}
