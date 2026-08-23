import { Image as ImageIcon, Rows3 } from "lucide-react";
import { useMemo } from "react";
import type { Photo } from "../../api/photos";
import {
  Busy,
  VirtualList,
} from "../../shared/ui";
import { usePhotoInteraction, type PhotoOpenHandlers } from "./PhotoInteraction";
import { usePhotoMutation } from "./photoMutations";
import { findTypeSelectIndex, nextListIndex } from "./photoListNavigation";
import type { CursorPageController } from "../../shared/useCursorPage";
import { ResizablePanels } from "../../shared/ResizablePanels";
import { PhotoDisplay, PhotoDisplayToggle, usePhotoActivation, usePhotoDisplayMode } from "./PhotoDisplay";
import { PhotoPaneHeader } from "./PhotoPaneHeader";
import {
  usePublishedPhotoTaxonSummary,
  type PhotoTaxonDisplayState,
} from "./photoTaxonSummary";

export function PhotoBrowser({
  title,
  detail,
  loadingLabel = "Loading photos...",
  page,
  handlers,
  active,
  onPhotoTaxonDisplayState,
}: {
  title: string;
  detail?: string;
  loadingLabel?: string;
  page: CursorPageController<Photo>;
  handlers: PhotoOpenHandlers;
  active: boolean;
  onPhotoTaxonDisplayState: (state: PhotoTaxonDisplayState | null) => void;
}) {
  const photos = page.items;
  const interaction = usePhotoInteraction({
    photos,
    handlers,
    stateKey: "photo-browser.interaction",
  });
  const [mode, setMode] = usePhotoDisplayMode({
    onEnterFullscreen: () => {
      if (interaction.selected) handlers.openFullscreen(interaction.selected);
    },
  });
  usePublishedPhotoTaxonSummary({
    photoId: interaction.selectedId,
    active,
    onChange: onPhotoTaxonDisplayState,
  });
  const activation = usePhotoActivation({
    onSelect: interaction.selectPhoto,
    onOpenImage: () => setMode("image"),
    onOpenDetails: handlers.openDetails,
  });
  usePhotoMutation(() => {
    void page.reload();
  });

  const activeIndex = photos.findIndex((photo) => photo.photo_id === interaction.selectedId);

  const moveSelection = (direction: -1 | 1) => {
    const nextIndex = nextListIndex(photos.length, activeIndex, direction);
    if (nextIndex >= 0) interaction.selectPhoto(photos[nextIndex]);
  };

  const typeSelect = (query: string, shouldCycle: boolean) => {
    const matchIndex = findTypeSelectIndex(
      photos,
      query,
      (photo) => [photo.filename, photo.relative_path],
      shouldCycle && activeIndex >= 0 ? activeIndex + 1 : 0,
    );
    if (matchIndex >= 0) interaction.selectPhoto(photos[matchIndex]);
  };

  const status = useMemo(
    () => `${photos.length} photo${photos.length === 1 ? "" : "s"}${page.hasMore ? " loaded" : ""}`,
    [page.hasMore, photos.length],
  );

  return (
    <>
      <ResizablePanels
        className="photo-browser"
        initialRatio={0.25}
        minFirst={180}
        minSecond={360}
        separatorLabel="Resize photo list and photo browser"
        stateKey="photo-browser.columns"
        first={(<aside className="photo-browser-list">
          <header className="pane-header">
            <div><strong>{title}</strong><span>{detail ?? status}</span></div>
            <Rows3 size={14} />
          </header>
          <VirtualList
            stateKey="photo-browser.list"
            items={photos}
            activeIndex={activeIndex}
            focusWhen={mode === "thumbnails"}
            rowHeight={28}
            itemKey={(photo) => photo.photo_id}
            onNearEnd={() => void page.loadMore()}
            onActivateActive={() => {
              if (activeIndex >= 0) setMode("image");
            }}
            onMoveActive={moveSelection}
            onTypeSelect={typeSelect}
            renderItem={(photo) => (
              <button
                className={`photo-list-row${interaction.selectedId === photo.photo_id ? " active" : ""}`}
                type="button"
                onClick={() => activation.clickPhoto(photo)}
                onDoubleClick={() => activation.doubleClickPhoto(photo)}
                onContextMenu={(event) => interaction.openContextMenu(event, photo)}
              >
                <ImageIcon size={14} />
                <span>{photo.filename}</span>
              </button>
            )}
          />
        </aside>)}
        second={(<main className="photo-browser-main">
          <header className="pane-header photo-pane-heading">
            {interaction.selected ? <PhotoPaneHeader photo={interaction.selected} /> : <div><strong>Photos</strong><span>{status}</span></div>}
            {page.loading && photos.length > 0 && <small className="pane-loading-label">Loading...</small>}
            <PhotoDisplayToggle mode={mode} onChange={setMode} />
          </header>
          {page.loading && photos.length === 0 ? (
            <div className="photo-browser-loading" role="status" aria-live="polite"><Busy label={loadingLabel} /></div>
          ) : (
            <PhotoDisplay
              photos={photos}
              selected={interaction.selected}
              mode={mode}
              stateKey="photo-browser.grid"
              onModeChange={setMode}
              onSelect={interaction.selectPhoto}
              onClickPhoto={activation.clickPhoto}
              onDoubleClickPhoto={activation.doubleClickPhoto}
              onNearEnd={() => void page.loadMore()}
              onContextMenu={interaction.openContextMenu}
            />
          )}
        </main>)}
      />
      {interaction.contextMenu}
    </>
  );
}
