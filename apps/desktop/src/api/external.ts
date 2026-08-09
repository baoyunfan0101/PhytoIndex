import { openUrl } from "@tauri-apps/plugin-opener";
import { desktopRuntime } from "./client";

export const projectRepositoryUrl = "https://github.com/baoyunfan0101/Vividarium";
export const authorEmail = "baoyunfan0101@gmail.com";
export const authorEmailUrl = `mailto:${authorEmail}`;

export async function openExternalUrl(url: string): Promise<void> {
  if (desktopRuntime) {
    await openUrl(url);
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
}
