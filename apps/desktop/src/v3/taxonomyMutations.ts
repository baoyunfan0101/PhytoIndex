import { useEffect, useRef } from "react";

type TaxonomyMutationListener = () => void;

const listeners = new Set<TaxonomyMutationListener>();

export function emitTaxonomyMutation() {
  listeners.forEach((listener) => listener());
}

export function useTaxonomyMutation(listener: TaxonomyMutationListener) {
  const listenerRef = useRef(listener);
  listenerRef.current = listener;

  useEffect(() => {
    const current = () => listenerRef.current();
    listeners.add(current);
    return () => {
      listeners.delete(current);
    };
  }, []);
}
