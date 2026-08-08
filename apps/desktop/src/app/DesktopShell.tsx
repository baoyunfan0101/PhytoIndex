import {
  Activity,
  ArrowDownUp,
  ArrowLeft,
  ArrowRight,
  Code,
  Database,
  DatabaseSearch,
  FileClock,
  Folder,
  FolderClock,
  MapPinned,
  Network,
  Search,
  Settings,
  TableProperties,
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  getWorkspaceState,
  saveWorkspaceState,
  type GeneralSettings,
} from "../api/general";
import {
  getPhoto,
  type Photo,
} from "../api/photos";
import {
  listPhotoLibraries,
  openTaxonomyDatabase,
  openPhotoLibrary,
  type PhotoLibraryWorkspace,
} from "../api/storage";
import { selectPhotoDirectory, selectSqliteDatabase } from "../api/dialogs";
import { Button, EmptyState, IconButton, type IconComponent } from "../shared/ui";
import { MappingEditor } from "../features/mapping/MappingEditor";
import {
  createNavigationHistory,
  findNavigationTarget,
  pruneNavigationHistory,
  recordNavigation,
} from "./navigationHistory";
import { PhotoDetailView } from "../features/photos/PhotoDetailView";
import { PhotoSet } from "../features/photos/PhotoSet";
import { EmptyWorkspace } from "../features/photos/search/EmptyWorkspace";
import { GlobalSearchOverlay } from "../features/photos/search/GlobalSearchOverlay";
import {
  addRecentSearch,
  loadRecentSearches,
  normalizeSearchQuery,
  removeRecentSearch,
  saveRecentSearches,
  trimRecentSearches,
} from "../features/photos/search/recentSearchStorage";
import type { PhotoOpenHandlers } from "../features/photos/PhotoInteraction";
import { emitPhotoMutation, usePhotoMutation } from "../features/photos/photoMutations";
import {
  FolderPhotosView,
  PhotoMapView,
  TaxonPhotosView,
} from "../features/photos/PhotosView";
import { MappingView } from "../features/mapping/MappingView";
import { SettingsView, type SettingsSection } from "../features/settings/SettingsView";
import {
  FormattedUpdateView,
  TaxonomySearchView,
} from "../features/taxonomy/TaxonomyView";
import { CustomSqlView } from "../features/taxonomy/CustomSqlView";
import { OperationHistoryView } from "../features/operations/OperationHistoryView";
import { useOperationObserver } from "./useOperationObserver";
import { ViewStateProvider, type ViewStateStore } from "../shared/viewState";
import {
  dependsOnReplacedTaxonomy,
  retainTabsAfterTaxonomyReplacement,
} from "./taxonomyReplacement";
import { closeAllTabsState, closeTabState } from "./tabState";
import { nativeMenuActions, useNativeMenu } from "./nativeMenu";
import { NativeAboutOverlay } from "./NativeAboutOverlay";
import { getTaxonDetailNode } from "../api/taxonomy";
import {
  restoreWorkspaceState,
  serializeWorkspaceState,
  type AppTab,
} from "./workspaceState";

type TabKind = AppTab["kind"];

const initialTab: AppTab = { id: "folders", kind: "folders", title: "Folders" };
const keepAliveTabKinds = new Set<TabKind>([
  "map",
  "formatted-update",
  "custom-sql",
  "settings",
  "mapping",
]);

