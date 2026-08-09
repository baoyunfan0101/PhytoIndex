import "maplibre-gl/dist/maplibre-gl.css";

import { ChevronDown, ChevronRight, Folder, Images, Network } from "lucide-react";
import maplibregl, { type Map as MapLibreMap } from "maplibre-gl";
import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import {
  browsePhotoDirectory,
  getPhotoDirectoryCounts,
  getPhotoLibrary,
  refreshPhotoDirectory,
  type PhotoDirectory,
  type PhotoDirectoryItem,
  type PhotoLibrary,
  type Photo,
  photoUrl,
} from "../../api/photos";
import { errorMessage } from "../../api/common";
import { getMapPhotoBounds, getMapSettings, listMapPhotos, type MapBounds, type MapPhoto } from "../../api/map";
import { browsePhotoTaxon, type PhotoTaxonItem, type PhotoTaxonUsage } from "../../api/mapping";
import type { TaxonTreeNameParts } from "../../api/general";
import { waitForOperation } from "../../api/tasks";
import { EmptyState, IconButton, SectionHeader, VirtualList } from "../../shared/ui";
import { DirectoryContextMenu } from "./DirectoryContextMenu";
import { PhotoStage } from "./PhotoMedia";
import { PhotoDisplay, PhotoDisplayToggle, usePhotoActivation, usePhotoDisplayMode } from "./PhotoDisplay";
import { usePhotoInteraction, type PhotoOpenHandlers } from "./PhotoInteraction";
import { TaxonContextMenu } from "./TaxonContextMenu";
import { emitPhotoMutation, useDeferredPhotoMutation, usePhotoMutation } from "./photoMutations";
import { findTypeSelectIndex, nextListIndex, treeArrowAction } from "./photoListNavigation";
import { useCursorPage } from "../../shared/useCursorPage";
import { useCursorTree, type CursorTreeNode } from "../../shared/useCursorTree";
import { useViewState } from "../../shared/viewState";
import { ResizablePanels } from "../../shared/ResizablePanels";

type DirectoryTreeRow =
  | { kind: "directory"; directory: PhotoDirectory; depth: number }
  | { kind: "photo"; photo: Photo; depth: number }
  | { kind: "more"; parentId: number; depth: number; loading: boolean };

type TaxonTreeRow =
  | { kind: "taxon"; taxon: PhotoTaxonUsage; depth: number }
  | { kind: "photo"; photo: Photo; depth: number }
  | { kind: "more"; parentId: number; depth: number; loading: boolean };

function directoryTreeRowKey(item: DirectoryTreeRow) {
  if (item.kind === "directory") return `d:${item.directory.directory_id}`;
  if (item.kind === "photo") return `p:${item.photo.photo_id}`;
  return `m:${item.parentId}`;
}

function taxonTreeRowKey(item: TaxonTreeRow) {
  if (item.kind === "taxon") return `t:${item.taxon.taxon_id}`;
  if (item.kind === "photo") return `p:${item.photo.photo_id}`;
  return `m:${item.parentId}`;
}

function flattenDirectoryItems(
  items: PhotoDirectoryItem[],
  nodes: Map<number, CursorTreeNode<PhotoDirectoryItem>>,
  depth = 0,
  visited = new Set<number>(),
): DirectoryTreeRow[] {
  return items.flatMap((item): DirectoryTreeRow[] => {
    if (item.kind === "photo") return [{ kind: "photo", photo: item.photo, depth }];
    const row: DirectoryTreeRow = { kind: "directory", directory: item.directory, depth };
    const node = nodes.get(item.directory.directory_id);
    if (!node?.expanded || visited.has(item.directory.directory_id)) return [row];
    const nextVisited = new Set(visited).add(item.directory.directory_id);
    const descendants = flattenDirectoryItems(node.items, nodes, depth + 1, nextVisited);
    const more: DirectoryTreeRow[] = node.loading || node.nextCursor
      ? [{ kind: "more", parentId: item.directory.directory_id, depth: depth + 1, loading: node.loading }]
      : [];
    return [row, ...descendants, ...more];
  });
}

