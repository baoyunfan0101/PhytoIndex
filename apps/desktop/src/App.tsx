import {
  Activity,
  ArrowLeft,
  ArrowRightLeft,
  ArrowRight,
  Braces,
  ChevronDown,
  Database,
  FileClock,
  FolderOpen,
  History,
  ListTree,
  Map,
  MoreHorizontal,
  Plus,
  Search,
  Settings,
  TableProperties,
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState, type RefObject } from "react";
import {
  discardBaseImportSession,
  getPhoto,
  listPhotoLibraries,
  listTaxonPhotos,
  openPhotoLibrary,
  photoLibraryAvailabilityLabel,
  searchPhotos,
  selectPhotoDirectory,
  suggestPhotoTaxa,
  switchPhotoLibrary,
  type Photo,
  type PhotoLibraryWorkspace,
  type TaxonSuggestion,
} from "./v3/api";
import type { IconComponent } from "./v3/components";
import { MappingEditor } from "./v3/MappingEditor";
import {
  createNavigationHistory,
  findNavigationTarget,
  pruneNavigationHistory,
  recordNavigation,
} from "./v3/navigationHistory";
import { PhotoBrowser } from "./v3/PhotoBrowser";
import { PhotoDetailView } from "./v3/PhotoDetailView";
import type { PhotoOpenHandlers } from "./v3/PhotoInteraction";
import { emitPhotoMutation, usePhotoMutation } from "./v3/photoMutations";
import {
  FolderPhotosView,
  PhotoMapView,
  TaxonPhotosView,
} from "./v3/PhotosView";
import { MappingView } from "./v3/MappingView";
import { SettingsView } from "./v3/SettingsView";
import {
  FormattedUpdateView,
  TaxonomySearchView,
} from "./v3/TaxonomyView";
import { CustomSqlView } from "./v3/CustomSqlView";
import { OperationHistoryView } from "./v3/OperationHistoryView";
import { EmptyState } from "./v3/components";
import { useOperationObserver } from "./v3/useOperationObserver";
import { useCursorPage } from "./v3/useCursorPage";
import { ViewStateProvider, type ViewStateStore } from "./v3/viewState";
import {
  dependsOnReplacedTaxonomy,
  retainTabsAfterTaxonomyReplacement,
} from "./v3/taxonomyReplacement";

type TabKind =
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
  "custom-sql",
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
  ["custom-sql", "Custom SQL", Braces],
  ["taxonomy-history", "Update history", FileClock],
];

