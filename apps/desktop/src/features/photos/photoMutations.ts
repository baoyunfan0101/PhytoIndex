import { useEffect, useRef } from "react";
import type { Photo } from "../../api/photos";

export type PhotoMutation = {
  photoId: number | null;
  photoIds?: number[];
  kind: "photo" | "mapping";
  photo?: Photo;
};

type PhotoMutationListener = (mutation: PhotoMutation) => void;

const listeners = new Set<PhotoMutationListener>();

export function emitPhotoMutation(mutation: PhotoMutation) {
  listeners.forEach((listener) => listener(mutation));
}

export function usePhotoMutation(listener: PhotoMutationListener) {
  const listenerRef = useRef(listener);
  listenerRef.current = listener;

  useEffect(() => {
    const current = (mutation: PhotoMutation) => listenerRef.current(mutation);
    listeners.add(current);
    return () => {
      listeners.delete(current);
    };
  }, []);
}

export function useDeferredPhotoMutation(
  active: boolean,
  invalidate: () => void,
  shouldInvalidate?: (mutation: PhotoMutation) => boolean,
) {
  const activeRef = useRef(active);
  const dirtyRef = useRef(false);
  const invalidateRef = useRef(invalidate);
  const shouldInvalidateRef = useRef(shouldInvalidate);
  activeRef.current = active;
  invalidateRef.current = invalidate;
  shouldInvalidateRef.current = shouldInvalidate;

  usePhotoMutation((mutation) => {
    if (shouldInvalidateRef.current && !shouldInvalidateRef.current(mutation)) return;
    if (activeRef.current) {
      invalidateRef.current();
    } else {
      dirtyRef.current = true;
    }
  });

  useEffect(() => {
    if (!active || !dirtyRef.current) return;
    dirtyRef.current = false;
    invalidateRef.current();
  }, [active]);
}
