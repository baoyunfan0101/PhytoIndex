import { useEffect, useRef, useState } from "react";
import { getPhotoTaxonDisplaySummary } from "../../api/mapping";
import type { TaxonDisplaySummary } from "../../api/taxonomy";
import { useTaxonomyMutation } from "../taxonomy/taxonomyMutations";
import { usePhotoMutation } from "./photoMutations";
import {
  loadSelectedPhotoTaxonSummary,
  PhotoTaxonSummaryCache,
} from "./photoTaxonSummaryCache";

export function usePhotoTaxonDisplaySummary(photoId: number | null): TaxonDisplaySummary | null {
  const cacheRef = useRef<PhotoTaxonSummaryCache | null>(null);
  if (cacheRef.current === null) {
    cacheRef.current = new PhotoTaxonSummaryCache(getPhotoTaxonDisplaySummary);
  }
  const [loaded, setLoaded] = useState<{
    key: string;
    summary: TaxonDisplaySummary | null;
  }>({ key: "", summary: null });
  const [revision, setRevision] = useState(0);
  const request = useRef(0);
  const selectionKey = `${photoId ?? "none"}:${revision}`;

  usePhotoMutation((mutation) => {
    if (mutation.kind !== "mapping") return;
    const affected = mutation.photoId === null
      || mutation.photoId === photoId
      || mutation.photoIds?.includes(photoId ?? -1);
    if (mutation.photoId === null) cacheRef.current!.invalidate();
    else {
      cacheRef.current!.invalidate(mutation.photoId);
      mutation.photoIds?.forEach((id) => cacheRef.current!.invalidate(id));
    }
    if (affected) setRevision((current) => current + 1);
  });
  useTaxonomyMutation(() => {
    cacheRef.current!.invalidate();
    if (photoId !== null) setRevision((current) => current + 1);
  });

  useEffect(() => {
    const current = ++request.current;
    let active = true;
    void loadSelectedPhotoTaxonSummary(photoId, cacheRef.current!).then((next) => {
      if (active && current === request.current) setLoaded({ key: selectionKey, summary: next });
    }).catch(() => {
      if (active && current === request.current) setLoaded({ key: selectionKey, summary: null });
    });
    return () => {
      active = false;
    };
  }, [photoId, revision, selectionKey]);

  return loaded.key === selectionKey ? loaded.summary : null;
}
