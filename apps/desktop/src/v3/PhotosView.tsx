import "maplibre-gl/dist/maplibre-gl.css";

import { ChevronDown, ChevronRight, Folder, Images, RefreshCw } from "lucide-react";
import maplibregl, { type Map as MapLibreMap } from "maplibre-gl";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  browsePhotoDirectory,
  browsePhotoTaxon,
  errorMessage,
  getMapSettings,
  getPhotoDirectoryCounts,
  getPhotoLibrary,
  listMapPhotos,
  refreshPhotoDirectory,
  waitForOperation,
  type PhotoDirectory,
  type PhotoDirectoryItem,
  type PhotoLibrary,
  type MapBounds,
  type MapPhoto,
  type Photo,
  type PhotoTaxonItem,
  type PhotoTaxonUsage,
} from "./api";
import { EmptyState, PhotoStage, SectionHeader, VirtualList } from "./components";
import { usePhotoInteraction, type PhotoOpenHandlers } from "./PhotoInteraction";
import { useDeferredPhotoMutation, usePhotoMutation } from "./photoMutations";
import { useCursorPage } from "./useCursorPage";
import { useCursorTree, type CursorTreeNode } from "./useCursorTree";
import { useViewState } from "./viewState";

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
        <button className="icon-button" type="button" onClick={() => void refresh()} title="Refresh"><RefreshCw size={14} /></button>
      </header>
      <div className="explorer-columns">
        <aside className="finder-pane">
          <VirtualList
            stateKey="folders.list"
            items={rows}
            rowHeight={40}
            itemKey={(item) => item.kind === "directory"
              ? `d:${item.directory.directory_id}`
              : item.kind === "photo"
                ? `p:${item.photo.photo_id}`
                : `m:${item.parentId}`}
            onNearEnd={() => void page.loadMore()}
            renderItem={(item) => (
              item.kind === "directory" ? (
                <div className="finder-row tree" style={{ paddingLeft: 6 + item.depth * 18 }}>
                  <button
                    className="tree-toggle"
                    type="button"
                    onClick={() => tree.toggle(item.directory.directory_id)}
                    title={tree.nodes.get(item.directory.directory_id)?.expanded ? "Collapse folder" : "Expand folder"}
                  >
                    {tree.nodes.get(item.directory.directory_id)?.expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
                  </button>
                  <Folder size={14} />
                  <button className="tree-label" type="button" onDoubleClick={() => enter(item.directory)}>
                    {item.directory.name}
                  </button>
                </div>
              ) : item.kind === "photo" ? (
                <button
                  className={`finder-row${interaction.selectedId === item.photo.photo_id ? " active" : ""}`}
                  style={{ paddingLeft: 30 + item.depth * 18 }}
                  type="button"
                  onClick={() => interaction.selectPhoto(item.photo)}
                  onContextMenu={(event) => interaction.openContextMenu(event, item.photo)}
                >
                  <Images size={14} /><span>{item.photo.filename}</span>
                </button>
              ) : (
                <button
                  className="finder-row tree-more"
                  style={{ paddingLeft: 30 + item.depth * 18 }}
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
        </aside>
        <PhotoStage photo={interaction.selected} onContextMenu={interaction.openContextMenu} />
      </div>
      {interaction.contextMenu}
    </div>
  );
}

export function TaxonPhotosView({ handlers }: { handlers: PhotoOpenHandlers }) {
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
      <header className="workbench-toolbar breadcrumbs">
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
      </header>
      <div className="explorer-columns">
        <aside className="finder-pane">
          <VirtualList
            stateKey="photo-taxonomy.list"
            items={rows}
            rowHeight={48}
            itemKey={(item) => item.kind === "taxon"
              ? `t:${item.taxon.taxon_id}`
              : item.kind === "photo"
                ? `p:${item.photo.photo_id}`
                : `m:${item.parentId}`}
            onNearEnd={() => void page.loadMore()}
            renderItem={(item) => (
              item.kind === "taxon" ? (
                <div className="finder-row tree taxon" style={{ paddingLeft: 6 + item.depth * 18 }}>
                  <button
                    className="tree-toggle"
                    type="button"
                    onClick={() => tree.toggle(item.taxon.taxon_id)}
                    title={tree.nodes.get(item.taxon.taxon_id)?.expanded ? "Collapse taxon" : "Expand taxon"}
                  >
                    {tree.nodes.get(item.taxon.taxon_id)?.expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
                  </button>
                  <button className="tree-label stacked" type="button" onClick={() => {
                    tree.clear();
                    setTrail((current) => [...current, item.taxon]);
                  }}>
                    <strong>{item.taxon.names.sci_name ?? `Taxon ${item.taxon.taxon_id}`}</strong>
                    <small>{item.taxon.subtree_photo_count} photos</small>
                  </button>
                </div>
              ) : item.kind === "photo" ? (
                <button className={`finder-row${interaction.selectedId === item.photo.photo_id ? " active" : ""}`} style={{ paddingLeft: 30 + item.depth * 18 }} type="button" onClick={() => interaction.selectPhoto(item.photo)} onContextMenu={(event) => interaction.openContextMenu(event, item.photo)}>
                  <Images size={14} /><span>{item.photo.filename}</span>
                </button>
              ) : (
                <button
                  className="finder-row tree-more"
                  style={{ paddingLeft: 30 + item.depth * 18 }}
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
        </aside>
        <PhotoStage photo={interaction.selected} onContextMenu={interaction.openContextMenu} />
      </div>
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
