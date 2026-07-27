import {
  ArrowRightLeft,
  Braces,
  Database,
  FileClock,
  FolderOpen,
  HardDrive,
  History,
  ListTree,
  Map,
  Search,
  Settings,
  TableProperties,
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState, type RefObject } from "react";
import {
  getPhoto,
  getPhotoLibrary,
  listTaxonPhotos,
  openPhotoLibrary,
  searchPhotos,
  selectPhotoDirectory,
  suggestPhotoTaxa,
  type Photo,
  type TaxonSuggestion,
} from "./v3/api";
import type { IconComponent } from "./v3/components";
import { MappingEditor } from "./v3/MappingEditor";
import { PhotoBrowser } from "./v3/PhotoBrowser";
import { PhotoDetailView } from "./v3/PhotoDetailView";
import type { PhotoOpenHandlers } from "./v3/PhotoInteraction";
import { usePhotoMutation } from "./v3/photoMutations";
import {
  FolderPhotosView,
  PhotoHistoryView,
  PhotoMapView,
  TaxonPhotosView,
} from "./v3/PhotosView";
import { MappingView } from "./v3/MappingView";
import { SettingsView } from "./v3/SettingsView";
import {
  CustomUpdateView,
  FormattedUpdateView,
  TaxonomyHistoryView,
  TaxonomySearchView,
} from "./v3/TaxonomyView";
import { useCursorPage } from "./v3/useCursorPage";
import { ViewStateProvider, type ViewStateStore } from "./v3/viewState";

type TabKind =
  | "folders"
  | "photo-taxonomy"
  | "map"
  | "photo-history"
  | "mapping"
  | "taxonomy-search"
  | "formatted-update"
  | "custom-update"
  | "taxonomy-history"
  | "settings"
  | "search-photos"
  | "taxon-photos"
  | "photo-detail"
  | "mapping-editor"
  | "taxon-detail";

type AppTab = {
  id: string;
  kind: TabKind;
  title: string;
  query?: string;
  taxonId?: number;
  photo?: Photo;
};

const initialTab: AppTab = { id: "folders", kind: "folders", title: "Folders" };
const keepAliveTabKinds = new Set<TabKind>([
  "map",
  "formatted-update",
  "custom-update",
  "settings",
  "mapping",
]);

const photoItems: Array<[TabKind, string, IconComponent]> = [
  ["folders", "Folders", FolderOpen],
  ["photo-taxonomy", "Photo hierarchy", ListTree],
  ["map", "Map", Map],
  ["photo-history", "Rename history", History],
];
const taxonomyItems: Array<[TabKind, string, IconComponent]> = [
  ["taxonomy-search", "Taxonomy search", Database],
  ["formatted-update", "Formatted update", TableProperties],
  ["custom-update", "Custom update", Braces],
  ["taxonomy-history", "Update history", FileClock],
];

