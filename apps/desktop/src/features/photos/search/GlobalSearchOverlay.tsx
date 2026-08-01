import { useEffect } from "react";
import { PhotoSearch } from "./PhotoSearch";
import { usePhotoSearch, type SearchSubmitter } from "./usePhotoSearch";

export function GlobalSearchOverlay({
  onClose,
  onSubmit,
  suggestionsEnabled,
}: {
  onClose: () => void;
  onSubmit: SearchSubmitter;
  suggestionsEnabled: boolean;
}) {
  const search = usePhotoSearch(onSubmit);

  useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [onClose]);

  return (
    <div
      className="global-search-overlay"
      role="presentation"
      onPointerDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="global-search-dialog" role="dialog" aria-label="Search photos" aria-modal="true">
        <PhotoSearch
          autoFocus
          controller={search}
          enabled={suggestionsEnabled}
          idPrefix="global-photo-search"
        />
      </div>
    </div>
  );
}