function flattenTaxonItems(
  items: PhotoTaxonItem[],
  nodes: Map<number, CursorTreeNode<PhotoTaxonItem>>,
  depth = 0,
  visited = new Set<number>(),
): TaxonTreeRow[] {
  return items.flatMap((item): TaxonTreeRow[] => {
    if (item.kind === "photo") return [{ kind: "photo", photo: item.photo, depth }];
    const row: TaxonTreeRow = { kind: "taxon", taxon: item.taxon, depth };
    const node = nodes.get(item.taxon.taxon_id);
    if (!node?.expanded || visited.has(item.taxon.taxon_id)) return [row];
    const nextVisited = new Set(visited).add(item.taxon.taxon_id);
    const descendants = flattenTaxonItems(node.items, nodes, depth + 1, nextVisited);
    const more: TaxonTreeRow[] = node.loading || node.nextCursor
      ? [{ kind: "more", parentId: item.taxon.taxon_id, depth: depth + 1, loading: node.loading }]
      : [];
    return [row, ...descendants, ...more];
  });
}

function formatTaxonTreeName(taxon: PhotoTaxonUsage, parts: TaxonTreeNameParts) {
  const selected = [
    parts.sci_name ? taxon.names.sci_name : null,
    parts.zh_name ? taxon.names.zh_name : null,
    parts.en_name ? taxon.names.en_name : null,
  ].filter(Boolean);
  const names = selected.length > 0
    ? selected
    : [taxon.names.sci_name, taxon.names.zh_name, taxon.names.en_name].filter(Boolean);
  return names.length > 0 ? names.join(" \u00b7 ") : `Taxon ${taxon.taxon_id}`;
}

function normalizeLongitude(value: number) {
  return ((value + 180) % 360 + 360) % 360 - 180;
}

function readMapBounds(value: maplibregl.LngLatBounds): MapBounds {
  const rawWest = value.getWest();
  const rawEast = value.getEast();
  const span = rawEast - rawWest;
  return {
    west: span >= 360 ? -180 : normalizeLongitude(rawWest),
    south: Math.max(-90, value.getSouth()),
    east: span >= 360 ? 180 : normalizeLongitude(rawEast),
    north: Math.min(90, value.getNorth()),
  };
}

