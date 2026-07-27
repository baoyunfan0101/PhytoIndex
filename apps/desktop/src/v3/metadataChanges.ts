import { useEffect, useRef } from "react";

export type MetadataChange =
  | { key: "taxonomy_name_separator"; value: string };

type MetadataChangeListener = (change: MetadataChange) => void;

const listeners = new Set<MetadataChangeListener>();

export function emitMetadataChange(change: MetadataChange) {
  listeners.forEach((listener) => listener(change));
}

export function useMetadataChange(listener: MetadataChangeListener) {
  const listenerRef = useRef(listener);
  listenerRef.current = listener;

  useEffect(() => {
    const current = (change: MetadataChange) => listenerRef.current(change);
    listeners.add(current);
    return () => {
      listeners.delete(current);
    };
  }, []);
}
