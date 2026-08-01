import { invoke } from "@tauri-apps/api/core";

export const desktopRuntime = "__TAURI_INTERNALS__" in window;

export async function call<T>(
  command: string,
  args: Record<string, unknown> | undefined,
  fallback: () => T | Promise<T>,
): Promise<T> {
  if (desktopRuntime) return invoke<T>(command, args);
  await new Promise((resolve) => window.setTimeout(resolve, 40));
  return fallback();
}