export function App() {
  const [tabs, setTabs] = useState<AppTab[]>([initialTab]);
  const [activeId, setActiveId] = useState(initialTab.id);
  const [workspaceId, setWorkspaceId] = useState(0);
  const [root, setRoot] = useState("");
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [suggestions, setSuggestions] = useState<TaxonSuggestion[]>([]);
  const [suggestionIndex, setSuggestionIndex] = useState(-1);
  const [status, setStatus] = useState("Ready");
  const searchRef = useRef<HTMLDivElement>(null);
  const searchToggleRef = useRef<HTMLButtonElement>(null);
  const viewStateStores = useRef(new globalThis.Map<string, ViewStateStore>());
  const active = tabs.find((tab) => tab.id === activeId) ?? tabs[0];

  usePhotoMutation((mutation) => {
    if (mutation.kind !== "photo") return;
    const applyPhoto = (photo: Photo) => setTabs((current) => current.map((tab) => {
      if (tab.photo?.photo_id !== photo.photo_id) return tab;
      const title = tab.kind === "photo-detail"
        ? photo.filename
        : tab.kind === "mapping-editor"
          ? `Map ${photo.filename}`
          : tab.title;
      return { ...tab, photo, title };
    }));
    if (mutation.photo) {
      applyPhoto(mutation.photo);
    } else {
      const photoIds = mutation.photoIds ?? (mutation.photoId === null ? [] : [mutation.photoId]);
      photoIds.forEach((photoId) => {
        void getPhoto(photoId).then(applyPhoto);
      });
    }
  });

  useEffect(() => {
    void getPhotoLibrary().then((library) => setRoot(library?.root_path ?? ""));
  }, []);

  useEffect(() => {
    const value = searchQuery.trim();
    if (!searchOpen || !value) {
      setSuggestions([]);
      setSuggestionIndex(-1);
      return;
    }
    const timer = window.setTimeout(() => void suggestPhotoTaxa(value).then((next) => {
      setSuggestions(next);
      setSuggestionIndex(-1);
    }), 140);
    return () => window.clearTimeout(timer);
  }, [searchOpen, searchQuery]);

  useEffect(() => {
    if (!searchOpen) return;
    const closeSearch = (event: PointerEvent) => {
      const target = event.target as Node;
      if (searchRef.current?.contains(target) || searchToggleRef.current?.contains(target)) return;
      setSearchOpen(false);
    };
    window.addEventListener("pointerdown", closeSearch);
    return () => window.removeEventListener("pointerdown", closeSearch);
  }, [searchOpen]);

  function openTab(tab: AppTab, singleton = false) {
    const existing = tabs.find((item) => item.id === tab.id || (singleton && item.kind === tab.kind));
    if (existing) {
      setActiveId(existing.id);
      return;
    }
    setTabs((current) => [...current, tab]);
    setActiveId(tab.id);
  }

  function openModule(kind: TabKind, title: string) {
    openTab({ id: kind, kind, title }, true);
  }

  function updateTaxonTab(id: string, taxonId: number, title: string) {
    setTabs((current) => current.map((tab) => tab.id === id && tab.kind === "taxon-detail"
      ? { ...tab, taxonId, title }
      : tab));
  }

  function closeTab(id: string) {
    setTabs((current) => {
      if (current.length === 1) return current;
      viewStateStores.current.delete(id);
      const index = current.findIndex((tab) => tab.id === id);
      const next = current.filter((tab) => tab.id !== id);
      if (activeId === id) setActiveId(next[Math.max(0, index - 1)]?.id ?? next[0].id);
      return next;
    });
  }

  const handlers: PhotoOpenHandlers = useMemo(() => ({
    openDetails: (photo) => openTab({ id: `photo:${photo.photo_id}`, kind: "photo-detail", title: photo.filename, photo }),
    openTaxon: (taxonId) => openTab({ id: `taxon-detail:${crypto.randomUUID()}`, kind: "taxon-detail", title: `Taxon ${taxonId}`, taxonId }),
    openMappingEditor: (photo) => openTab({ id: `mapping:${photo.photo_id}`, kind: "mapping-editor", title: `Map ${photo.filename}`, photo }),
  }), [tabs]);

  function submitSearch(query = searchQuery) {
    const value = query.trim();
    if (!value) return;
    openTab({ id: `search:${value.toLocaleLowerCase()}`, kind: "search-photos", title: `Search: ${value}`, query: value });
    setSearchOpen(false);
    setSuggestionIndex(-1);
  }

  function resetWorkspace(message: string) {
    viewStateStores.current.clear();
    setWorkspaceId((current) => current + 1);
    setTabs([initialTab]);
    setActiveId(initialTab.id);
    setSearchOpen(false);
    setSearchQuery("");
    setSuggestions([]);
    setStatus(message);
  }

  async function chooseRoot() {
    const selected = await selectPhotoDirectory();
    if (!selected) return;
    await openPhotoLibrary(selected);
    setRoot(selected);
    resetWorkspace("Photo root opened");
  }

  return (
    <div className="desktop-shell">
      <aside className="activity-bar">
        <ActivityButton buttonRef={searchToggleRef} icon={Search} label="Search photos" active={searchOpen} onClick={() => setSearchOpen((current) => !current)} />
        <div className="activity-divider" />
        {photoItems.map(([kind, label, icon]) => <ActivityButton key={kind} icon={icon} label={label} active={active?.kind === kind} onClick={() => openModule(kind, label)} />)}
        <div className="activity-divider" />
        <ActivityButton icon={ArrowRightLeft} label="Mapping" active={active?.kind === "mapping"} onClick={() => openModule("mapping", "Mapping")} />
        <div className="activity-divider" />
        {taxonomyItems.map(([kind, label, icon]) => <ActivityButton key={kind} icon={icon} label={label} active={active?.kind === kind} onClick={() => openModule(kind, label)} />)}
        <div className="activity-spacer" />
        <ActivityButton icon={Settings} label="Settings" active={active?.kind === "settings"} onClick={() => openModule("settings", "Settings")} />
      </aside>
      <div className="desktop-main">
        <header className="app-topbar">
          <button className="root-button" type="button" onClick={() => void chooseRoot()} title={root || "Open photo root"}>
            <HardDrive size={14} /><span>{root || "Open photo root"}</span>
          </button>
          <div className="tab-strip">
            {tabs.map((tab) => (
              <button className={`app-tab${tab.id === activeId ? " active" : ""}`} type="button" key={tab.id} onClick={() => setActiveId(tab.id)}>
                <span>{tab.title}</span>
                <i role="button" tabIndex={0} onClick={(event) => {
                  event.stopPropagation();
                  closeTab(tab.id);
                }}><X size={12} /></i>
              </button>
            ))}
          </div>
          {searchOpen && (
            <div className="global-search" ref={searchRef}>
              <label><Search size={15} /><input
                autoFocus
                role="combobox"
                aria-autocomplete="list"
                aria-expanded={suggestions.length > 0}
                aria-activedescendant={suggestionIndex >= 0 ? `photo-suggestion-${suggestionIndex}` : undefined}
                value={searchQuery}
                onChange={(event) => {
                  setSearchQuery(event.target.value);
                  setSuggestionIndex(-1);
                }}
                onKeyDown={(event) => {
                  if (suggestions.length > 0 && event.key === "ArrowDown") {
                    event.preventDefault();
                    setSuggestionIndex((current) => current < suggestions.length - 1 ? current + 1 : 0);
                    return;
                  }
                  if (suggestions.length > 0 && event.key === "ArrowUp") {
                    event.preventDefault();
                    setSuggestionIndex((current) => current > 0 ? current - 1 : suggestions.length - 1);
                    return;
                  }
                  if (suggestions.length > 0 && event.key === "ArrowRight" && suggestionIndex >= 0) {
                    event.preventDefault();
                    setSearchQuery(suggestionLabel(suggestions[suggestionIndex], searchQuery));
                    return;
                  }
                  if (event.key === "Enter") {
                    submitSearch(suggestionIndex >= 0
                      ? suggestionLabel(suggestions[suggestionIndex], searchQuery)
                      : searchQuery);
                  }
                  if (event.key === "Escape") setSearchOpen(false);
                }}
                placeholder="Search filenames and photo taxonomy"
              /></label>
              {suggestions.length > 0 && <div className="suggestions" role="listbox">{suggestions.map((item, index) => (
                <button
                  className={index === suggestionIndex ? "active" : ""}
                  id={`photo-suggestion-${index}`}
                  role="option"
                  aria-selected={index === suggestionIndex}
                  type="button"
                  key={item.taxon_id}
                  onMouseEnter={() => setSuggestionIndex(index)}
                  onClick={() => submitSearch(suggestionLabel(item, searchQuery))}
                >
                  <strong>{item.names.sci_name ?? `Taxon ${item.taxon_id}`}</strong><span>{item.rank} / {item.names.zh_name ?? item.names.en_name ?? ""}</span>
                </button>
              ))}</div>}
            </div>
          )}
        </header>
        <main className="tab-content">
          {tabs.map((tab) => {
            const isActive = tab.id === activeId;
            if (!isActive && !keepAliveTabKinds.has(tab.kind)) return null;
            let viewState = viewStateStores.current.get(tab.id);
            if (!viewState) {
              viewState = new globalThis.Map();
              viewStateStores.current.set(tab.id, viewState);
            }
            return (
              <section
                className={`tab-panel${isActive ? " active" : ""}`}
                aria-hidden={!isActive}
                key={`${workspaceId}:${tab.id}`}
              >
                <ViewStateProvider store={viewState}>
                  <TabBody
                    active={isActive}
                    tab={tab}
                    handlers={handlers}
                    onStatus={setStatus}
                    openTab={openTab}
                    updateTaxonTab={updateTaxonTab}
                    onBaseReplaced={() => resetWorkspace("Base database replaced")}
                  />
                </ViewStateProvider>
              </section>
            );
          })}
        </main>
        <footer className="status-bar"><span className="status-dot" />{status}<span>{active?.title}</span></footer>
      </div>
    </div>
  );
}