const photoTabKinds = new Set<TabKind>([
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

export function App() {
  const [tabs, setTabs] = useState<AppTab[]>([initialTab]);
  const [activeId, setActiveId] = useState(initialTab.id);
  const [libraries, setLibraries] = useState<PhotoLibraryWorkspace[]>([]);
  const [workspaceLoading, setWorkspaceLoading] = useState(true);
  const [libraryMenuOpen, setLibraryMenuOpen] = useState(false);
  const [operationsOpen, setOperationsOpen] = useState(false);
  const [moreOpen, setMoreOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [suggestions, setSuggestions] = useState<TaxonSuggestion[]>([]);
  const [suggestionIndex, setSuggestionIndex] = useState(-1);
  const [status, setStatus] = useState("Ready");
  const searchRef = useRef<HTMLDivElement>(null);
  const searchToggleRef = useRef<HTMLButtonElement>(null);
  const viewStateStores = useRef(new globalThis.Map<string, ViewStateStore>());
  const [navigationHistory, setNavigationHistory] = useState(
    createNavigationHistory(initialTab.id),
  );
  const operations = useOperationObserver();
  const active = tabs.find((tab) => tab.id === activeId) ?? tabs[0];
  const activeLibrary = libraries.find((library) => library.active) ?? null;
  const workspaceAvailable = Boolean(
    activeLibrary?.root_available && activeLibrary.database_available,
  );
  const runningOperations = Object.values(operations).filter((operation) => operation.running);
  const taxonomyMutationLocked = runningOperations.some(
    (operation) => operation.operation === "apply_base_import",
  );
  const existingTabIds = useMemo(
    () => new Set(tabs.map((tab) => tab.id)),
    [tabs],
  );
  const backTarget = findNavigationTarget(navigationHistory, existingTabIds, -1);
  const forwardTarget = findNavigationTarget(navigationHistory, existingTabIds, 1);

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

  useEffect(() => { void reloadLibraries(); }, []);

  useEffect(() => {
    setNavigationHistory((current) => pruneNavigationHistory(
      current,
      existingTabIds,
      active?.id ?? null,
    ));
  }, [active?.id, existingTabIds]);

  useEffect(() => {
    const value = searchQuery.trim();
    if (!searchOpen || !value) {
      setSuggestions([]);
      setSuggestionIndex(-1);
      return;
    }
    if (!workspaceAvailable) return;
    const timer = window.setTimeout(() => void suggestPhotoTaxa(value)
      .then((next) => {
        setSuggestions(next);
        setSuggestionIndex(-1);
      })
      .catch(() => setSuggestions([])), 140);
    return () => window.clearTimeout(timer);
  }, [searchOpen, searchQuery, workspaceAvailable]);

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
      focusTab(existing.id);
      return;
    }
    setTabs((current) => [...current, tab]);
    focusTab(tab.id);
  }

  function focusTab(id: string, record = true) {
    setActiveId(id);
    if (record) {
      setNavigationHistory((current) => recordNavigation(current, id));
    }
  }

  function navigate(offset: -1 | 1) {
    const target = findNavigationTarget(navigationHistory, existingTabIds, offset);
    if (!target) return;
    setNavigationHistory((current) => ({ ...current, index: target.index }));
    focusTab(target.tabId, false);
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
    const closingTab = tabs.find((tab) => tab.id === id);
    const viewState = viewStateStores.current.get(id);
    const baseImportSession = closingTab?.kind === "settings"
      ? viewState?.get("base-import.session-id")
      : null;
    if (
      typeof baseImportSession === "string"
      && !window.confirm("Discard the unfinished Base Import session and close Settings?")
    ) {
      return;
    }
    if (typeof baseImportSession === "string") {
      void discardBaseImportSession(baseImportSession).catch((nextError) => setStatus(String(nextError)));
    }
    if (tabs.length === 1) return;
    viewStateStores.current.delete(id);
    const index = tabs.findIndex((tab) => tab.id === id);
    const next = tabs.filter((tab) => tab.id !== id);
    setTabs(next);
    if (activeId === id) focusTab(next[Math.max(0, index - 1)]?.id ?? next[0].id);
  }

  const handlers: PhotoOpenHandlers = useMemo(() => ({
    openDetails: (photo) => openTab({ id: `photo:${photo.photo_id}`, kind: "photo-detail", title: photo.filename, photo }),
    openTaxon: (taxonId) => openTab({ id: `taxon-detail:${crypto.randomUUID()}`, kind: "taxon-detail", title: `Taxon ${taxonId}`, taxonId }),
    openMappingEditor: (photo) => openTab({ id: `mapping:${photo.photo_id}`, kind: "mapping-editor", title: `Map ${photo.filename}`, photo }),
  }), [tabs]);

  function submitSearch(query = searchQuery) {
    const value = query.trim();
    if (!value || !workspaceAvailable) return;
    openTab({ id: `search:${value.toLocaleLowerCase()}`, kind: "search-photos", title: `Search: ${value}`, query: value });
    setSearchOpen(false);
    setSuggestionIndex(-1);
  }

  function resetPhotoWorkspace(message: string) {
    setTabs((current) => {
      const remaining = current.filter((tab) => !photoTabKinds.has(tab.kind));
      current.filter((tab) => photoTabKinds.has(tab.kind)).forEach((tab) => viewStateStores.current.delete(tab.id));
      if (remaining.length === 0 || photoTabKinds.has(active?.kind)) {
        const folders = { ...initialTab };
        setActiveId(folders.id);
        return [...remaining, folders];
      }
      return remaining;
    });
    setSearchOpen(false);
    setSearchQuery("");
    setSuggestions([]);
    setStatus(message);
  }

  async function reloadLibraries() {
    setWorkspaceLoading(true);
    try {
      setLibraries(await listPhotoLibraries());
    } catch (nextError) {
      setStatus(String(nextError));
    } finally {
      setWorkspaceLoading(false);
    }
  }

  async function createLibrary() {
    const selected = await selectPhotoDirectory();
    if (!selected) return;
    await openPhotoLibrary(selected);
    await reloadLibraries();
    setLibraryMenuOpen(false);
    resetPhotoWorkspace("Photo Library opened");
  }

  async function selectLibrary(library: PhotoLibraryWorkspace) {
    if (library.active) {
      setLibraryMenuOpen(false);
      return;
    }
    if (!library.root_available || !library.database_available) {
      setLibraryMenuOpen(false);
      setStatus(`${library.display_name} is unavailable; rebind it in Settings`);
      openModule("settings", "Settings");
      return;
    }
    try {
      await switchPhotoLibrary(library.library_uuid);
      await reloadLibraries();
      setLibraryMenuOpen(false);
      resetPhotoWorkspace(`Opened ${library.display_name}`);
    } catch (nextError) {
      setStatus(String(nextError));
      await reloadLibraries();
    }
  }

  function resetTaxonomyResources() {
    setTabs((current) => {
      current.filter((tab) => dependsOnReplacedTaxonomy(tab.kind)).forEach((tab) => viewStateStores.current.delete(tab.id));
      const remaining = retainTabsAfterTaxonomyReplacement(current);
      if (remaining.every((tab) => tab.id !== activeId)) {
        setActiveId(remaining[0]?.id ?? initialTab.id);
      }
      return remaining.length > 0 ? remaining : [{ ...initialTab }];
    });
    emitPhotoMutation({ photoId: null, kind: "mapping" });
    setStatus("Taxonomy database replaced successfully. Photo mappings are being rebuilt in the background.");
  }

  return (
    <div className="desktop-shell">
      <aside className="activity-bar">
        <ActivityButton buttonRef={searchToggleRef} icon={Search} label="Search photos" active={searchOpen} disabled={!workspaceAvailable} onClick={() => setSearchOpen((current) => !current)} />
        <div className="activity-divider" />
        {photoItems.map(([kind, label, icon]) => <ActivityButton key={kind} icon={icon} label={label} active={active?.kind === kind} disabled={!workspaceAvailable} onClick={() => openModule(kind, label)} />)}
        <div className="activity-divider" />
        <ActivityButton icon={ArrowRightLeft} label="Mapping" active={active?.kind === "mapping"} disabled={!workspaceAvailable} onClick={() => openModule("mapping", "Mapping")} />
        <div className="activity-divider" />
        {taxonomyItems.map(([kind, label, icon]) => <ActivityButton key={kind} icon={icon} label={label} active={active?.kind === kind} onClick={() => openModule(kind, label)} />)}
        <div className="activity-spacer" />
        <ActivityButton icon={Settings} label="Settings" active={active?.kind === "settings"} onClick={() => openModule("settings", "Settings")} />
      </aside>
      <div className="desktop-main">
        <header className="app-topbar">
          <div className="toolbar-navigation">
            <button type="button" title="Go Back" disabled={!backTarget} onClick={() => navigate(-1)}><ArrowLeft size={14} /></button>
            <button type="button" title="Go Forward" disabled={!forwardTarget} onClick={() => navigate(1)}><ArrowRight size={14} /></button>
          </div>
          <div className="library-selector">
            <button
              className={`library-selector-button${workspaceAvailable ? "" : " unavailable"}`}
              type="button"
              onClick={() => setLibraryMenuOpen((current) => !current)}
              title={activeLibrary?.root_path ?? "Select a Photo Library"}
            >
              <Database size={14} />
              <span>{workspaceLoading ? "Loading libraries" : activeLibrary?.display_name ?? "No Photo Library"}</span>
              <ChevronDown size={13} />
            </button>
            {libraryMenuOpen && (
              <div className="toolbar-popover library-popover">
                <strong>Photo Libraries</strong>
                {libraries.length === 0 && <span>No registered libraries</span>}
                {libraries.map((library) => (
                  <button
                    className={library.active ? "active" : ""}
                    type="button"
                    key={library.library_uuid}
                    onClick={() => void selectLibrary(library)}
                  >
                    <i className={library.root_available && library.database_available ? "available" : "unavailable"} />
                      <span><b>{library.display_name}</b><small>{library.root_path}</small></span>
                      <em>{photoLibraryAvailabilityLabel(library)}</em>
                  </button>
                ))}
                <button className="popover-action" type="button" onClick={() => void createLibrary()}><Plus size={13} />Create or open library</button>
                <button className="popover-action" type="button" onClick={() => {
                  setLibraryMenuOpen(false);
                  openModule("settings", "Settings");
                }}><Settings size={13} />Manage libraries</button>
              </div>
            )}
          </div>
          <div className="tab-strip">
            {tabs.map((tab) => (
              <button className={`app-tab${tab.id === activeId ? " active" : ""}`} type="button" key={tab.id} onClick={() => focusTab(tab.id)}>
                <span>{tab.title}</span>
                <i role="button" tabIndex={0} onClick={(event) => {
                  event.stopPropagation();
                  closeTab(tab.id);
                }}><X size={12} /></i>
              </button>
            ))}
          </div>
          <div className="toolbar-actions">
            <button className={runningOperations.length > 0 ? "running" : ""} type="button" title="Background operations" onClick={() => setOperationsOpen((current) => !current)}>
              <Activity size={15} /><span>{runningOperations.length}</span>
            </button>
            <button type="button" title="More actions" onClick={() => setMoreOpen((current) => !current)}><MoreHorizontal size={16} /></button>
            {operationsOpen && (
              <div className="toolbar-popover operations-popover">
                <strong>Background operations</strong>
                {Object.values(operations).length === 0 && <span>No operations</span>}
                {Object.values(operations).map((operation) => (
                  <div key={operation.module}>
                    <b>{operation.module}</b>
                    <span>{operation.message}</span>
                    {operation.running && <progress value={operation.processed} max={operation.total ?? undefined} />}
                    {operation.error && <small>{operation.error}</small>}
                  </div>
                ))}
              </div>
            )}
            {moreOpen && (
              <div className="toolbar-popover more-popover">
                <button type="button" onClick={() => {
                  setMoreOpen(false);
                  openModule("settings", "Settings");
                }}><Settings size={13} />Settings</button>
                <button type="button" onClick={() => {
                  setMoreOpen(false);
                  void reloadLibraries();
                }}><Database size={13} />Refresh libraries</button>
              </div>
            )}
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
                key={tab.id}
              >
                <ViewStateProvider store={viewState}>
                  <TabBody
                    active={isActive}
                    tab={tab}
                    handlers={handlers}
                    onStatus={setStatus}
                    openTab={openTab}
                    updateTaxonTab={updateTaxonTab}
                    workspaceAvailable={workspaceAvailable}
                    activeLibrary={activeLibrary}
                    taxonomyMutationLocked={taxonomyMutationLocked}
                    onCreateLibrary={() => void createLibrary()}
                    onWorkspaceChanged={(resetPhotoTabs) => {
                      void reloadLibraries();
                      if (resetPhotoTabs) resetPhotoWorkspace("Photo Library workspace changed");
                    }}
                    onBaseReplaced={resetTaxonomyResources}
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
  workspaceAvailable,
  activeLibrary,
  taxonomyMutationLocked,
  onCreateLibrary,
  onWorkspaceChanged,
  onBaseReplaced,
}: {
  active: boolean;
  tab: AppTab;
  handlers: PhotoOpenHandlers;
  onStatus: (message: string, busy?: boolean) => void;
  openTab: (tab: AppTab, singleton?: boolean) => void;
  updateTaxonTab: (id: string, taxonId: number, title: string) => void;
  workspaceAvailable: boolean;
  activeLibrary: PhotoLibraryWorkspace | null;
  taxonomyMutationLocked: boolean;
  onCreateLibrary: () => void;
  onWorkspaceChanged: (resetPhotoTabs: boolean) => void;
  onBaseReplaced: () => void;
}) {
  if (photoTabKinds.has(tab.kind) && !workspaceAvailable) {
    return (
      <EmptyState
        title={activeLibrary ? "Photo Library unavailable" : "No Photo Library selected"}
        detail={activeLibrary
          ? "Reconnect its photo root or database in Settings, then open the library again."
          : "Create or select a Photo Library from the top toolbar."}
        icon={Database}
        action={(
          <div className="empty-state-actions">
            {!activeLibrary && <button className="primary-button" type="button" onClick={onCreateLibrary}>Create or open</button>}
            <button className="secondary-button" type="button" onClick={() => openTab({ id: "settings", kind: "settings", title: "Settings" }, true)}>Manage libraries</button>
          </div>
        )}
      />
    );
  }
  if (tab.kind === "folders") return <FolderPhotosView handlers={handlers} onStatus={onStatus} />;
  if (tab.kind === "photo-taxonomy") return <TaxonPhotosView handlers={handlers} />;
  if (tab.kind === "map") return <PhotoMapView active={active} handlers={handlers} />;
  if (tab.kind === "photo-history") return <OperationHistoryView domain="photo" onStatus={onStatus} />;
  if (tab.kind === "mapping") return <MappingView active={active} onStatus={onStatus} handlers={handlers} />;
  if (tab.kind === "taxonomy-search") return <TaxonomySearchView onOpenPhotos={(taxonId, label) => openTab({ id: `taxon-photos:${taxonId}`, kind: "taxon-photos", title: label, taxonId })} />;
  if (tab.kind === "taxon-detail") return <TaxonomySearchView taxonId={tab.taxonId} onTaxonChange={(taxonId, label) => updateTaxonTab(tab.id, taxonId, label)} onOpenPhotos={(taxonId, label) => openTab({ id: `taxon-photos:${taxonId}`, kind: "taxon-photos", title: label, taxonId })} />;
  if (tab.kind === "formatted-update") return <FormattedUpdateView mutationDisabled={taxonomyMutationLocked} />;
  if (tab.kind === "custom-sql") return <CustomSqlView onStatus={onStatus} mutationDisabled={taxonomyMutationLocked} />;
  if (tab.kind === "taxonomy-history") return <OperationHistoryView domain="taxonomy" onStatus={onStatus} />;
  if (tab.kind === "settings") return <SettingsView onBaseReplaced={onBaseReplaced} onWorkspaceChanged={onWorkspaceChanged} />;
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
  disabled = false,
}: {
  icon: IconComponent;
  label: string;
  active: boolean;
  onClick: () => void;
  buttonRef?: RefObject<HTMLButtonElement>;
  disabled?: boolean;
}) {
  return <button ref={buttonRef} className={`activity-button${active ? " active" : ""}`} type="button" title={label} aria-label={label} disabled={disabled} onClick={onClick}><Icon size={19} /></button>;
}
