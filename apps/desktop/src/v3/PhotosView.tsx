import "maplibre-gl/dist/maplibre-gl.css";

import { ChevronRight, Download, Folder, History, Images, RefreshCw, RotateCcw } from "lucide-react";
import maplibregl, { type Map as MapLibreMap } from "maplibre-gl";
import { useCallback, useEffect, useRef, useState } from "react";
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
  getPhotoMapping,
  listMapPhotos,
  listPhotoOperations,
  refreshPhotoDirectory,
  revertPhotoOperation,
  waitForOperation,
  type Photo,
  type PhotoDirectory,
  type PhotoLibrary,
  type PhotoOperation,
  type PhotoTaxonMapping,
  type PhotoTaxonUsage,
} from "./api";
import { EmptyState, PhotoStage, SectionHeader, VirtualList } from "./components";
import { PhotoContextMenu } from "./PhotoContextMenu";

export type PhotoOpenHandlers = {
  openDetails: (photo: Photo) => void;
  openTaxon: (taxonId: number) => void;
  openMappingEditor: (photo: Photo) => void;
};

export function FolderPhotosView({
  handlers,
  onStatus,
}: {
  handlers: PhotoOpenHandlers;
  onStatus: (message: string, busy?: boolean) => void;
}) {
  const [library, setLibrary] = useState<PhotoLibrary | null>(null);
  const [trail, setTrail] = useState<PhotoDirectory[]>([]);
  const [directories, setDirectories] = useState<PhotoDirectory[]>([]);
  const [photos, setPhotos] = useState<Photo[]>([]);
  const [selected, setSelected] = useState<Photo | null>(null);
  const [context, setContext] = useState<{ photo: Photo; mapping: PhotoTaxonMapping | null; loading: boolean; x: number; y: number } | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const directoryId = trail[trail.length - 1]?.directory_id ?? library?.root_directory_id ?? null;

  const load = useCallback(async (id: number) => {
    setLoading(true);
    setError("");
    try {
      const [page, counts] = await Promise.all([browsePhotoDirectory(id), getPhotoDirectoryCounts(id)]);
      setDirectories(page.items.flatMap((item) => item.kind === "directory" ? [item.directory] : []));
      const nextPhotos = page.items.flatMap((item) => item.kind === "photo" ? [item.photo] : []);
      setPhotos(nextPhotos);
      setSelected(nextPhotos[0] ?? null);
      onStatus(`${counts.directory_count} folders, ${counts.file_count} photos`);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setLoading(false);
    }
  }, [onStatus]);

  useEffect(() => {
    getPhotoLibrary().then((next) => {
      setLibrary(next);
      if (next) return load(next.root_directory_id);
      setLoading(false);
    }).catch((nextError) => {
      setError(errorMessage(nextError));
      setLoading(false);
    });
  }, [load]);

  async function refresh() {
    if (directoryId === null) return;
    onStatus("Refreshing photo library", true);
    const started = await refreshPhotoDirectory(directoryId);
    await waitForOperation("photos", started.operation.task_id, (operation) => onStatus(operation.message, true));
    await load(directoryId);
  }

  function enter(directory: PhotoDirectory) {
    setTrail((current) => [...current, directory]);
    void load(directory.directory_id);
  }

  function openContext(event: React.MouseEvent, photo: Photo) {
    setSelected(photo);
    setContext({ photo, mapping: null, loading: true, x: event.clientX, y: event.clientY });
    void getPhotoMapping(photo.photo_id).then((mapping) => setContext((current) => current?.photo.photo_id === photo.photo_id ? { ...current, mapping, loading: false } : current));
  }

  return (
    <div className="folder-workbench">
      <header className="workbench-toolbar">
        <div className="breadcrumbs">
          <button type="button" onClick={() => {
            setTrail([]);
            if (library) void load(library.root_directory_id);
          }}>Root</button>
          {trail.map((item, index) => (
            <span key={item.directory_id}><ChevronRight size={12} /><button type="button" onClick={() => {
              setTrail(trail.slice(0, index + 1));
              void load(item.directory_id);
            }}>{item.name}</button></span>
          ))}
        </div>
        <button className="icon-button" type="button" onClick={() => void refresh()} title="Refresh"><RefreshCw size={14} /></button>
      </header>
      <div className="explorer-columns">
        <aside className="finder-pane">
          <VirtualList
            items={[
              ...directories.map((directory) => ({ kind: "directory" as const, directory })),
              ...photos.map((photo) => ({ kind: "photo" as const, photo })),
            ]}
            rowHeight={40}
            itemKey={(item) => item.kind === "directory" ? `d:${item.directory.directory_id}` : `p:${item.photo.photo_id}`}
            renderItem={(item) => (
              item.kind === "directory" ? (
                <button className="finder-row" type="button" onDoubleClick={() => enter(item.directory)}>
                  <Folder size={14} /><span>{item.directory.name}</span><ChevronRight size={12} />
                </button>
              ) : (
                <button
                  className={`finder-row${selected?.photo_id === item.photo.photo_id ? " active" : ""}`}
                  type="button"
                  onClick={() => setSelected(item.photo)}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    openContext(event, item.photo);
                  }}
                >
                  <Images size={14} /><span>{item.photo.filename}</span>
                </button>
              )
            )}
          />
          {loading && <div className="pane-overlay">Loading</div>}
          {error && <div className="inline-error">{error}</div>}
        </aside>
        <PhotoStage photo={selected} onContextMenu={openContext} />
      </div>
      {context && <PhotoContextMenu {...context} onClose={() => setContext(null)} onChanged={(photo) => setSelected(photo)} onOpenDetails={() => handlers.openDetails(context.photo)} onOpenTaxon={handlers.openTaxon} onOpenMappingEditor={() => handlers.openMappingEditor(context.photo)} />}
    </div>
  );
}

