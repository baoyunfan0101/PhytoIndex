import assert from "node:assert/strict";
import test from "node:test";
import type { WorkspaceState } from "../src/api/general.ts";
import type { Photo } from "../src/api/photos.ts";
import {
  restoreWorkspaceState,
  serializeWorkspaceState,
  type AppTab,
} from "../src/app/workspaceState.ts";

const photo: Photo = {
  photo_id: 7,
  directory_id: 2,
  relative_path: "Mammalia/Panthera_leo.jpg",
  filename: "Panthera_leo.jpg",
  file_size: 100,
  modified_at_ns: 200,
  thumbnail_path: null,
};

function savedState(): WorkspaceState {
  return {
    opened_tabs: [
      {
        id: "settings",
        kind: "settings",
        title: "Settings",
        query: null,
        taxon_id: null,
        photo_id: null,
        settings_section: "General",
      },
      {
        id: "search:lion",
        kind: "search-photos",
        title: "Search: lion",
        query: "lion",
        taxon_id: null,
        photo_id: null,
        settings_section: null,
      },
      {
        id: "taxon:10",
        kind: "taxon-detail",
        title: "Panthera",
        query: null,
        taxon_id: 10,
        photo_id: null,
        settings_section: null,
      },
      {
        id: "photo:7",
        kind: "photo-detail",
        title: photo.filename,
        query: null,
        taxon_id: null,
        photo_id: photo.photo_id,
        settings_section: null,
      },
    ],
    active_tab: "photo:7",
  };
}

test("serializes opened tabs and the active tab without transient UI", () => {
  const tabs: AppTab[] = [
    { id: "settings", kind: "settings", title: "Settings", settingsSection: "Direct Import" },
    { id: "photo:7", kind: "photo-detail", title: photo.filename, photo },
  ];
  assert.deepEqual(serializeWorkspaceState(tabs, "photo:7"), {
    opened_tabs: [
      {
        id: "settings",
        kind: "settings",
        title: "Settings",
        query: null,
        taxon_id: null,
        photo_id: null,
        settings_section: "Direct Import",
      },
      {
        id: "photo:7",
        kind: "photo-detail",
        title: photo.filename,
        query: null,
        taxon_id: null,
        photo_id: 7,
        settings_section: null,
      },
    ],
    active_tab: "photo:7",
  });
});

test("restores valid tabs and resolves saved photo records", async () => {
  const restored = await restoreWorkspaceState(savedState(), {
    getPhoto: async () => photo,
    photoWorkspaceAvailable: true,
    taxonExists: async () => true,
  });
  assert.equal(restored.tabs.length, 4);
  assert.equal(restored.tabs[3].photo?.photo_id, 7);
  assert.equal(restored.activeId, "photo:7");
});

test("ignores unavailable, invalid, and stale tabs and selects a valid active tab", async () => {
  const state = savedState();
  state.opened_tabs.push({ ...state.opened_tabs[0], id: "duplicate", title: "" });
  const restored = await restoreWorkspaceState(state, {
    getPhoto: async () => { throw new Error("missing photo"); },
    photoWorkspaceAvailable: false,
    taxonExists: async () => false,
  });
  assert.deepEqual(restored.tabs.map((tab) => tab.id), ["settings"]);
  assert.equal(restored.activeId, "settings");
});
