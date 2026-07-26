import { Image as ImageIcon, Rows3 } from "lucide-react";
import { useMemo, useState } from "react";
import { getPhotoMapping, type Photo, type PhotoTaxonMapping } from "./api";
import {
  PhotoStage,
  PhotoThumb,
  Segmented,
  VirtualGrid,
  VirtualList,
} from "./components";
import { PhotoContextMenu } from "./PhotoContextMenu";

type DisplayMode = "Thumbnails" | "Image";

type ContextState = {
  photo: Photo;
  mapping: PhotoTaxonMapping | null;
  loading: boolean;
  x: number;
  y: number;
};

export function PhotoBrowser({
  title,
  detail,
  photos,
  hasMore = false,
  onLoadMore,
  onOpenDetails,
  onOpenTaxon,
  onOpenMappingEditor,
  onPhotoChanged,
}: {
  title: string;
  detail?: string;
  photos: Photo[];
  hasMore?: boolean;
  onLoadMore?: () => void;
  onOpenDetails: (photo: Photo) => void;
  onOpenTaxon: (taxonId: number) => void;
  onOpenMappingEditor: (photo: Photo) => void;
  onPhotoChanged?: (photo: Photo) => void;
}) {
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const selected = useMemo(
    () => photos.find((photo) => photo.photo_id === selectedId) ?? photos[0] ?? null,
    [photos, selectedId],
  );
  const setSelected = (photo: Photo) => setSelectedId(photo.photo_id);
  const [mode, setMode] = useState<DisplayMode>("Thumbnails");
  const [context, setContext] = useState<ContextState | null>(null);

  const typeSelect = (query: string) => {
    const match = photos.find((photo) => photo.filename.toLocaleLowerCase().startsWith(query));
    if (match) setSelected(match);
  };

  function openContext(event: React.MouseEvent, photo: Photo) {
    setSelected(photo);
    setContext({ photo, mapping: null, loading: true, x: event.clientX, y: event.clientY });
    getPhotoMapping(photo.photo_id)
      .then((mapping) => setContext((current) => current?.photo.photo_id === photo.photo_id ? { ...current, mapping, loading: false } : current))
      .catch(() => setContext((current) => current?.photo.photo_id === photo.photo_id ? { ...current, loading: false } : current));
  }

  const status = useMemo(
    () => `${photos.length} photo${photos.length === 1 ? "" : "s"}${hasMore ? " loaded" : ""}`,
    [hasMore, photos.length],
  );

  return (
    <div className="photo-browser">
      <aside className="photo-browser-list">
        <header className="pane-header">
          <div><strong>{title}</strong><span>{detail ?? status}</span></div>
          <Rows3 size={14} />
        </header>
        <VirtualList
          items={photos}
          rowHeight={43}
          itemKey={(photo) => photo.photo_id}
          onNearEnd={onLoadMore}
          onTypeSelect={typeSelect}
          renderItem={(photo) => (
            <button
              className={`photo-list-row${selected?.photo_id === photo.photo_id ? " active" : ""}`}
              type="button"
              onClick={() => setSelected(photo)}
              onDoubleClick={() => onOpenDetails(photo)}
              onContextMenu={(event) => {
                event.preventDefault();
                openContext(event, photo);
              }}
            >
              <ImageIcon size={14} />
              <span>{photo.filename}</span>
            </button>
          )}
        />
      </aside>
      <main className="photo-browser-main">
        <header className="pane-header">
          <div><strong>{selected?.filename ?? "Photos"}</strong><span>{selected?.relative_path ?? status}</span></div>
          <Segmented value={mode} items={["Thumbnails", "Image"] as const} onChange={setMode} />
        </header>
        {mode === "Thumbnails" ? (
          <VirtualGrid
            items={photos}
            itemKey={(photo) => photo.photo_id}
            onNearEnd={onLoadMore}
            renderItem={(photo) => (
              <PhotoThumb
                photo={photo}
                selected={selected?.photo_id === photo.photo_id}
                onClick={() => setSelected(photo)}
                onContextMenu={(event) => openContext(event, photo)}
              />
            )}
          />
        ) : (
          <PhotoStage photo={selected} onContextMenu={openContext} />
        )}
      </main>
      {context && (
        <PhotoContextMenu
          {...context}
          onClose={() => setContext(null)}
          onChanged={(photo) => {
            onPhotoChanged?.(photo);
            setSelected(photo);
          }}
          onOpenDetails={() => onOpenDetails(context.photo)}
          onOpenTaxon={onOpenTaxon}
          onOpenMappingEditor={() => onOpenMappingEditor(context.photo)}
        />
      )}
    </div>
  );
}
