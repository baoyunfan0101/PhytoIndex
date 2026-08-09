import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useRef } from "react";
import { desktopRuntime } from "../api/client";

export const nativeMenuActions = {
  aboutVividarium: "app.about-vividarium",
  openPhotoLibrary: "file.open-photo-library",
  managePhotoLibraries: "file.manage-photo-libraries",
  openTaxonomyDatabase: "file.open-taxonomy-database",
  manageTaxonomyDatabases: "file.manage-taxonomy-databases",
  closeAllTabs: "file.close-all-tabs",
} as const;

export type NativeMenuAction = typeof nativeMenuActions[keyof typeof nativeMenuActions];

export function useNativeMenu(handler: (action: NativeMenuAction) => void): void {
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => {
    if (!desktopRuntime) return;
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    void listen<string>("native-menu-action", (event) => {
      if (isNativeMenuAction(event.payload)) handlerRef.current(event.payload);
    }).then((nextUnlisten) => {
      if (disposed) nextUnlisten();
      else unlisten = nextUnlisten;
    }).catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
}

function isNativeMenuAction(value: string): value is NativeMenuAction {
  return Object.values(nativeMenuActions).some((action) => action === value);
}
