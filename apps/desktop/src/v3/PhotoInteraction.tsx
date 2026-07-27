import { useCallback, useMemo, useState, type MouseEvent } from "react";
import { getPhotoMapping, type Photo, type PhotoTaxonMapping } from "./api";
import { PhotoContextMenu } from "./PhotoContextMenu";

export type PhotoOpenHandlers = {
  openDetails: (photo: Photo) => void;
  openTaxon: (taxonId: number) => void;
  openMappingEditor: (photo: Photo) => void;
};

type PhotoContextState = {
  photo: Photo;
  mapping: PhotoTaxonMapping | null;
  loading: boolean;
  x: number;
  y: number;
};

export function usePhotoInteraction({
  photos,
  handlers,
  onPhotoChanged,
  onMappingChanged,
  knownMapping,
  selectFirst = true,
}: {
  photos: Photo[];
  handlers: PhotoOpenHandlers;
  onPhotoChanged?: (photo: Photo) => void;
  onMappingChanged?: () => void;
  knownMapping?: (photo: Photo) => PhotoTaxonMapping | null | undefined;
  selectFirst?: boolean;
}) {
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [context, setContext] = useState<PhotoContextState | null>(null);
  const selected = useMemo(
    () => photos.find((photo) => photo.photo_id === selectedId) ?? (selectFirst ? photos[0] : null) ?? null,
    [photos, selectFirst, selectedId],
  );
  const selectPhoto = useCallback((photo: Photo) => setSelectedId(photo.photo_id), []);

  const openContextMenu = useCallback((event: MouseEvent, photo: Photo) => {
    event.preventDefault();
    selectPhoto(photo);
    const mapping = knownMapping?.(photo);
    const loading = mapping === undefined;
    setContext({ photo, mapping: mapping ?? null, loading, x: event.clientX, y: event.clientY });
    if (!loading) return;
    void getPhotoMapping(photo.photo_id)
      .then((nextMapping) => setContext((current) => (
        current?.photo.photo_id === photo.photo_id
          ? { ...current, mapping: nextMapping, loading: false }
          : current
      )))
      .catch(() => setContext((current) => (
        current?.photo.photo_id === photo.photo_id
          ? { ...current, loading: false }
          : current
      )));
  }, [knownMapping, selectPhoto]);

  const contextMenu = context ? (
    <PhotoContextMenu
      {...context}
      onClose={() => setContext(null)}
      onChanged={(photo) => {
        selectPhoto(photo);
        onPhotoChanged?.(photo);
      }}
      onMappingChanged={() => onMappingChanged?.()}
      onOpenDetails={() => handlers.openDetails(context.photo)}
      onOpenTaxon={handlers.openTaxon}
      onOpenMappingEditor={() => handlers.openMappingEditor(context.photo)}
    />
  ) : null;

  return {
    selected,
    selectedId: selected?.photo_id ?? null,
    selectPhoto,
    openContextMenu,
    contextMenu,
  };
}
