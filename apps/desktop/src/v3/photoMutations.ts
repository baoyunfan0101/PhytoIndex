import { useEffect, useRef } from "react";
import type { Photo } from "./api";

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
