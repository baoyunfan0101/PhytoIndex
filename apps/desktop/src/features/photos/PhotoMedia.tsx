import { Camera, LoaderCircle } from "lucide-react";
import { useEffect, useState } from "react";
import { photoUrl, type Photo } from "../../api/photos";

export function PhotoStage({
  photo,
  compact = false,
  onContextMenu,
}: {
  photo: Photo | null;
  compact?: boolean;
  onContextMenu?: (event: React.MouseEvent, photo: Photo) => void;
}) {
  const [loaded, setLoaded] = useState(false);
  useEffect(() => setLoaded(false), [photo?.photo_id]);

  if (!photo) {
    return (
      <div className="photo-stage empty">
        <Camera size={28} />
        <span>Select a photo</span>
      </div>
    );
  }
  return (
    <div
      className={`photo-stage${compact ? " compact" : ""}`}
      onContextMenu={(event) => {
        event.preventDefault();
        onContextMenu?.(event, photo);
      }}
    >
      {!loaded && <LoaderCircle className="spin photo-loader" size={20} />}
      <img
        src={photoUrl(photo)}
        alt={photo.filename}
        draggable={false}
        onLoad={() => setLoaded(true)}
      />
      <div className="photo-stage-caption">{photo.filename}</div>
    </div>
  );
}

export function PhotoThumb({
  photo,
  selected,
  onClick,
  onContextMenu,
}: {
  photo: Photo;
  selected: boolean;
  onClick: () => void;
  onContextMenu?: (event: React.MouseEvent) => void;
}) {
  return (
    <button
      className={`photo-thumb${selected ? " selected" : ""}`}
      type="button"
      onClick={onClick}
      onContextMenu={(event) => {
        event.preventDefault();
        onContextMenu?.(event);
      }}
    >
      <img src={photoUrl(photo, true)} alt="" loading="lazy" draggable={false} />
      <span>{photo.filename}</span>
    </button>
  );
}
