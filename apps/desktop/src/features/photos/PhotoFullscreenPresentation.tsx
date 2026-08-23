import { useEffect } from "react";
import type { Photo } from "../../api/photos";
import { PhotoStage } from "./PhotoMedia";
import { usePhotoInteraction, type PhotoOpenHandlers } from "./PhotoInteraction";

export function PhotoFullscreenPresentation({
  photo,
  handlers,
  onExit,
}: {
  photo: Photo;
  handlers: PhotoOpenHandlers;
  onExit: () => void;
}) {
  const interaction = usePhotoInteraction({
    photos: [photo],
    handlers,
    stateKey: "photo-fullscreen.interaction",
  });

  useEffect(() => {
    const exitOnEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      event.preventDefault();
      void onExit();
    };
    window.addEventListener("keydown", exitOnEscape, true);
    return () => window.removeEventListener("keydown", exitOnEscape, true);
  }, [onExit]);

  return (
    <div className="photo-fullscreen-presentation">
      <PhotoStage photo={photo} onContextMenu={interaction.openContextMenu} />
      {interaction.contextMenu}
    </div>
  );
}
