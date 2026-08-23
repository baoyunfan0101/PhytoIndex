import { Image as ImageIcon, LayoutGrid } from "lucide-react";
import { useCallback, useEffect, useRef, useState, type MouseEvent } from "react";
import type { Photo } from "../../api/photos";
import { IconButton, VirtualGrid } from "../../shared/ui";
import { PhotoStage, PhotoThumb } from "./PhotoMedia";

export type PhotoDisplayMode = "thumbnails" | "image";
const photoDoubleClickDelayMs = 250;

export function usePhotoActivation({
  onSelect,
  onOpenImage,
  onOpenDetails,
}: {
  onSelect: (photo: Photo) => void;
  onOpenImage: (photo: Photo) => void;
  onOpenDetails: (photo: Photo) => void;
}) {
  const singleClickTimer = useRef<number | null>(null);

  useEffect(() => () => {
    if (singleClickTimer.current !== null) window.clearTimeout(singleClickTimer.current);
  }, []);

  const clickPhoto = useCallback((photo: Photo) => {
    onSelect(photo);
    if (singleClickTimer.current !== null) window.clearTimeout(singleClickTimer.current);
    singleClickTimer.current = window.setTimeout(() => {
      singleClickTimer.current = null;
      onOpenImage(photo);
    }, photoDoubleClickDelayMs);
  }, [onOpenImage, onSelect]);

  const doubleClickPhoto = useCallback((photo: Photo) => {
    if (singleClickTimer.current !== null) {
      window.clearTimeout(singleClickTimer.current);
      singleClickTimer.current = null;
    }
    onSelect(photo);
    onOpenDetails(photo);
  }, [onOpenDetails, onSelect]);

  return { clickPhoto, doubleClickPhoto };
}

export function usePhotoDisplayMode({
  onEscapeToThumbnails,
}: {
  onEscapeToThumbnails?: () => void;
} = {}) {
  const [mode, setMode] = useState<PhotoDisplayMode>("thumbnails");
  const modeRef = useRef(mode);
  const onEscapeToThumbnailsRef = useRef(onEscapeToThumbnails);
  modeRef.current = mode;
  onEscapeToThumbnailsRef.current = onEscapeToThumbnails;

  useEffect(() => {
    const returnToThumbnails = (event: KeyboardEvent) => {
      if (modeRef.current !== "image" || event.key !== "Escape" || event.defaultPrevented) return;
      event.preventDefault();
      setMode("thumbnails");
      onEscapeToThumbnailsRef.current?.();
    };
    window.addEventListener("keydown", returnToThumbnails, true);
    return () => window.removeEventListener("keydown", returnToThumbnails, true);
  }, []);

  return [mode, setMode] as const;
}

export function PhotoDisplayToggle({
  mode,
  onChange,
}: {
  mode: PhotoDisplayMode;
  onChange: (mode: PhotoDisplayMode) => void;
}) {
  return (
    <div className="photo-display-toggle" role="group" aria-label="Photo display">
      <IconButton
        aria-label="Thumbnails"
        className={mode === "thumbnails" ? "active" : ""}
        size="small"
        title="Thumbnails"
        onClick={() => onChange("thumbnails")}
      >
        <LayoutGrid size={14} />
      </IconButton>
      <IconButton
        aria-label="Image"
        className={mode === "image" ? "active" : ""}
        size="small"
        title="Image"
        onClick={() => onChange("image")}
      >
        <ImageIcon size={14} />
      </IconButton>
    </div>
  );
}

export function PhotoDisplay({
  photos,
  selected,
  mode,
  stateKey,
  onModeChange,
  onSelect,
  onClickPhoto,
  onDoubleClickPhoto,
  onNearEnd,
  onContextMenu,
}: {
  photos: Photo[];
  selected: Photo | null;
  mode: PhotoDisplayMode;
  stateKey: string;
  onModeChange: (mode: PhotoDisplayMode) => void;
  onSelect: (photo: Photo) => void;
  onClickPhoto: (photo: Photo) => void;
  onDoubleClickPhoto: (photo: Photo) => void;
  onNearEnd?: () => void;
  onContextMenu?: (event: MouseEvent, photo: Photo) => void;
}) {
  const activeIndex = selected
    ? photos.findIndex((photo) => photo.photo_id === selected.photo_id)
    : -1;

  if (mode === "image") {
    return <PhotoStage photo={selected} onContextMenu={onContextMenu} />;
  }

  return (
    <VirtualGrid
      stateKey={stateKey}
      items={photos}
      activeIndex={activeIndex}
      itemKey={(photo) => photo.photo_id}
      onActivateActive={() => {
        if (activeIndex >= 0) onModeChange("image");
      }}
      onMoveActive={(index) => onSelect(photos[index])}
      onNearEnd={onNearEnd}
      renderItem={(photo) => (
        <PhotoThumb
          photo={photo}
          selected={selected?.photo_id === photo.photo_id}
          onClick={() => onClickPhoto(photo)}
          onDoubleClick={() => onDoubleClickPhoto(photo)}
          onContextMenu={(event) => onContextMenu?.(event, photo)}
        />
      )}
    />
  );
}
