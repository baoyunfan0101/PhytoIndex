import { Image as ImageIcon, Rows3 } from "lucide-react";
import { useMemo } from "react";
import type { Photo } from "../../api/photos";
import {
  Busy,
  Segmented,
  VirtualGrid,
  VirtualList,
} from "../../shared/ui";
import { PhotoStage, PhotoThumb } from "./PhotoMedia";
import { usePhotoInteraction, type PhotoOpenHandlers } from "./PhotoInteraction";
import { usePhotoMutation } from "./photoMutations";
import { findTypeSelectIndex, nextListIndex } from "./photoListNavigation";
import type { CursorPageController } from "../../shared/useCursorPage";
import { useViewState } from "../../shared/viewState";
import { ResizablePanels } from "../../shared/ResizablePanels";

type DisplayMode = "Thumbnails" | "Image";

export function PhotoBrowser({
  title,
  detail,
  loadingLabel = "Loading photos...",
  page,
  handlers,
}: {
  title: string;
  detail?: string;
  loadingLabel?: string;
  page: CursorPageController<Photo>;
  handlers: PhotoOpenHandlers;
}) {
  const photos = page.items;
  const [mode, setMode] = useViewState<DisplayMode>("photo-browser.mode", "Thumbnails");
  const interaction = usePhotoInteraction({
    photos,
    handlers,
    stateKey: "photo-browser.interaction",
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
            rowHeight={28}
            itemKey={(photo) => photo.photo_id}
            onNearEnd={() => void page.loadMore()}
            onMoveActive={moveSelection}
            onTypeSelect={typeSelect}
            renderItem={(photo) => (
              <button
                className={`photo-list-row${interaction.selectedId === photo.photo_id ? " active" : ""}`}
                type="button"
                onClick={() => interaction.selectPhoto(photo)}
                onDoubleClick={() => handlers.openDetails(photo)}
                onContextMenu={(event) => interaction.openContextMenu(event, photo)}
              >
                <ImageIcon size={14} />
                <span>{photo.filename}</span>
              </button>
            )}
          />
        </aside>)}
        second={(<main className="photo-browser-main">
          <header className="pane-header">
            <div><strong>{interaction.selected?.filename ?? "Photos"}</strong><span>{interaction.selected?.relative_path ?? status}</span></div>
            <Segmented value={mode} items={["Thumbnails", "Image"] as const} onChange={setMode} />
          </header>
          {page.loading && photos.length === 0 ? (
            <div className="photo-browser-loading" role="status" aria-live="polite"><Busy label={loadingLabel} /></div>
          ) : mode === "Thumbnails" ? (
            <VirtualGrid
              stateKey="photo-browser.grid"
              items={photos}
              itemKey={(photo) => photo.photo_id}
              onNearEnd={() => void page.loadMore()}
              renderItem={(photo) => (
                <PhotoThumb
                  photo={photo}
                  selected={interaction.selectedId === photo.photo_id}
                  onClick={() => interaction.selectPhoto(photo)}
                  onContextMenu={(event) => interaction.openContextMenu(event, photo)}
                />
              )}
            />
          ) : (
            <PhotoStage photo={interaction.selected} onContextMenu={interaction.openContextMenu} />
          )}
        </main>)}
      />
      {interaction.contextMenu}
    </>
  );
}