function TabBody({
  active,
  tab,
  handlers,
  onStatus,
  openTab,
  updateTaxonTab,
  onBaseReplaced,
}: {
  active: boolean;
  tab: AppTab;
  handlers: PhotoOpenHandlers;
  onStatus: (message: string, busy?: boolean) => void;
  openTab: (tab: AppTab, singleton?: boolean) => void;
  updateTaxonTab: (id: string, taxonId: number, title: string) => void;
  onBaseReplaced: () => void;
}) {
  if (tab.kind === "folders") return <FolderPhotosView handlers={handlers} onStatus={onStatus} />;
  if (tab.kind === "photo-taxonomy") return <TaxonPhotosView handlers={handlers} />;
  if (tab.kind === "map") return <PhotoMapView active={active} handlers={handlers} />;
  if (tab.kind === "photo-history") return <PhotoHistoryView onStatus={onStatus} />;
  if (tab.kind === "mapping") return <MappingView active={active} onStatus={onStatus} handlers={handlers} />;
  if (tab.kind === "taxonomy-search") return <TaxonomySearchView onOpenPhotos={(taxonId, label) => openTab({ id: `taxon-photos:${taxonId}`, kind: "taxon-photos", title: label, taxonId })} />;
  if (tab.kind === "taxon-detail") return <TaxonomySearchView taxonId={tab.taxonId} onTaxonChange={(taxonId, label) => updateTaxonTab(tab.id, taxonId, label)} onOpenPhotos={(taxonId, label) => openTab({ id: `taxon-photos:${taxonId}`, kind: "taxon-photos", title: label, taxonId })} />;
  if (tab.kind === "formatted-update") return <FormattedUpdateView />;
  if (tab.kind === "custom-update") return <CustomUpdateView />;
  if (tab.kind === "taxonomy-history") return <TaxonomyHistoryView />;
  if (tab.kind === "settings") return <SettingsView onBaseReplaced={onBaseReplaced} />;
  if (tab.kind === "photo-detail" && tab.photo) return <PhotoDetailView photo={tab.photo} />;
  if (tab.kind === "mapping-editor" && tab.photo) return <MappingEditor photo={tab.photo} />;
  if (tab.kind === "search-photos" && tab.query) return <PhotoSet query={tab.query} handlers={handlers} />;
  if (tab.kind === "taxon-photos" && tab.taxonId !== undefined) return <PhotoSet taxonId={tab.taxonId} handlers={handlers} />;
  return null;
}