const photoItems: Array<[TabKind, string, IconComponent]> = [
  ["folders", "Folders", Folder],
  ["photo-taxonomy", "Taxon Tree", Network],
  ["map", "Map", MapPinned],
  ["photo-history", "Rename history", FolderClock],
];
const taxonomyItems: Array<[TabKind, string, IconComponent]> = [
  ["taxonomy-search", "Taxonomy search", DatabaseSearch],
  ["formatted-update", "Formatted update", TableProperties],
  ["custom-sql", "Custom SQL", Code],
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

export function DesktopShell({
  generalSettings,
  generalSettingsLoadError,
  onGeneralSettingsChange,
}: {
  generalSettings: GeneralSettings;
  generalSettingsLoadError?: string;
  onGeneralSettingsChange: (settings: GeneralSettings) => void;
}) {
  const [tabs, setTabs] = useState<AppTab[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [libraries, setLibraries] = useState<PhotoLibraryWorkspace[]>([]);
  const [workspaceReady, setWorkspaceReady] = useState(false);
  const [operationsOpen, setOperationsOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [recentSearches, setRecentSearches] = useState(() => (
    loadRecentSearches(undefined, generalSettings.recent_searches_limit)
  ));
  const [status, setStatus] = useState("Ready");
  const emptySearchInputRef = useRef<HTMLInputElement>(null);
  const operationsMenuRef = useRef<HTMLDivElement>(null);
  const viewStateStores = useRef(new globalThis.Map<string, ViewStateStore>());
  const [navigationHistory, setNavigationHistory] = useState(
    createNavigationHistory(null),
  );
  const operations = useOperationObserver();
  const active = activeId === null ? null : tabs.find((tab) => tab.id === activeId) ?? null;
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

  useEffect(() => {
    let cancelled = false;
    async function bootstrap() {
      let nextLibraries: PhotoLibraryWorkspace[] = [];
      try {
        nextLibraries = await listPhotoLibraries();
      } catch (nextError) {
        if (!cancelled) setStatus(String(nextError));
      }
      if (cancelled) return;
      setLibraries(nextLibraries);
      if (generalSettings.restore_tabs) {
        try {
          const activeLibrary = nextLibraries.find((library) => library.active) ?? null;
          const restored = await restoreWorkspaceState(await getWorkspaceState(), {
            getPhoto,
            photoWorkspaceAvailable: Boolean(
              activeLibrary?.root_available && activeLibrary.database_available,
            ),
            taxonExists: async (taxonId) => {
              try {
                await getTaxonDetailNode(taxonId);
                return true;
              } catch {
                return false;
              }
            },
          });
          if (cancelled) return;
          setTabs(restored.tabs);
          setActiveId(restored.activeId);
          setNavigationHistory(createNavigationHistory(restored.activeId));
        } catch (nextError) {
          if (!cancelled) setStatus(`Workspace state could not be restored: ${String(nextError)}`);
        }
      }
      if (!cancelled) setWorkspaceReady(true);
    }
    void bootstrap();
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    const next = trimRecentSearches(
      recentSearches,
      generalSettings.recent_searches_limit,
    );
    if (next.length === recentSearches.length) return;
    setRecentSearches(next);
    saveRecentSearches(next);
  }, [generalSettings.recent_searches_limit, recentSearches]);

  useEffect(() => {
    if (!workspaceReady || !generalSettings.restore_tabs) return;
    const timer = window.setTimeout(() => {
      void saveWorkspaceState(serializeWorkspaceState(tabs, activeId))
        .catch((nextError) => setStatus(`Workspace state could not be saved: ${String(nextError)}`));
    }, 250);
    return () => window.clearTimeout(timer);
  }, [activeId, generalSettings.restore_tabs, tabs, workspaceReady]);

  useEffect(() => {
    setNavigationHistory((current) => pruneNavigationHistory(
      current,
      existingTabIds,
      active?.id ?? null,
    ));
  }, [active?.id, existingTabIds]);

  useEffect(() => {
    if (!operationsOpen) return;
    const closeMenuOnOutsidePointer = (event: PointerEvent) => {
      const target = event.target as Node;
      if (operationsOpen && !operationsMenuRef.current?.contains(target)) {
        setOperationsOpen(false);
      }
    };
    window.addEventListener("pointerdown", closeMenuOnOutsidePointer);
    return () => window.removeEventListener("pointerdown", closeMenuOnOutsidePointer);
  }, [operationsOpen]);

  function openTab(tab: AppTab, singleton = false) {
    const existing = tabs.find((item) => item.id === tab.id || (singleton && item.kind === tab.kind));
    if (existing) {
      if (tab.kind === "settings" && tab.settingsSection !== undefined) {
        setTabs((current) => current.map((item) => item.id === existing.id
          ? { ...item, settingsSection: tab.settingsSection }
          : item));
      }
      focusTab(existing.id);
      return;
    }
    setTabs((current) => [...current, tab]);
    focusTab(tab.id);
  }

  function focusTab(id: string | null, record = true) {
    setActiveId(id);
    if (record && id !== null) {
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

  function updateSettingsTab(id: string, settingsSection: SettingsSection) {
    setTabs((current) => current.map((tab) => tab.id === id && tab.kind === "settings"
      ? { ...tab, settingsSection }
      : tab));
  }

  function closeTab(id: string) {
    const remainingIds = new Set(tabs
      .filter((tab) => tab.id !== id)
      .map((tab) => tab.id));
    const previousActiveId = activeId === id
      ? findNavigationTarget(navigationHistory, remainingIds, -1)?.tabId ?? null
      : null;
    const next = closeTabState(tabs, activeId, id, previousActiveId);
    if (next.tabs === tabs) return;
    viewStateStores.current.delete(id);
    setTabs(next.tabs);
    if (activeId !== next.activeId) focusTab(next.activeId);
    if (next.activeId === null) setSearchOpen(false);
  }

  const handlers: PhotoOpenHandlers = useMemo(() => ({
    openDetails: (photo) => openTab({ id: `photo:${photo.photo_id}`, kind: "photo-detail", title: photo.filename, photo }),
    openTaxon: (taxonId) => openTab({ id: `taxon-detail:${crypto.randomUUID()}`, kind: "taxon-detail", title: `Taxon ${taxonId}`, taxonId }),
    openMappingEditor: (photo) => openTab({ id: `mapping:${photo.photo_id}`, kind: "mapping-editor", title: `Map ${photo.filename}`, photo }),
  }), [tabs]);

  async function submitSearch(query: string) {
    const value = normalizeSearchQuery(query);
    if (!value) return;
    if (!workspaceAvailable) throw new Error("Photo Library unavailable");
    openTab({ id: `search:${value.toLocaleLowerCase()}`, kind: "search-photos", title: `Search: ${value}`, query: value });
    setRecentSearches((current) => {
      const next = addRecentSearch(
        current,
        value,
        generalSettings.recent_searches_limit,
      );
      saveRecentSearches(next);
      return next;
    });
    setSearchOpen(false);
  }

  function deleteRecentSearch(query: string) {
    setRecentSearches((current) => {
      const next = removeRecentSearch(current, query);
      saveRecentSearches(next);
      return next;
    });
  }

  function clearRecentSearches() {
    setRecentSearches([]);
    saveRecentSearches([]);
  }

  function openGlobalSearch() {
    if (tabs.length === 0) {
      setSearchOpen(false);
      window.requestAnimationFrame(() => emptySearchInputRef.current?.focus());
      return;
    }
    setSearchOpen(true);
  }

  function resetPhotoWorkspace(message: string) {
    setTabs((current) => {
      const remaining = current.filter((tab) => !photoTabKinds.has(tab.kind));
      current.filter((tab) => photoTabKinds.has(tab.kind)).forEach((tab) => viewStateStores.current.delete(tab.id));
      if (remaining.length === 0 || (active !== null && photoTabKinds.has(active.kind))) {
        const folders = { ...initialTab };
        setActiveId(folders.id);
        return [...remaining, folders];
      }
      return remaining;
    });
    setSearchOpen(false);
    setStatus(message);
  }

  async function reloadLibraries() {
    try {
      setLibraries(await listPhotoLibraries());
    } catch (nextError) {
      setStatus(String(nextError));
    }
  }

  async function createLibrary() {
    const selected = await selectPhotoDirectory();
    if (!selected) return;
    try {
      await openPhotoLibrary(selected);
      await reloadLibraries();
      resetPhotoWorkspace("Photo Library opened");
    } catch (nextError) {
      setStatus(String(nextError));
    }
  }

  async function openExistingTaxonomyDatabase() {
    const selected = await selectSqliteDatabase();
    if (!selected) return;
    try {
      await openTaxonomyDatabase(selected);
      resetTaxonomyResources("Taxonomy Database opened. Photo mappings are being rebuilt in the background.");
    } catch (nextError) {
      setStatus(String(nextError));
    }
  }

  function closeAllTabs() {
    const next = closeAllTabsState<AppTab>();
    viewStateStores.current.clear();
    setTabs(next.tabs);
    focusTab(next.activeId);
    setOperationsOpen(false);
    setAboutOpen(false);
    setSearchOpen(false);
  }

  function resetTaxonomyResources(message = "Taxonomy database replaced successfully. Photo mappings are being rebuilt in the background.") {
    setTabs((current) => {
      current.filter((tab) => dependsOnReplacedTaxonomy(tab.kind)).forEach((tab) => viewStateStores.current.delete(tab.id));
      const remaining = retainTabsAfterTaxonomyReplacement(current);
      if (remaining.every((tab) => tab.id !== activeId)) {
        setActiveId(remaining[0]?.id ?? initialTab.id);
      }
      return remaining.length > 0 ? remaining : [{ ...initialTab }];
    });
    emitPhotoMutation({ photoId: null, kind: "mapping" });
    setStatus(message);
  }

  useNativeMenu((action) => {
    if (action === nativeMenuActions.aboutVividarium) {
      setSearchOpen(false);
      setAboutOpen(true);
    }
    if (action === nativeMenuActions.openPhotoLibrary) void createLibrary();
    if (action === nativeMenuActions.managePhotoLibraries) {
      openTab({ id: "settings", kind: "settings", title: "Settings", settingsSection: "Photo Libraries" }, true);
    }
    if (action === nativeMenuActions.openTaxonomyDatabase) void openExistingTaxonomyDatabase();
    if (action === nativeMenuActions.manageTaxonomyDatabases) {
      openTab({ id: "settings", kind: "settings", title: "Settings", settingsSection: "Taxonomy Databases" }, true);
    }
    if (action === nativeMenuActions.closeAllTabs) closeAllTabs();
  });

  return (
    <div className="desktop-shell">
      <aside className="activity-bar">
        <ActivityButton icon={Search} label="Search photos" active={searchOpen || active === null} onClick={openGlobalSearch} />
        <div className="activity-divider" />
        {photoItems.map(([kind, label, icon]) => <ActivityButton key={kind} icon={icon} label={label} active={active?.kind === kind} disabled={!workspaceAvailable} onClick={() => openModule(kind, label)} />)}
        <div className="activity-divider" />
        <ActivityButton icon={ArrowDownUp} label="Mapping" active={active?.kind === "mapping"} disabled={!workspaceAvailable} onClick={() => openModule("mapping", "Mapping")} />
        <div className="activity-divider" />
        {taxonomyItems.map(([kind, label, icon]) => <ActivityButton key={kind} icon={icon} label={label} active={active?.kind === kind} onClick={() => openModule(kind, label)} />)}
        <div className="activity-spacer" />
        <ActivityButton icon={Settings} label="Settings" active={active?.kind === "settings"} onClick={() => openModule("settings", "Settings")} />
      </aside>
      <div className="desktop-main">
        <header className="app-topbar">
          <div className="toolbar-navigation">
            <IconButton aria-label="Go Back" title="Go Back" disabled={!backTarget} onClick={() => navigate(-1)}><ArrowLeft size={14} /></IconButton>
            <IconButton aria-label="Go Forward" title="Go Forward" disabled={!forwardTarget} onClick={() => navigate(1)}><ArrowRight size={14} /></IconButton>
          </div>
          <div className="tab-strip" role="tablist" aria-label="Open tabs">
            {tabs.map((tab) => (
              <div
                aria-selected={tab.id === activeId}
                className={`app-tab${tab.id === activeId ? " active" : ""}`}
                key={tab.id}
                role="tab"
                tabIndex={0}
                onClick={() => focusTab(tab.id)}
                onKeyDown={(event) => {
                  if (event.target !== event.currentTarget) return;
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    focusTab(tab.id);
                  }
                }}
              >
                <span className="app-tab-title">{tab.title}</span>
                <IconButton
                  aria-label={`Close ${tab.title}`}
                  className="app-tab-close"
                  size="small"
                  title={`Close ${tab.title}`}
                  onClick={(event) => {
                    event.stopPropagation();
                    closeTab(tab.id);
                  }}
                ><X size={12} /></IconButton>
              </div>
            ))}
          </div>
        </header>
        <main className="tab-content">
          {workspaceReady && tabs.length === 0 && (
            <EmptyWorkspace
              inputRef={emptySearchInputRef}
              recentSearches={recentSearches}
              suggestionsEnabled={workspaceAvailable}
              onSubmit={submitSearch}
              onRemoveRecent={deleteRecentSearch}
              onClearRecent={clearRecentSearches}
            />
          )}
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
                    updateSettingsTab={updateSettingsTab}
                    workspaceAvailable={workspaceAvailable}
                    activeLibrary={activeLibrary}
                    taxonomyMutationLocked={taxonomyMutationLocked}
                    onCreateLibrary={() => void createLibrary()}
                    onWorkspaceChanged={(resetPhotoTabs) => {
                      void reloadLibraries();
                      if (resetPhotoTabs) resetPhotoWorkspace("Photo Library workspace changed");
                    }}
                    onBaseReplaced={resetTaxonomyResources}
                    generalSettings={generalSettings}
                    generalSettingsLoadError={generalSettingsLoadError}
                    onGeneralSettingsChange={onGeneralSettingsChange}
                  />
                </ViewStateProvider>
              </section>
            );
          })}
        </main>
        <footer className="status-bar">
          <span className="status-dot" />
          {status}
          <span className="status-title">{active?.title ?? ""}</span>
          <div className="status-operations" ref={operationsMenuRef}>
            <IconButton aria-label="Background operations" className={runningOperations.length > 0 ? "running" : ""} title="Background operations" onClick={() => {
              setOperationsOpen((current) => !current);
            }}>
              <Activity size={12} /><span>{runningOperations.length}</span>
            </IconButton>
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
          </div>
        </footer>
      </div>
      {searchOpen && active !== null && (
        <GlobalSearchOverlay
          suggestionsEnabled={workspaceAvailable}
          onClose={() => setSearchOpen(false)}
          onSubmit={submitSearch}
        />
      )}
      {aboutOpen && <NativeAboutOverlay onClose={() => setAboutOpen(false)} />}
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
  updateSettingsTab,
  workspaceAvailable,
  activeLibrary,
  taxonomyMutationLocked,
  onCreateLibrary,
  onWorkspaceChanged,
  onBaseReplaced,
  generalSettings,
  generalSettingsLoadError,
  onGeneralSettingsChange,
}: {
  active: boolean;
  tab: AppTab;
  handlers: PhotoOpenHandlers;
  onStatus: (message: string, busy?: boolean) => void;
  openTab: (tab: AppTab, singleton?: boolean) => void;
  updateTaxonTab: (id: string, taxonId: number, title: string) => void;
  updateSettingsTab: (id: string, section: SettingsSection) => void;
  workspaceAvailable: boolean;
  activeLibrary: PhotoLibraryWorkspace | null;
  taxonomyMutationLocked: boolean;
  onCreateLibrary: () => void;
  onWorkspaceChanged: (resetPhotoTabs: boolean) => void;
  onBaseReplaced: () => void;
  generalSettings: GeneralSettings;
  generalSettingsLoadError?: string;
  onGeneralSettingsChange: (settings: GeneralSettings) => void;
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
            {!activeLibrary && <Button variant="primary" onClick={onCreateLibrary}>Create or open</Button>}
            <Button onClick={() => openTab({ id: "settings", kind: "settings", title: "Settings", settingsSection: "Photo Libraries" }, true)}>Manage libraries</Button>
          </div>
        )}
      />
    );
  }
  if (tab.kind === "folders") return <FolderPhotosView handlers={handlers} onStatus={onStatus} />;
  if (tab.kind === "photo-taxonomy") return <TaxonPhotosView handlers={handlers} nameParts={generalSettings.taxon_tree_name_parts} />;
  if (tab.kind === "map") return <PhotoMapView active={active} handlers={handlers} />;
  if (tab.kind === "photo-history") return <OperationHistoryView domain="photo" onStatus={onStatus} />;
  if (tab.kind === "mapping") return <MappingView active={active} onStatus={onStatus} handlers={handlers} />;
  if (tab.kind === "taxonomy-search") return <TaxonomySearchView onOpenPhotos={(taxonId, label) => openTab({ id: `taxon-photos:${taxonId}`, kind: "taxon-photos", title: label, taxonId })} />;
  if (tab.kind === "taxon-detail") return <TaxonomySearchView taxonId={tab.taxonId} onTaxonChange={(taxonId, label) => updateTaxonTab(tab.id, taxonId, label)} onOpenPhotos={(taxonId, label) => openTab({ id: `taxon-photos:${taxonId}`, kind: "taxon-photos", title: label, taxonId })} />;
  if (tab.kind === "formatted-update") return <FormattedUpdateView mutationDisabled={taxonomyMutationLocked} />;
  if (tab.kind === "custom-sql") return <CustomSqlView onStatus={onStatus} mutationDisabled={taxonomyMutationLocked} />;
  if (tab.kind === "taxonomy-history") return <OperationHistoryView domain="taxonomy" onStatus={onStatus} />;
  if (tab.kind === "settings") return <SettingsView section={tab.settingsSection ?? "General"} onSectionChange={(section) => updateSettingsTab(tab.id, section)} onBaseReplaced={onBaseReplaced} onWorkspaceChanged={onWorkspaceChanged} generalSettings={generalSettings} generalSettingsLoadError={generalSettingsLoadError} onGeneralSettingsChange={onGeneralSettingsChange} />;
  if (tab.kind === "photo-detail" && tab.photo) return <PhotoDetailView photo={tab.photo} />;
  if (tab.kind === "mapping-editor" && tab.photo) return <MappingEditor photo={tab.photo} onOpenTaxon={handlers.openTaxon} />;
  if (tab.kind === "search-photos" && tab.query) return <PhotoSet query={tab.query} handlers={handlers} />;
  if (tab.kind === "taxon-photos" && tab.taxonId !== undefined) return <PhotoSet taxonId={tab.taxonId} handlers={handlers} />;
  return null;
}

function ActivityButton({
  icon: Icon,
  label,
  active,
  onClick,
  disabled = false,
}: {
  icon: IconComponent;
  label: string;
  active: boolean;
  onClick: () => void;
  disabled?: boolean;
}) {
  return <IconButton className={`activity-button${active ? " active" : ""}`} title={label} aria-label={label} disabled={disabled} onClick={onClick}><Icon size={19} /></IconButton>;
}