export function FolderPhotosView({
  handlers,
  onStatus,
}: {
  handlers: PhotoOpenHandlers;
  onStatus: (message: string, busy?: boolean) => void;
}) {
  const [library, setLibrary] = useViewState<PhotoLibrary | null>("folders.library", null);
  const [trail, setTrail] = useViewState<PhotoDirectory[]>("folders.trail", []);
  const [activeRowKey, setActiveRowKey] = useViewState<string | null>("folders.active-row", null);
  const [libraryLoading, setLibraryLoading] = useState(library === null);
  const [libraryError, setLibraryError] = useState("");
  const [directoryContext, setDirectoryContext] = useState<{
    directory: PhotoDirectory;
    x: number;
    y: number;
  } | null>(null);
  const directoryId = trail[trail.length - 1]?.directory_id ?? library?.root_directory_id ?? null;
  const currentDirectory = trail[trail.length - 1] ?? (library
    ? {
        directory_id: library.root_directory_id,
        parent_directory_id: null,
        name: "Root",
        relative_path: "",
      }
    : null);
  const page = useCursorPage<PhotoDirectoryItem, number | null>({
    params: directoryId,
    resetKey: directoryId,
    stateKey: "folders.page",
    enabled: directoryId !== null,
    loadPage: (id, cursor) => browsePhotoDirectory(id!, cursor),
  });
  const tree = useCursorTree<PhotoDirectoryItem, number>({
    stateKey: "folders.tree",
    loadPage: (id, cursor) => browsePhotoDirectory(id, cursor),
  });
  const rows = useMemo(
    () => flattenDirectoryItems(page.items, tree.nodes),
    [page.items, tree.nodes],
  );
  const photos = useMemo(
    () => rows.flatMap((row) => row.kind === "photo" ? [row.photo] : []),
    [rows],
  );
  const interaction = usePhotoInteraction({
    photos,
    handlers,
    selectFirst: false,
    stateKey: "folders.interaction",
  });
  const [displayMode, setDisplayMode] = usePhotoDisplayMode();
  const activation = usePhotoActivation({
    onSelect: selectDirectoryPhoto,
    onOpenImage: () => setDisplayMode("image"),
    onOpenDetails: handlers.openDetails,
  });
  const resolvedActiveRowKey = activeRowKey ?? (interaction.selectedId === null ? null : `p:${interaction.selectedId}`);
  const activeRowIndex = rows.findIndex((row) => directoryTreeRowKey(row) === resolvedActiveRowKey);
  usePhotoMutation(() => {
    void Promise.all([page.reload(), tree.reloadExpanded()]);
  });

  const reportDirectoryCounts = useCallback(async (id: number) => {
    try {
      const counts = await getPhotoDirectoryCounts(id);
      onStatus(`${counts.directory_count} folders, ${counts.file_count} photos`);
    } catch {}
  }, [onStatus]);

  useEffect(() => {
    getPhotoLibrary().then((next) => {
      setLibrary(next);
      setLibraryLoading(false);
    }).catch((nextError) => {
      setLibraryError(errorMessage(nextError));
      setLibraryLoading(false);
    });
  }, []);

  useEffect(() => {
    if (directoryId === null) return;
    void reportDirectoryCounts(directoryId);
  }, [directoryId, reportDirectoryCounts]);

  async function refreshDirectory(refreshDirectoryId: number) {
    onStatus("Refreshing photo library", true);
    const started = await refreshPhotoDirectory(refreshDirectoryId);
    await waitForOperation("photos", started.operation.task_id, (operation) => onStatus(operation.message, true));
    await Promise.all([page.reload(), tree.reloadExpanded()]);
    if (directoryId !== null) await reportDirectoryCounts(directoryId);
  }

  function enter(directory: PhotoDirectory) {
    tree.clear();
    setActiveRowKey(null);
    setTrail((current) => [...current, directory]);
  }

  function selectDirectoryRow(item: DirectoryTreeRow) {
    setActiveRowKey(directoryTreeRowKey(item));
    if (item.kind === "photo") interaction.selectPhoto(item.photo);
    else interaction.clearSelection();
  }

  function selectDirectoryPhoto(photo: Photo) {
    setActiveRowKey(`p:${photo.photo_id}`);
    interaction.selectPhoto(photo);
  }

  function openDirectoryContextMenu(event: MouseEvent, item: Extract<DirectoryTreeRow, { kind: "directory" }>) {
    event.preventDefault();
    event.stopPropagation();
    selectDirectoryRow(item);
    setDirectoryContext({ directory: item.directory, x: event.clientX, y: event.clientY });
  }

  function openDirectoryPhotoContextMenu(event: MouseEvent, item: Extract<DirectoryTreeRow, { kind: "photo" }>) {
    selectDirectoryRow(item);
    interaction.openContextMenu(event, item.photo);
  }

  function openCurrentDirectoryContextMenu(event: MouseEvent<HTMLElement>) {
    if (!currentDirectory) return;
    const target = event.target as HTMLElement | null;
    if (target?.closest(".finder-row")) return;
    event.preventDefault();
    setDirectoryContext({ directory: currentDirectory, x: event.clientX, y: event.clientY });
  }

  function activateDirectoryRow() {
    const item = rows[activeRowIndex];
    if (!item) return;
    if (item.kind === "directory") enter(item.directory);
    else if (item.kind === "photo") {
      interaction.selectPhoto(item.photo);
      setDisplayMode("image");
    }
    else void tree.loadMore(item.parentId);
  }

  function moveDirectoryRow(direction: -1 | 1) {
    const nextIndex = nextListIndex(rows.length, activeRowIndex, direction);
    if (nextIndex >= 0) selectDirectoryRow(rows[nextIndex]);
  }

  function moveDirectoryBranch(direction: -1 | 1) {
    const item = rows[activeRowIndex];
    if (item?.kind !== "directory") return false;
    const node = tree.nodes.get(item.directory.directory_id);
    if (!treeArrowAction(node?.expanded ?? false, direction)) return false;
    tree.toggle(item.directory.directory_id);
    return true;
  }

  function typeSelectDirectoryRow(query: string, shouldCycle: boolean) {
    const matchIndex = findTypeSelectIndex(
      rows,
      query,
      (item) => item.kind === "directory"
        ? [item.directory.name, item.directory.relative_path]
        : item.kind === "photo"
          ? [item.photo.filename, item.photo.relative_path]
          : ["Load more"],
      shouldCycle && activeRowIndex >= 0 ? activeRowIndex + 1 : 0,
    );
    if (matchIndex >= 0) selectDirectoryRow(rows[matchIndex]);
  }

  return (
    <div className="folder-workbench">
      <header className="workbench-toolbar">
        <div className="breadcrumbs">
          <button type="button" onClick={() => {
            tree.clear();
            setTrail([]);
          }}>Root</button>
          {trail.map((item, index) => (
            <span key={item.directory_id}><ChevronRight size={12} /><button type="button" onClick={() => {
              tree.clear();
              setTrail(trail.slice(0, index + 1));
            }}>{item.name}</button></span>
          ))}
        </div>
        <PhotoDisplayToggle mode={displayMode} onChange={setDisplayMode} />
      </header>
      <ResizablePanels
        className="explorer-columns"
        initialRatio={0.34}
        minFirst={220}
        minSecond={320}
        separatorLabel="Resize folder browser and photo preview"
        stateKey="folders.columns"
        first={(<aside className="finder-pane" onContextMenu={openCurrentDirectoryContextMenu}>
          <VirtualList
            stateKey="folders.list"
            items={rows}
            activeIndex={activeRowIndex}
            focusWhen={displayMode === "thumbnails"}
            rowHeight={28}
            itemKey={directoryTreeRowKey}
            onActivateActive={activateDirectoryRow}
            onMoveHorizontal={moveDirectoryBranch}
            onMoveActive={moveDirectoryRow}
            onNearEnd={() => void page.loadMore()}
            onTypeSelect={typeSelectDirectoryRow}
            renderItem={(item) => (
              item.kind === "directory" ? (
                <div
                  className={`finder-row tree${directoryTreeRowKey(item) === resolvedActiveRowKey ? " active" : ""}`}
                  style={{ paddingLeft: 4 + item.depth * 14 }}
                  onContextMenu={(event) => openDirectoryContextMenu(event, item)}
                >
                  <IconButton
                    aria-label={tree.nodes.get(item.directory.directory_id)?.expanded ? "Collapse folder" : "Expand folder"}
                    className="tree-toggle"
                    onClick={() => tree.toggle(item.directory.directory_id)}
                    title={tree.nodes.get(item.directory.directory_id)?.expanded ? "Collapse folder" : "Expand folder"}
                  >
                    {tree.nodes.get(item.directory.directory_id)?.expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
                  </IconButton>
                  <button className="tree-node-button" type="button" onClick={() => enter(item.directory)}>
                    <Folder size={14} />
                    <span className="tree-label">{item.directory.name}</span>
                  </button>
                </div>
              ) : item.kind === "photo" ? (
                <button
                  className={`finder-row${directoryTreeRowKey(item) === resolvedActiveRowKey ? " active" : ""}`}
                  style={{ paddingLeft: 4 + item.depth * 14 }}
                  type="button"
                  onClick={() => activation.clickPhoto(item.photo)}
                  onDoubleClick={() => activation.doubleClickPhoto(item.photo)}
                  onContextMenu={(event) => openDirectoryPhotoContextMenu(event, item)}
                >
                  <Images size={14} /><span>{item.photo.filename}</span>
                </button>
              ) : (
                <button
                  className={`finder-row tree-more${directoryTreeRowKey(item) === resolvedActiveRowKey ? " active" : ""}`}
                  style={{ paddingLeft: 4 + item.depth * 14 }}
                  type="button"
                  disabled={item.loading}
                  onClick={() => void tree.loadMore(item.parentId)}
                >
                  {item.loading ? "Loading" : "Load more"}
                </button>
              )
            )}
          />
          {(libraryLoading || page.loading) && <div className="pane-overlay">Loading</div>}
          {(libraryError || page.error) && <div className="inline-error">{libraryError || page.error}</div>}
        </aside>)}
        second={(
          <PhotoDisplay
            photos={photos}
            selected={interaction.selected}
            mode={displayMode}
            stateKey="folders.photo-grid"
            onModeChange={setDisplayMode}
            onSelect={selectDirectoryPhoto}
            onClickPhoto={activation.clickPhoto}
            onDoubleClickPhoto={activation.doubleClickPhoto}
            onNearEnd={() => void page.loadMore()}
            onContextMenu={interaction.openContextMenu}
          />
        )}
      />
      {directoryContext && (
        <DirectoryContextMenu
          {...directoryContext}
          onClose={() => setDirectoryContext(null)}
          onRefresh={(directory) => refreshDirectory(directory.directory_id)}
          onDirectoryRenamed={async (directory) => {
            setTrail((current) => current.map((item) => (
              item.directory_id === directory.directory_id ? directory : item
            )));
            await Promise.all([page.reload(), tree.reloadExpanded()]);
            emitPhotoMutation({ photoId: null, kind: "photo" });
          }}
          onRenamed={(result) => {
            const photoIds = result.rows.map((row) => row.photo_id);
            if (photoIds.length > 0) emitPhotoMutation({ photoId: null, photoIds, kind: "photo" });
          }}
          onStatus={onStatus}
        />
      )}
      {interaction.contextMenu}
    </div>
  );
}

export function TaxonPhotosView({
  handlers,
  nameParts,
}: {
  handlers: PhotoOpenHandlers;
  nameParts: TaxonTreeNameParts;
}) {
  const [trail, setTrail] = useViewState<PhotoTaxonUsage[]>("photo-taxonomy.trail", []);
  const [activeRowKey, setActiveRowKey] = useViewState<string | null>("photo-taxonomy.active-row", null);
  const [taxonContext, setTaxonContext] = useState<{
    taxon: PhotoTaxonUsage;
    x: number;
    y: number;
    showExpandAll: boolean;
  } | null>(null);
  const currentId = trail[trail.length - 1]?.taxon_id ?? null;
  const currentTaxon = trail[trail.length - 1] ?? null;
  const page = useCursorPage<PhotoTaxonItem, number | null>({
    params: currentId,
    resetKey: currentId,
    stateKey: "photo-taxonomy.page",
    loadPage: (taxonId, cursor) => browsePhotoTaxon(taxonId, false, cursor),
  });
  const tree = useCursorTree<PhotoTaxonItem, number>({
    stateKey: "photo-taxonomy.tree",
    loadPage: (taxonId, cursor) => browsePhotoTaxon(taxonId, false, cursor),
  });
  const rows = useMemo(
    () => flattenTaxonItems(page.items, tree.nodes),
    [page.items, tree.nodes],
  );
  const photos = useMemo(
    () => rows.flatMap((row) => row.kind === "photo" ? [row.photo] : []),
    [rows],
  );
  const interaction = usePhotoInteraction({
    photos,
    handlers,
    selectFirst: false,
    stateKey: "photo-taxonomy.interaction",
  });
  const [displayMode, setDisplayMode] = usePhotoDisplayMode();
  const activation = usePhotoActivation({
    onSelect: selectTaxonPhoto,
    onOpenImage: () => setDisplayMode("image"),
    onOpenDetails: handlers.openDetails,
  });
  const resolvedActiveRowKey = activeRowKey ?? (interaction.selectedId === null ? null : `p:${interaction.selectedId}`);
  const activeRowIndex = rows.findIndex((row) => taxonTreeRowKey(row) === resolvedActiveRowKey);
  usePhotoMutation(() => {
    void Promise.all([page.reload(), tree.reloadExpanded()]);
  });

  function enterTaxon(taxon: PhotoTaxonUsage) {
    tree.clear();
    setActiveRowKey(null);
    setTrail((current) => [...current, taxon]);
  }

  function selectTaxonRow(item: TaxonTreeRow) {
    setActiveRowKey(taxonTreeRowKey(item));
    if (item.kind === "photo") interaction.selectPhoto(item.photo);
    else interaction.clearSelection();
  }

  function selectTaxonPhoto(photo: Photo) {
    setActiveRowKey(`p:${photo.photo_id}`);
    interaction.selectPhoto(photo);
  }

  async function expandTaxonSubtree(taxon: PhotoTaxonUsage) {
    await tree.expandSubtree(
      taxon.taxon_id,
      (item) => item.kind === "taxon" ? item.taxon.taxon_id : null,
    );
  }

  function openTaxonContextMenu(event: MouseEvent, item: Extract<TaxonTreeRow, { kind: "taxon" }>) {
    event.preventDefault();
    event.stopPropagation();
    selectTaxonRow(item);
    setTaxonContext({ taxon: item.taxon, x: event.clientX, y: event.clientY, showExpandAll: true });
  }

  function openCurrentTaxonContextMenu(event: MouseEvent<HTMLElement>) {
    const item = rows[activeRowIndex];
    const taxon = currentTaxon ?? (item?.kind === "taxon" ? item.taxon : null);
    if (!taxon) return;
    const target = event.target as HTMLElement | null;
    if (target?.closest(".finder-row")) return;
    event.preventDefault();
    event.stopPropagation();
    setTaxonContext({ taxon, x: event.clientX, y: event.clientY, showExpandAll: false });
  }

  function openTaxonPhotoContextMenu(event: MouseEvent, item: Extract<TaxonTreeRow, { kind: "photo" }>) {
    selectTaxonRow(item);
    interaction.openContextMenu(event, item.photo);
  }

  function activateTaxonRow() {
    const item = rows[activeRowIndex];
    if (!item) return;
    if (item.kind === "taxon") enterTaxon(item.taxon);
    else if (item.kind === "photo") {
      interaction.selectPhoto(item.photo);
      setDisplayMode("image");
    }
    else void tree.loadMore(item.parentId);
  }

  function moveTaxonRow(direction: -1 | 1) {
    const nextIndex = nextListIndex(rows.length, activeRowIndex, direction);
    if (nextIndex >= 0) selectTaxonRow(rows[nextIndex]);
  }

  function moveTaxonBranch(direction: -1 | 1) {
    const item = rows[activeRowIndex];
    if (item?.kind !== "taxon") return false;
    const node = tree.nodes.get(item.taxon.taxon_id);
    if (!treeArrowAction(node?.expanded ?? false, direction)) return false;
    tree.toggle(item.taxon.taxon_id);
    return true;
  }

  function typeSelectTaxonRow(query: string, shouldCycle: boolean) {
    const matchIndex = findTypeSelectIndex(
      rows,
      query,
      (item) => item.kind === "taxon"
        ? [item.taxon.names.sci_name, item.taxon.names.zh_name, item.taxon.names.en_name]
        : item.kind === "photo"
          ? [item.photo.filename, item.photo.relative_path]
          : ["Load more"],
      shouldCycle && activeRowIndex >= 0 ? activeRowIndex + 1 : 0,
    );
    if (matchIndex >= 0) selectTaxonRow(rows[matchIndex]);
  }

  return (
    <div className="folder-workbench">
      <header className="workbench-toolbar">
        <div className="breadcrumbs">
          <button type="button" onClick={() => {
            tree.clear();
            setTrail([]);
          }}>Taxonomy</button>
          {trail.map((item, index) => (
            <span key={item.taxon_id}><ChevronRight size={12} /><button type="button" onClick={() => {
              tree.clear();
              setTrail(trail.slice(0, index + 1));
            }}>{item.names.sci_name ?? `Taxon ${item.taxon_id}`}</button></span>
          ))}
        </div>
        <PhotoDisplayToggle mode={displayMode} onChange={setDisplayMode} />
      </header>
      <ResizablePanels
        className="explorer-columns"
        initialRatio={0.34}
        minFirst={220}
        minSecond={320}
        separatorLabel="Resize taxon browser and photo preview"
        stateKey="photo-taxonomy.columns"
        first={(<aside className="finder-pane" onContextMenu={openCurrentTaxonContextMenu}>
          <VirtualList
            stateKey="photo-taxonomy.list"
            items={rows}
            activeIndex={activeRowIndex}
            focusWhen={displayMode === "thumbnails"}
            rowHeight={28}
            itemKey={taxonTreeRowKey}
            onActivateActive={activateTaxonRow}
            onMoveHorizontal={moveTaxonBranch}
            onMoveActive={moveTaxonRow}
            onNearEnd={() => void page.loadMore()}
            onContextMenu={openCurrentTaxonContextMenu}
            onTypeSelect={typeSelectTaxonRow}
            renderItem={(item) => (
              item.kind === "taxon" ? (
                <div className={`finder-row tree taxon${taxonTreeRowKey(item) === resolvedActiveRowKey ? " active" : ""}`} style={{ paddingLeft: 4 + item.depth * 14 }} onContextMenu={(event) => openTaxonContextMenu(event, item)}>
                  <IconButton
                    aria-label={tree.nodes.get(item.taxon.taxon_id)?.expanded ? "Collapse taxon" : "Expand taxon"}
                    className="tree-toggle"
                    onClick={() => tree.toggle(item.taxon.taxon_id)}
                    title={tree.nodes.get(item.taxon.taxon_id)?.expanded ? "Collapse taxon" : "Expand taxon"}
                  >
                    {tree.nodes.get(item.taxon.taxon_id)?.expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
                  </IconButton>
                  <button className="tree-node-button" type="button" title={formatTaxonTreeName(item.taxon, nameParts)} onClick={() => enterTaxon(item.taxon)}>
                    <Network size={14} />
                    <span className="tree-label">{formatTaxonTreeName(item.taxon, nameParts)}</span>
                  </button>
                </div>
              ) : item.kind === "photo" ? (
                <button className={`finder-row${taxonTreeRowKey(item) === resolvedActiveRowKey ? " active" : ""}`} style={{ paddingLeft: 4 + item.depth * 14 }} type="button" onClick={() => activation.clickPhoto(item.photo)} onDoubleClick={() => activation.doubleClickPhoto(item.photo)} onContextMenu={(event) => openTaxonPhotoContextMenu(event, item)}>
                  <Images size={14} /><span>{item.photo.filename}</span>
                </button>
              ) : (
                <button
                  className={`finder-row tree-more${taxonTreeRowKey(item) === resolvedActiveRowKey ? " active" : ""}`}
                  style={{ paddingLeft: 4 + item.depth * 14 }}
                  type="button"
                  disabled={item.loading}
                  onClick={() => void tree.loadMore(item.parentId)}
                >
                  {item.loading ? "Loading" : "Load more"}
                </button>
              )
            )}
          />
          {page.loading && <div className="pane-overlay">Loading</div>}
          {page.error && <div className="inline-error">{page.error}</div>}
        </aside>)}
        second={(
          <PhotoDisplay
            photos={photos}
            selected={interaction.selected}
            mode={displayMode}
            stateKey="photo-taxonomy.photo-grid"
            onModeChange={setDisplayMode}
            onSelect={selectTaxonPhoto}
            onClickPhoto={activation.clickPhoto}
            onDoubleClickPhoto={activation.doubleClickPhoto}
            onNearEnd={() => void page.loadMore()}
            onContextMenu={interaction.openContextMenu}
          />
        )}
      />
      {taxonContext && (
        <TaxonContextMenu
          taxon={taxonContext.taxon}
          x={taxonContext.x}
          y={taxonContext.y}
          onClose={() => setTaxonContext(null)}
          onExpandAll={taxonContext.showExpandAll ? expandTaxonSubtree : undefined}
          onOpenTaxonomy={(taxon) => handlers.openTaxon(taxon.taxon_id)}
        />
      )}
      {interaction.contextMenu}
    </div>
  );
}

export function PhotoMapView({
  active,
  handlers,
}: {
  active: boolean;
  handlers: PhotoOpenHandlers;
}) {
  const container = useRef<HTMLDivElement>(null);
  const map = useRef<MapLibreMap | null>(null);
  const markers = useRef(new Map<number, maplibregl.Marker>());
  const [mapReady, setMapReady] = useState(false);
  const [bounds, setBounds] = useState<MapBounds | null>(null);
  const [savedViewport, setSavedViewport] = useViewState<{
    longitude: number;
    latitude: number;
    zoom: number;
  } | null>("map.viewport", null);
  const boundsKey = bounds
    ? `${bounds.west}:${bounds.south}:${bounds.east}:${bounds.north}`
    : "no-bounds";
  const page = useCursorPage<MapPhoto, MapBounds | null>({
    params: bounds,
    resetKey: boundsKey,
    enabled: bounds !== null,
    loadPage: (viewport, cursor) => listMapPhotos(viewport, cursor, 200),
  });
  const photos = useMemo(() => page.items.map((item) => item.photo), [page.items]);
  const interaction = usePhotoInteraction({
    photos,
    handlers,
    selectFirst: false,
    stateKey: "map.interaction",
  });
  useDeferredPhotoMutation(active, () => {
    void page.reload();
  }, (mutation) => mutation.kind === "photo");

  useEffect(() => {
    let disposed = false;
    if (!active || map.current) return;
    Promise.all([
      getMapSettings().catch(() => ({ provider: "osm" as const, tianditu_token: null })),
      savedViewport ? Promise.resolve(null) : getMapPhotoBounds().catch(() => null),
    ]).then(([settings, photoBounds]) => {
      if (disposed || !container.current) return;
      const rasterUrl = settings.provider === "tianditu" && settings.tianditu_token
        ? `https://t0.tianditu.gov.cn/vec_w/wmts?tk=${settings.tianditu_token}&service=wmts&request=gettile&version=1.0.0&layer=vec&style=default&tilematrixset=w&format=tiles&tilematrix={z}&tilerow={y}&tilecol={x}`
        : "https://tile.openstreetmap.org/{z}/{x}/{y}.png";
      const next = new maplibregl.Map({
        container: container.current,
        ...(savedViewport
          ? { center: [savedViewport.longitude, savedViewport.latitude] as [number, number], zoom: savedViewport.zoom }
          : photoBounds
            ? {
                bounds: [[photoBounds.west, photoBounds.south], [photoBounds.east, photoBounds.north]] as [[number, number], [number, number]],
                fitBoundsOptions: { padding: 64, maxZoom: 13 },
              }
            : { center: [0, 20] as [number, number], zoom: 1.5 }),
        style: { version: 8, sources: { tiles: { type: "raster", tiles: [rasterUrl], tileSize: 256 } }, layers: [{ id: "tiles", type: "raster", source: "tiles" }] },
      });
      map.current = next;
      const updateViewport = () => {
        const center = next.getCenter();
        setSavedViewport({ longitude: center.lng, latitude: center.lat, zoom: next.getZoom() });
        setBounds(readMapBounds(next.getBounds()));
      };
      setMapReady(true);
      updateViewport();
      next.on("load", updateViewport);
      next.on("moveend", updateViewport);
    });
    return () => {
      disposed = true;
    };
  }, [active]);

  useEffect(() => () => {
    map.current?.remove();
    map.current = null;
    markers.current.clear();
  }, []);

  useEffect(() => {
    if (!active || !map.current) return;
    const frame = window.requestAnimationFrame(() => map.current?.resize());
    return () => window.cancelAnimationFrame(frame);
  }, [active]);

  useEffect(() => {
    if (!mapReady || !map.current) return;
    const visibleIds = new Set(page.items.map((item) => item.photo.photo_id));
    markers.current.forEach((marker, photoId) => {
      if (visibleIds.has(photoId)) return;
      marker.remove();
      markers.current.delete(photoId);
    });
    page.items.forEach((item) => {
      if (markers.current.has(item.photo.photo_id)) return;
      const marker = document.createElement("button");
      marker.className = "map-photo-marker";
      marker.type = "button";
      marker.title = item.photo.filename;
      marker.addEventListener("click", (event) => {
        event.stopPropagation();
        interaction.selectPhoto(item.photo);
      });
      markers.current.set(
        item.photo.photo_id,
        new maplibregl.Marker({ element: marker })
          .setLngLat([item.longitude, item.latitude])
          .addTo(map.current!),
      );
    });
  }, [interaction.selectPhoto, mapReady, page.items]);

  useEffect(() => {
    if (page.hasMore && !page.loading) void page.loadMore();
  }, [page.hasMore, page.loadMore, page.loading]);

  return (
    <div
      className="map-view"
      onClick={(event) => {
        const target = event.target as Element;
        if (!target.closest(".map-photo-preview") && !target.closest(".map-photo-marker")) {
          interaction.clearSelection();
        }
      }}
    >
      <div className="map-canvas" ref={container} />
      {interaction.selected && (
        <button
          className="map-photo-preview"
          type="button"
          aria-label={`Open details for ${interaction.selected.filename}`}
          onClick={() => handlers.openDetails(interaction.selected!)}
          onContextMenu={(event) => interaction.openContextMenu(event, interaction.selected!)}
        >
          <img src={photoUrl(interaction.selected, true)} alt="" draggable={false} />
          <span>{interaction.selected.filename}</span>
        </button>
      )}
      <span className="map-count">{page.items.length} photos in view{page.loading ? " loading" : ""}</span>
      {interaction.contextMenu}
    </div>
  );
}