export function TaxonPhotosView({ handlers }: { handlers: PhotoOpenHandlers }) {
  const [trail, setTrail] = useState<PhotoTaxonUsage[]>([]);
  const [taxa, setTaxa] = useState<PhotoTaxonUsage[]>([]);
  const [photos, setPhotos] = useState<Photo[]>([]);
  const [selected, setSelected] = useState<Photo | null>(null);
  const [context, setContext] = useState<{ photo: Photo; mapping: PhotoTaxonMapping | null; loading: boolean; x: number; y: number } | null>(null);
  const currentId = trail[trail.length - 1]?.taxon_id ?? null;

  const load = useCallback(async (taxonId: number | null) => {
    const page = await browsePhotoTaxon(taxonId);
    setTaxa(page.items.flatMap((item) => item.kind === "taxon" ? [item.taxon] : []));
    setPhotos(page.items.flatMap((item) => item.kind === "photo" ? [item.photo] : []));
    setSelected(page.items.flatMap((item) => item.kind === "photo" ? [item.photo] : [])[0] ?? null);
  }, []);

  useEffect(() => { void load(currentId); }, [currentId, load]);

  function openContext(event: React.MouseEvent, photo: Photo) {
    setSelected(photo);
    setContext({ photo, mapping: null, loading: true, x: event.clientX, y: event.clientY });
    void getPhotoMapping(photo.photo_id).then((mapping) => setContext((current) => current?.photo.photo_id === photo.photo_id ? { ...current, mapping, loading: false } : current));
  }

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
            items={[
              ...taxa.map((taxon) => ({ kind: "taxon" as const, taxon })),
              ...photos.map((photo) => ({ kind: "photo" as const, photo })),
            ]}
            rowHeight={48}
            itemKey={(item) => item.kind === "taxon" ? `t:${item.taxon.taxon_id}` : `p:${item.photo.photo_id}`}
            renderItem={(item) => (
              item.kind === "taxon" ? (
                <button className="finder-row taxon" type="button" onClick={() => setTrail((current) => [...current, item.taxon])}>
                  <span><strong>{item.taxon.names.sci_name ?? `Taxon ${item.taxon.taxon_id}`}</strong><small>{item.taxon.subtree_photo_count} photos</small></span><ChevronRight size={12} />
                </button>
              ) : (
                <button className={`finder-row${selected?.photo_id === item.photo.photo_id ? " active" : ""}`} type="button" onClick={() => setSelected(item.photo)} onContextMenu={(event) => {
                  event.preventDefault();
                  openContext(event, item.photo);
                }}><Images size={14} /><span>{item.photo.filename}</span></button>
              )
            )}
          />
        </aside>
        <PhotoStage photo={selected} onContextMenu={openContext} />
      </div>
      {context && <PhotoContextMenu {...context} onClose={() => setContext(null)} onChanged={(photo) => setSelected(photo)} onOpenDetails={() => handlers.openDetails(context.photo)} onOpenTaxon={handlers.openTaxon} onOpenMappingEditor={() => handlers.openMappingEditor(context.photo)} />}
    </div>
  );
}

