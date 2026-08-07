import "maplibre-gl/dist/maplibre-gl.css";

import { ChevronDown, ChevronRight, Folder, Images, Network, RefreshCw } from "lucide-react";
import maplibregl, { type Map as MapLibreMap } from "maplibre-gl";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  browsePhotoDirectory,
  getPhotoDirectoryCounts,
  getPhotoLibrary,
  refreshPhotoDirectory,
  type PhotoDirectory,
  type PhotoDirectoryItem,
  type PhotoLibrary,
  type Photo,
} from "../../api/photos";
import { errorMessage } from "../../api/common";
import { getMapSettings, listMapPhotos, type MapBounds, type MapPhoto } from "../../api/map";
import { browsePhotoTaxon, type PhotoTaxonItem, type PhotoTaxonUsage } from "../../api/mapping";
import type { TaxonTreeNameParts } from "../../api/general";
import { waitForOperation } from "../../api/tasks";
import { EmptyState, IconButton, SectionHeader, VirtualList } from "../../shared/ui";
import { PhotoStage } from "./PhotoMedia";
import { usePhotoInteraction, type PhotoOpenHandlers } from "./PhotoInteraction";
import { useDeferredPhotoMutation, usePhotoMutation } from "./photoMutations";
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
  const [libraryLoading, setLibraryLoading] = useState(library === null);
  const [libraryError, setLibraryError] = useState("");
  const directoryId = trail[trail.length - 1]?.directory_id ?? library?.root_directory_id ?? null;
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
    stateKey: "folders.interaction",
  });
  usePhotoMutation(() => {
    void Promise.all([page.reload(), tree.reloadExpanded()]);
  });

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
    void getPhotoDirectoryCounts(directoryId)
      .then((counts) => onStatus(`${counts.directory_count} folders, ${counts.file_count} photos`))
      .catch(() => undefined);
  }, [directoryId, onStatus]);

  async function refresh() {
    if (directoryId === null) return;
    onStatus("Refreshing photo library", true);
    const started = await refreshPhotoDirectory(directoryId);
    await waitForOperation("photos", started.operation.task_id, (operation) => onStatus(operation.message, true));
    await page.reload();
  }

  function enter(directory: PhotoDirectory) {
    tree.clear();
    setTrail((current) => [...current, directory]);
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
        <IconButton aria-label="Refresh" onClick={() => void refresh()} title="Refresh"><RefreshCw size={14} /></IconButton>
      </header>
      <ResizablePanels
        className="explorer-columns"
        initialRatio={0.34}
        minFirst={220}
        minSecond={320}
        separatorLabel="Resize folder browser and photo preview"
        stateKey="folders.columns"
        first={(<aside className="finder-pane">
          <VirtualList
            stateKey="folders.list"
            items={rows}
            rowHeight={28}
            itemKey={(item) => item.kind === "directory"
              ? `d:${item.directory.directory_id}`
              : item.kind === "photo"
                ? `p:${item.photo.photo_id}`
                : `m:${item.parentId}`}
            onNearEnd={() => void page.loadMore()}
            renderItem={(item) => (
              item.kind === "directory" ? (
                <div className="finder-row tree" style={{ paddingLeft: 4 + item.depth * 14 }}>
                  <IconButton
                    aria-label={tree.nodes.get(item.directory.directory_id)?.expanded ? "Collapse folder" : "Expand folder"}
                    className="tree-toggle"
                    onClick={() => tree.toggle(item.directory.directory_id)}
                    title={tree.nodes.get(item.directory.directory_id)?.expanded ? "Collapse folder" : "Expand folder"}
                  >
                    {tree.nodes.get(item.directory.directory_id)?.expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
                  </IconButton>
                  <Folder size={14} />
                  <button className="tree-label" type="button" onClick={() => enter(item.directory)}>
                    {item.directory.name}
                  </button>
                </div>
              ) : item.kind === "photo" ? (
                <button
                  className={`finder-row${interaction.selectedId === item.photo.photo_id ? " active" : ""}`}
                  style={{ paddingLeft: 22 + item.depth * 14 }}
                  type="button"
                  onClick={() => interaction.selectPhoto(item.photo)}
                  onContextMenu={(event) => interaction.openContextMenu(event, item.photo)}
                >
                  <Images size={14} /><span>{item.photo.filename}</span>
                </button>
              ) : (
                <button
                  className="finder-row tree-more"
                  style={{ paddingLeft: 22 + item.depth * 14 }}
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
        second={<PhotoStage photo={interaction.selected} onContextMenu={interaction.openContextMenu} />}
      />
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
  const currentId = trail[trail.length - 1]?.taxon_id ?? null;
  const page = useCursorPage<PhotoTaxonItem, number | null>({
    params: currentId,
    resetKey: currentId,
    stateKey: "photo-taxonomy.page",
    loadPage: (taxonId, cursor) => browsePhotoTaxon(taxonId, false, true, cursor),
  });
  const tree = useCursorTree<PhotoTaxonItem, number>({
    stateKey: "photo-taxonomy.tree",
    loadPage: (taxonId, cursor) => browsePhotoTaxon(taxonId, false, true, cursor),
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
    stateKey: "photo-taxonomy.interaction",
  });
  usePhotoMutation(() => {
    void Promise.all([page.reload(), tree.reloadExpanded()]);
  });

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
      </header>
      <ResizablePanels
        className="explorer-columns"
        initialRatio={0.34}
        minFirst={220}
        minSecond={320}
        separatorLabel="Resize taxon browser and photo preview"
        stateKey="photo-taxonomy.columns"
        first={(<aside className="finder-pane">
          <VirtualList
            stateKey="photo-taxonomy.list"
            items={rows}
            rowHeight={28}
            itemKey={(item) => item.kind === "taxon"
              ? `t:${item.taxon.taxon_id}`
              : item.kind === "photo"
                ? `p:${item.photo.photo_id}`
                : `m:${item.parentId}`}
            onNearEnd={() => void page.loadMore()}
            renderItem={(item) => (
              item.kind === "taxon" ? (
                <div className="finder-row tree taxon" style={{ paddingLeft: 4 + item.depth * 14 }}>
                  <IconButton
                    aria-label={tree.nodes.get(item.taxon.taxon_id)?.expanded ? "Collapse taxon" : "Expand taxon"}
                    className="tree-toggle"
                    onClick={() => tree.toggle(item.taxon.taxon_id)}
                    title={tree.nodes.get(item.taxon.taxon_id)?.expanded ? "Collapse taxon" : "Expand taxon"}
                  >
                    {tree.nodes.get(item.taxon.taxon_id)?.expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
                  </IconButton>
                  <Network size={14} />
                  <button className="tree-label" type="button" title={formatTaxonTreeName(item.taxon, nameParts)} onClick={() => {
                    tree.clear();
                    setTrail((current) => [...current, item.taxon]);
                  }}>
                    {formatTaxonTreeName(item.taxon, nameParts)}
                  </button>
                </div>
              ) : item.kind === "photo" ? (
                <button className={`finder-row${interaction.selectedId === item.photo.photo_id ? " active" : ""}`} style={{ paddingLeft: 22 + item.depth * 14 }} type="button" onClick={() => interaction.selectPhoto(item.photo)} onContextMenu={(event) => interaction.openContextMenu(event, item.photo)}>
                  <Images size={14} /><span>{item.photo.filename}</span>
                </button>
              ) : (
                <button
                  className="finder-row tree-more"
                  style={{ paddingLeft: 22 + item.depth * 14 }}
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
        second={<PhotoStage photo={interaction.selected} onContextMenu={interaction.openContextMenu} />}
      />
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
  });
  useDeferredPhotoMutation(active, () => {
    void page.reload();
  }, (mutation) => mutation.kind === "photo");

  useEffect(() => {
    let disposed = false;
    getMapSettings().then((settings) => {
      if (disposed || !container.current) return;
      const rasterUrl = settings.provider === "tianditu" && settings.tianditu_token
        ? `https://t0.tianditu.gov.cn/vec_w/wmts?tk=${settings.tianditu_token}&service=wmts&request=gettile&version=1.0.0&layer=vec&style=default&tilematrixset=w&format=tiles&tilematrix={z}&tilerow={y}&tilecol={x}`
        : "https://tile.openstreetmap.org/{z}/{x}/{y}.png";
      const next = new maplibregl.Map({
        container: container.current,
        center: [0, 20],
        zoom: 1.5,
        style: { version: 8, sources: { tiles: { type: "raster", tiles: [rasterUrl], tileSize: 256 } }, layers: [{ id: "tiles", type: "raster", source: "tiles" }] },
      });
      map.current = next;
      const updateBounds = () => setBounds(readMapBounds(next.getBounds()));
      setMapReady(true);
      updateBounds();
      next.on("load", updateBounds);
      next.on("moveend", updateBounds);
    });
    return () => {
      disposed = true;
      map.current?.remove();
      map.current = null;
      markers.current.clear();
    };
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
      marker.addEventListener("click", () => interaction.selectPhoto(item.photo));
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
    <div className="map-view">
      <div className="map-canvas" ref={container} />
      {interaction.selected && (
        <div className="map-photo-float">
          <PhotoStage photo={interaction.selected} compact onContextMenu={interaction.openContextMenu} />
        </div>
      )}
      <span className="map-count">{page.items.length} geotagged photos{page.loading ? " loading" : ""}</span>
      {interaction.contextMenu}
    </div>
  );
}
