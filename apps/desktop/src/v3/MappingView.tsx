import { RefreshCw, Search } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import {
  errorMessage,
  getMappingMetadata,
  listPhotosByMappingStatus,
  searchPhotosByMappingStatus,
  startPhotoMapping,
  waitForOperation,
  type MappingMetadata,
  type PhotoMappingListItem,
  type PhotoTaxonStatus,
} from "./api";
import { MappingBadge, PhotoStage, Segmented, VirtualList } from "./components";
import { MappingEditor } from "./MappingEditor";
import { PhotoContextMenu } from "./PhotoContextMenu";
import type { PhotoOpenHandlers } from "./PhotosView";

const statuses = ["matched", "ambiguous", "unmatched", "processing"] as const;
const emptyMetadata: MappingMetadata = {
  mapped_photo_count: 0,
  unmatched_photo_count: 0,
  ambiguous_photo_count: 0,
  processing_photo_count: 0,
  mapping_taxa_count: 0,
};

export function MappingView({
  onStatus,
  handlers,
}: {
  onStatus: (message: string, busy?: boolean) => void;
  handlers: PhotoOpenHandlers;
}) {
  const [status, setStatus] = useState<PhotoTaxonStatus>("ambiguous");
  const [metadata, setMetadata] = useState<MappingMetadata>(emptyMetadata);
  const [items, setItems] = useState<PhotoMappingListItem[]>([]);
  const [selected, setSelected] = useState<PhotoMappingListItem | null>(null);
  const [query, setQuery] = useState("");
  const [cursor, setCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [context, setContext] = useState<{ item: PhotoMappingListItem; x: number; y: number } | null>(null);

  const load = useCallback(async (append = false) => {
    if (loading) return;
    setLoading(true);
    setError("");
    try {
      const nextCursor = append ? cursor : null;
      const page = query.trim()
        ? await searchPhotosByMappingStatus(status, query.trim(), nextCursor)
        : await listPhotosByMappingStatus(status, nextCursor);
      setItems((current) => append ? [...current, ...page.items] : page.items);
      setCursor(page.next_cursor);
      if (!append) setSelected(page.items[0] ?? null);
      setMetadata(await getMappingMetadata());
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setLoading(false);
    }
  }, [cursor, loading, query, status]);

  useEffect(() => {
    const timer = window.setTimeout(() => void load(false), query ? 180 : 0);
    return () => window.clearTimeout(timer);
  }, [query, status]);

  async function mapAll() {
    onStatus("Mapping photos", true);
    const started = await startPhotoMapping();
    await waitForOperation("mapping", started.operation.task_id, (operation) => onStatus(operation.message, true));
    await load(false);
    onStatus("Mapping complete");
  }

  const counts: Record<PhotoTaxonStatus, number> = {
    matched: metadata.mapped_photo_count,
    ambiguous: metadata.ambiguous_photo_count,
    unmatched: metadata.unmatched_photo_count,
    processing: metadata.processing_photo_count,
  };

  return (
    <div className="mapping-workbench">
      <header className="workbench-toolbar">
        <Segmented value={status} items={statuses} onChange={setStatus} />
        <label className="search-field mapping-filter">
          <Search size={14} />
          <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={`Search ${status} photos`} />
        </label>
        <button className="secondary-button" type="button" onClick={() => void mapAll()}><RefreshCw size={13} />Map all</button>
      </header>
      <div className="mapping-summary">
        {statuses.map((item) => <span key={item}><MappingBadge status={item} />{counts[item]}</span>)}
      </div>
      <div className="mapping-three-columns">
        <aside className="mapping-photo-list">
          <VirtualList
            items={items}
            rowHeight={54}
            itemKey={(item) => item.photo.photo_id}
            onNearEnd={() => { if (cursor) void load(true); }}
            renderItem={(item) => (
              <button
                className={`mapping-photo-row${selected?.photo.photo_id === item.photo.photo_id ? " active" : ""}`}
                type="button"
                onClick={() => setSelected(item)}
                onContextMenu={(event) => {
                  event.preventDefault();
                  setSelected(item);
                  setContext({ item, x: event.clientX, y: event.clientY });
                }}
              >
                <span>{item.photo.filename}</span><MappingBadge status={item.mapping.status} />
              </button>
            )}
          />
          {loading && <div className="pane-overlay">Loading</div>}
          {error && <div className="inline-error">{error}</div>}
        </aside>
        <main className="mapping-photo-stage"><PhotoStage photo={selected?.photo ?? null} onContextMenu={(event) => {
          if (selected) setContext({ item: selected, x: event.clientX, y: event.clientY });
        }} /></main>
        <aside className="mapping-editor-pane">
          {selected ? <MappingEditor photo={selected.photo} embedded onChanged={() => void load(false)} /> : <div className="empty-copy">Select a photo</div>}
        </aside>
      </div>
      {context && (
        <PhotoContextMenu
          photo={context.item.photo}
          mapping={context.item.mapping}
          loading={false}
          x={context.x}
          y={context.y}
          onClose={() => setContext(null)}
          onChanged={() => void load(false)}
          onOpenDetails={() => handlers.openDetails(context.item.photo)}
          onOpenTaxon={handlers.openTaxon}
          onOpenMappingEditor={() => handlers.openMappingEditor(context.item.photo)}
        />
      )}
    </div>
  );
}