export function PhotoHistoryView({ onStatus }: { onStatus: (message: string) => void }) {
  const [items, setItems] = useState<PhotoOperation[]>([]);
  const [error, setError] = useState("");

  const load = useCallback(() => {
    listPhotoOperations().then((page) => setItems(page.items)).catch((nextError) => setError(errorMessage(nextError)));
  }, []);
  useEffect(load, [load]);

  async function exportAll() {
    downloadCsv("photo-rename-operations.csv", await exportAllPhotoOperationsCsv());
  }

  return (
    <div className="history-view">
      <SectionHeader title="Rename history" detail={`${items.length} operations`} actions={
        <button className="secondary-button" type="button" onClick={() => void exportAll()}><Download size={13} />Export all</button>
      } />
      {error ? <EmptyState title="Unable to load history" detail={error} /> : (
        <VirtualList
          className="history-list"
          items={items}
          rowHeight={72}
          itemKey={(item) => item.operation_id}
          renderItem={(item) => (
            <article className="operation-row">
              <History size={15} />
              <div><strong>Operation {item.operation_id}</strong><span>{item.applied_at} / {item.items.length} files / {item.source}</span></div>
              <div className="operation-actions">
                <button type="button" title="Export" onClick={() => void exportPhotoOperationCsv(item.operation_id).then((csv) => downloadCsv(`photo-operation-${item.operation_id}.csv`, csv))}><Download size={14} /></button>
                <button type="button" title="Revert" onClick={() => void revertPhotoOperation(item.operation_id).then(() => {
                  onStatus(`Reverted operation ${item.operation_id}`);
                  load();
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
  const [photos, setPhotos] = useState<Awaited<ReturnType<typeof listMapPhotos>>["items"]>([]);
  const [selected, setSelected] = useState<Photo | null>(null);
  const [context, setContext] = useState<{ photo: Photo; mapping: PhotoTaxonMapping | null; loading: boolean; x: number; y: number } | null>(null);

  function openContext(event: React.MouseEvent, photo: Photo) {
    setContext({ photo, mapping: null, loading: true, x: event.clientX, y: event.clientY });
    void getPhotoMapping(photo.photo_id).then((mapping) => setContext((current) => current?.photo.photo_id === photo.photo_id ? { ...current, mapping, loading: false } : current));
  }

  useEffect(() => {
    let disposed = false;
    Promise.all([getMapSettings(), listMapPhotos()]).then(([settings, page]) => {
      if (disposed || !container.current) return;
      setPhotos(page.items);
      const rasterUrl = settings.provider === "tianditu" && settings.tianditu_token
        ? `https://t0.tianditu.gov.cn/vec_w/wmts?tk=${settings.tianditu_token}&service=wmts&request=gettile&version=1.0.0&layer=vec&style=default&tilematrixset=w&format=tiles&tilematrix={z}&tilerow={y}&tilecol={x}`
        : "https://tile.openstreetmap.org/{z}/{x}/{y}.png";
      const next = new maplibregl.Map({
        container: container.current,
        center: page.items.length ? [page.items[0].longitude, page.items[0].latitude] : [0, 20],
        zoom: page.items.length ? 6 : 1.5,
        style: { version: 8, sources: { tiles: { type: "raster", tiles: [rasterUrl], tileSize: 256 } }, layers: [{ id: "tiles", type: "raster", source: "tiles" }] },
      });
      map.current = next;
      page.items.forEach((item) => {
        const marker = document.createElement("button");
        marker.className = "map-photo-marker";
        marker.type = "button";
        marker.title = item.photo.filename;
        marker.addEventListener("click", () => setSelected(item.photo));
        new maplibregl.Marker({ element: marker }).setLngLat([item.longitude, item.latitude]).addTo(next);
      });
    });
    return () => {
      disposed = true;
      map.current?.remove();
      map.current = null;
    };
  }, []);

  return (
    <div className="map-view">
      <div className="map-canvas" ref={container} />
      {selected && (
        <div className="map-photo-float">
          <PhotoStage photo={selected} compact onContextMenu={openContext} />
          <div className="map-float-actions">
            <button type="button" onClick={() => handlers.openDetails(selected)}>Open details</button>
            <button type="button" onClick={() => handlers.openMappingEditor(selected)}>Edit mapping</button>
          </div>
        </div>
      )}
      <span className="map-count">{photos.length} geotagged photos</span>
      {context && <PhotoContextMenu {...context} onClose={() => setContext(null)} onChanged={(photo) => setSelected(photo)} onOpenDetails={() => handlers.openDetails(context.photo)} onOpenTaxon={handlers.openTaxon} onOpenMappingEditor={() => handlers.openMappingEditor(context.photo)} />}
    </div>
  );
}
