import "maplibre-gl/dist/maplibre-gl.css";

import { ChevronRight, Download, Folder, History, Images, RefreshCw, RotateCcw } from "lucide-react";
import maplibregl, { type Map as MapLibreMap } from "maplibre-gl";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  browsePhotoDirectory,
  browsePhotoTaxon,
  downloadCsv,
  errorMessage,
  exportAllPhotoOperationsCsv,
  exportPhotoOperationCsv,
  getMapSettings,
  getPhotoDirectoryCounts,
  getPhotoLibrary,
  listMapPhotos,
  listPhotoOperations,
  refreshPhotoDirectory,
  revertPhotoOperation,
  waitForOperation,
  type PhotoDirectory,
  type PhotoDirectoryItem,
  type PhotoLibrary,
  type PhotoOperation,
  type PhotoTaxonItem,
  type PhotoTaxonUsage,
} from "./api";
import { EmptyState, PhotoStage, SectionHeader, VirtualList } from "./components";
import { usePhotoInteraction, type PhotoOpenHandlers } from "./PhotoInteraction";
import { useCursorPage } from "./useCursorPage";

export function FolderPhotosView({
  handlers,
  onStatus,
}: {
  handlers: PhotoOpenHandlers;
  onStatus: (message: string, busy?: boolean) => void;
}) {
  const [library, setLibrary] = useState<PhotoLibrary | null>(null);
  const [trail, setTrail] = useState<PhotoDirectory[]>([]);
  const [libraryLoading, setLibraryLoading] = useState(true);
  const [libraryError, setLibraryError] = useState("");
  const directoryId = trail[trail.length - 1]?.directory_id ?? library?.root_directory_id ?? null;
  const page = useCursorPage<PhotoDirectoryItem, number | null>({
    params: directoryId,
    resetKey: directoryId,
    enabled: directoryId !== null,
    loadPage: (id, cursor) => browsePhotoDirectory(id!, cursor),
  });
  const photos = useMemo(
    () => page.items.flatMap((item) => item.kind === "photo" ? [item.photo] : []),
    [page.items],
  );
  const interaction = usePhotoInteraction({
    photos,
    handlers,
    onPhotoChanged: (photo) => page.updateItems((current) => current.map((item) => (
      item.kind === "photo" && item.photo.photo_id === photo.photo_id
        ? { ...item, photo }
        : item
    ))),
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
    setTrail((current) => [...current, directory]);
  }

  return (
    <div className="folder-workbench">
      <header className="workbench-toolbar">
        <div className="breadcrumbs">
          <button type="button" onClick={() => {
            setTrail([]);
          }}>Root</button>
          {trail.map((item, index) => (
            <span key={item.directory_id}><ChevronRight size={12} /><button type="button" onClick={() => {
              setTrail(trail.slice(0, index + 1));
            }}>{item.name}</button></span>
          ))}
        </div>
        <button className="icon-button" type="button" onClick={() => void refresh()} title="Refresh"><RefreshCw size={14} /></button>
      </header>
      <div className="explorer-columns">
        <aside className="finder-pane">
          <VirtualList
            items={page.items}
            rowHeight={40}
            itemKey={(item) => item.kind === "directory" ? `d:${item.directory.directory_id}` : `p:${item.photo.photo_id}`}
            onNearEnd={() => void page.loadMore()}
            renderItem={(item) => (
              item.kind === "directory" ? (
                <button className="finder-row" type="button" onDoubleClick={() => enter(item.directory)}>
                  <Folder size={14} /><span>{item.directory.name}</span><ChevronRight size={12} />
                </button>
              ) : (
                <button
                  className={`finder-row${interaction.selectedId === item.photo.photo_id ? " active" : ""}`}
                  type="button"
                  onClick={() => interaction.selectPhoto(item.photo)}
                  onContextMenu={(event) => interaction.openContextMenu(event, item.photo)}
                >
                  <Images size={14} /><span>{item.photo.filename}</span>
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
  const [trail, setTrail] = useState<PhotoTaxonUsage[]>([]);
  const currentId = trail[trail.length - 1]?.taxon_id ?? null;
  const page = useCursorPage<PhotoTaxonItem, number | null>({
    params: currentId,
    resetKey: currentId,
    loadPage: (taxonId, cursor) => browsePhotoTaxon(taxonId, false, true, cursor),
  });
  const photos = useMemo(
    () => page.items.flatMap((item) => item.kind === "photo" ? [item.photo] : []),
    [page.items],
  );
  const interaction = usePhotoInteraction({
    photos,
    handlers,
    onPhotoChanged: (photo) => page.updateItems((current) => current.map((item) => (
      item.kind === "photo" && item.photo.photo_id === photo.photo_id
        ? { ...item, photo }
        : item
    ))),
  });

  return (
    <div className="folder-workbench">
      <header className="workbench-toolbar breadcrumbs">
        <button type="button" onClick={() => setTrail([])}>Taxonomy</button>
        {trail.map((item, index) => (
          <span key={item.taxon_id}><ChevronRight size={12} /><button type="button" onClick={() => setTrail(trail.slice(0, index + 1))}>{item.names.sci_name ?? `Taxon ${item.taxon_id}`}</button></span>
        ))}
      </header>
      <div className="explorer-columns">
        <aside className="finder-pane">
          <VirtualList
            items={page.items}
            rowHeight={48}
            itemKey={(item) => item.kind === "taxon" ? `t:${item.taxon.taxon_id}` : `p:${item.photo.photo_id}`}
            onNearEnd={() => void page.loadMore()}
            renderItem={(item) => (
              item.kind === "taxon" ? (
                <button className="finder-row taxon" type="button" onClick={() => setTrail((current) => [...current, item.taxon])}>
                  <span><strong>{item.taxon.names.sci_name ?? `Taxon ${item.taxon.taxon_id}`}</strong><small>{item.taxon.subtree_photo_count} photos</small></span><ChevronRight size={12} />
                </button>
              ) : (
                <button className={`finder-row${interaction.selectedId === item.photo.photo_id ? " active" : ""}`} type="button" onClick={() => interaction.selectPhoto(item.photo)} onContextMenu={(event) => interaction.openContextMenu(event, item.photo)}>
                  <Images size={14} /><span>{item.photo.filename}</span>
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

export function PhotoHistoryView({ onStatus }: { onStatus: (message: string) => void }) {
  const page = useCursorPage<PhotoOperation, null>({
    params: null,
    resetKey: "photo-history",
    loadPage: (_, cursor) => listPhotoOperations(cursor),
  });

  async function exportAll() {
    downloadCsv("photo-rename-operations.csv", await exportAllPhotoOperationsCsv());
  }

  return (
    <div className="history-view">
      <SectionHeader title="Rename history" detail={`${page.items.length} operations${page.hasMore ? " loaded" : ""}`} actions={
        <button className="secondary-button" type="button" onClick={() => void exportAll()}><Download size={13} />Export all</button>
      } />
      {page.error ? <EmptyState title="Unable to load history" detail={page.error} /> : (
        <VirtualList
          className="history-list"
          items={page.items}
          rowHeight={72}
          itemKey={(item) => item.operation_id}
          onNearEnd={() => void page.loadMore()}
          renderItem={(item) => (
            <article className="operation-row">
              <History size={15} />
              <div><strong>Operation {item.operation_id}</strong><span>{item.applied_at} / {item.items.length} files / {item.source}</span></div>
              <div className="operation-actions">
                <button type="button" title="Export" onClick={() => void exportPhotoOperationCsv(item.operation_id).then((csv) => downloadCsv(`photo-operation-${item.operation_id}.csv`, csv))}><Download size={14} /></button>
                <button type="button" title="Revert" onClick={() => void revertPhotoOperation(item.operation_id).then(() => {
                  onStatus(`Reverted operation ${item.operation_id}`);
                  void page.reload();
                })}><RotateCcw size={14} /></button>
              </div>
            </article>
          )}
        />
      )}
    </div>
  );
}

export function PhotoMapView({ handlers }: { handlers: PhotoOpenHandlers }) {
  const container = useRef<HTMLDivElement>(null);
  const map = useRef<MapLibreMap | null>(null);
  const markers = useRef(new Map<number, maplibregl.Marker>());
  const [mapReady, setMapReady] = useState(false);
  const page = useCursorPage({
    params: null,
    resetKey: "photo-map",
    loadPage: (_, cursor) => listMapPhotos(null, cursor),
  });
  const photos = useMemo(() => page.items.map((item) => item.photo), [page.items]);
  const interaction = usePhotoInteraction({
    photos,
    handlers,
    selectFirst: false,
    onPhotoChanged: (photo) => page.updateItems((current) => current.map((item) => (
      item.photo.photo_id === photo.photo_id ? { ...item, photo } : item
    ))),
  });

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
      setMapReady(true);
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
    const firstMarker = markers.current.size === 0;
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
    if (firstMarker && page.items[0]) {
      map.current.jumpTo({ center: [page.items[0].longitude, page.items[0].latitude], zoom: 6 });
    }
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
