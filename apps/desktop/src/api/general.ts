import { call } from "./client";
import {
  defaultGeneralSettings,
  type GeneralSettings,
  type WorkspaceState,
} from "./generalModel";

export * from "./generalModel";

let fallbackSettings = defaultGeneralSettings();
let fallbackWorkspace: WorkspaceState = { opened_tabs: [], active_tab: null };

export const getGeneralSettings = () =>
  call<GeneralSettings>("get_general_settings", undefined, () => ({ ...fallbackSettings }));

export const updateGeneralSettings = (settings: GeneralSettings) =>
  call<GeneralSettings>("update_general_settings", { settings }, () => {
    fallbackSettings = { ...settings };
    return { ...fallbackSettings };
  });

export const getWorkspaceState = () =>
  call<WorkspaceState>("get_workspace_state", undefined, () => structuredClone(fallbackWorkspace));

export const saveWorkspaceState = (workspaceState: WorkspaceState) =>
  call<void>("save_workspace_state", { workspaceState }, () => {
    fallbackWorkspace = structuredClone(workspaceState);
  });
