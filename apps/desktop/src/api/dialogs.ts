import { open, save } from "@tauri-apps/plugin-dialog";
import { desktopRuntime } from "./client";

export async function selectPhotoDirectory(): Promise<string | null> {
  if (!desktopRuntime) return "/Demo/Vividarium Photos";
  const selected = await open({ directory: true, multiple: false });
  return typeof selected === "string" ? selected : null;
}

export async function selectSqliteDatabase(): Promise<string | null> {
  if (!desktopRuntime) return "/Demo/Vividarium/source.db";
  const selected = await open({
    directory: false,
    multiple: false,
    filters: [{ name: "SQLite database", extensions: ["db", "sqlite", "sqlite3"] }],
  });
  return typeof selected === "string" ? selected : null;
}

export async function selectCsvFile(): Promise<string | null> {
  if (!desktopRuntime) return "/Demo/Vividarium/source.csv";
  const selected = await open({
    directory: false,
    multiple: false,
    filters: [{ name: "CSV", extensions: ["csv"] }],
  });
  return typeof selected === "string" ? selected : null;
}

export async function selectDatabaseDestination(defaultPath?: string): Promise<string | null> {
  if (!desktopRuntime) return defaultPath ?? "/Demo/Vividarium/destination.db";
  return save({
    defaultPath,
    filters: [{ name: "SQLite database", extensions: ["db", "sqlite", "sqlite3"] }],
  });
}

export async function selectCsvDestination(defaultPath?: string): Promise<string | null> {
  if (!desktopRuntime) return defaultPath ?? "/Demo/Vividarium/export.csv";
  return save({ defaultPath, filters: [{ name: "CSV", extensions: ["csv"] }] });
}
