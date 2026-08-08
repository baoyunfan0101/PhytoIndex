import { RefreshCw, Search } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import {
  getMappingMetadata,
  listPhotosByMappingStatus,
  searchPhotosByMappingStatus,
  startPhotoMapping,
  type MappingMetadata,
  type PhotoMappingListItem,
  type PhotoTaxonStatus,
} from "../../api/mapping";
import { waitForOperation } from "../../api/tasks";
import { Button, VirtualList } from "../../shared/ui";
import { PhotoStage } from "../photos/PhotoMedia";
import { MappingBadge } from "./MappingBadge";
import { MappingEditor } from "./MappingEditor";
import { usePhotoInteraction, type PhotoOpenHandlers } from "../photos/PhotoInteraction";
import { findTypeSelectIndex, nextListIndex } from "../photos/photoListNavigation";
import { emitPhotoMutation, useDeferredPhotoMutation } from "../photos/photoMutations";
import { useCursorPage } from "../../shared/useCursorPage";
import { ResizablePanels } from "../../shared/ResizablePanels";

const statuses = ["matched", "ambiguous", "unmatched", "processing"] as const;
const emptyMetadata: MappingMetadata = {
  mapped_photo_count: 0,
  unmatched_photo_count: 0,
  ambiguous_photo_count: 0,
  processing_photo_count: 0,
  mapping_taxa_count: 0,
};

export function MappingView({
  active,
  onStatus,
  handlers,
}: {
  active: boolean;
  onStatus: (message: string, busy?: boolean) => void;
  handlers: PhotoOpenHandlers;
}) {
  const [status, setStatus] = useState<PhotoTaxonStatus>("ambiguous");
  const [metadata, setMetadata] = useState<MappingMetadata>(emptyMetadata);
  const [query, setQuery] = useState("");
  const [editorRevision, setEditorRevision] = useState(0);
  const normalizedQuery = query.trim();
  const refreshMetadata = useCallback(() => {
    void getMappingMetadata().then(setMetadata);
  }, []);
  const page = useCursorPage<PhotoMappingListItem, { status: PhotoTaxonStatus; query: string }>({
    params: { status, query: normalizedQuery },
    resetKey: `${status}:${normalizedQuery}`,
    debounceMs: normalizedQuery ? 180 : 0,
    loadPage: (params, cursor) => params.query
      ? searchPhotosByMappingStatus(params.status, params.query, cursor)
      : listPhotosByMappingStatus(params.status, cursor),
    onPageLoaded: refreshMetadata,
  });
  const photos = useMemo(() => page.items.map((item) => item.photo), [page.items]);
  const knownMapping = useCallback(
    (photo: PhotoMappingListItem["photo"]) => page.items.find((item) => item.photo.photo_id === photo.photo_id)?.mapping,
    [page.items],
  );
  const interaction = usePhotoInteraction({
    photos,
    handlers,
    knownMapping,
  });
  useDeferredPhotoMutation(active, () => {
    void page.reload();
    refreshMetadata();
    setEditorRevision((current) => current + 1);
  });
  const selected = page.items.find((item) => item.photo.photo_id === interaction.selectedId) ?? null;
  const activeIndex = page.items.findIndex((item) => item.photo.photo_id === interaction.selectedId);

  function moveSelection(direction: -1 | 1) {
    const nextIndex = nextListIndex(page.items.length, activeIndex, direction);
    if (nextIndex >= 0) interaction.selectPhoto(page.items[nextIndex].photo);
  }

  function typeSelect(query: string, shouldCycle: boolean) {
    const matchIndex = findTypeSelectIndex(
      page.items,
      query,
      (item) => [item.photo.filename, item.photo.relative_path],
      shouldCycle && activeIndex >= 0 ? activeIndex + 1 : 0,
    );
    if (matchIndex >= 0) interaction.selectPhoto(page.items[matchIndex].photo);
  }

  async function mapAll() {
    onStatus("Mapping photos", true);
    const started = await startPhotoMapping();
    await waitForOperation("mapping", started.operation.task_id, (operation) => onStatus(operation.message, true));
    emitPhotoMutation({ photoId: null, kind: "mapping" });
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
        <div className="mapping-summary" aria-label="Mapping status filters">
          {statuses.map((item) => (
            <button
              className={`mapping-summary-button${status === item ? " active" : ""}`}
              type="button"
              key={item}
              aria-pressed={status === item}
              onClick={() => setStatus(item)}
            >
              <MappingBadge status={item} />
              <span>{counts[item]}</span>
            </button>
          ))}
        </div>
        <label className="search-field mapping-filter">
          <Search size={14} />
          <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={`Search ${status} photos`} />
        </label>
        <Button onClick={() => void mapAll()}><RefreshCw size={13} />Map all</Button>
      </header>
      <ResizablePanels
        className="mapping-three-columns"
        initialSize={240}
        minFirst={180}
        minSecond={600}
        separatorLabel="Resize mapping photo list"
        stateKey="mapping.photo-list"
        first={(<aside className="mapping-photo-list">
          <VirtualList
            items={page.items}
            activeIndex={activeIndex}
            rowHeight={28}
            itemKey={(item) => item.photo.photo_id}
            onNearEnd={() => void page.loadMore()}
            onMoveActive={moveSelection}
            onTypeSelect={typeSelect}
            renderItem={(item) => (
              <button
                className={`mapping-photo-row${selected?.photo.photo_id === item.photo.photo_id ? " active" : ""}`}
                type="button"
                onClick={() => interaction.selectPhoto(item.photo)}
                onContextMenu={(event) => interaction.openContextMenu(event, item.photo)}
              >
                <span>{item.photo.filename}</span>
              </button>
            )}
          />
          {page.loading && <div className="pane-overlay">Loading</div>}
          {page.error && <div className="inline-error">{page.error}</div>}
        </aside>)}
        second={(<ResizablePanels
          className="mapping-content-columns"
          initialRatio={0.55}
          minFirst={260}
          minSecond={320}
          separatorLabel="Resize mapping photo and editor"
          stateKey="mapping.editor"
          first={(<main className="mapping-photo-stage">
            <PhotoStage photo={interaction.selected} onContextMenu={interaction.openContextMenu} />
          </main>)}
          second={(<aside className="mapping-editor-pane">
            {selected ? <MappingEditor photo={selected.photo} embedded onOpenTaxon={handlers.openTaxon} refreshKey={editorRevision} /> : <div className="empty-copy">Select a photo</div>}
          </aside>)}
        />)}
      />
      {interaction.contextMenu}
    </div>
  );
}