function suggestionLabel(suggestion: TaxonSuggestion, fallback: string) {
  return suggestion.names.sci_name
    ?? suggestion.names.zh_name
    ?? suggestion.names.en_name
    ?? fallback;
}

function PhotoSet({ query, taxonId, handlers }: { query?: string; taxonId?: number; handlers: PhotoOpenHandlers }) {
  const params = query !== undefined
    ? { kind: "search" as const, query }
    : { kind: "taxon" as const, taxonId: taxonId! };
  const page = useCursorPage({
    params,
    resetKey: query !== undefined ? `search:${query}` : `taxon:${taxonId}`,
    stateKey: "photo-set.page",
    loadPage: (next, cursor) => next.kind === "search"
      ? searchPhotos(next.query, cursor)
      : listTaxonPhotos(next.taxonId, cursor),
  });
  return (
    <PhotoBrowser
      title={query !== undefined ? `Search: ${query}` : `Taxon ${taxonId}`}
      detail={page.loading ? "Loading" : page.error || undefined}
      page={page}
      handlers={handlers}
    />
  );
}

function ActivityButton({
  icon: Icon,
  label,
  active,
  onClick,
  buttonRef,
}: {
  icon: IconComponent;
  label: string;
  active: boolean;
  onClick: () => void;
  buttonRef?: RefObject<HTMLButtonElement>;
}) {
  return <button ref={buttonRef} className={`activity-button${active ? " active" : ""}`} type="button" title={label} aria-label={label} onClick={onClick}><Icon size={19} /></button>;
}
