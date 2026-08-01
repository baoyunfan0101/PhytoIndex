import { useEffect, useRef } from "react";

export type TaxonomyMutation = {
  kind: "update" | "replacement";
};

type TaxonomyMutationListener = (mutation: TaxonomyMutation) => void;

const listeners = new Set<TaxonomyMutationListener>();

export function emitTaxonomyMutation(mutation: TaxonomyMutation = { kind: "update" }) {
  listeners.forEach((listener) => listener(mutation));
}

export function useTaxonomyMutation(listener: TaxonomyMutationListener) {
  const listenerRef = useRef(listener);
  listenerRef.current = listener;

  useEffect(() => {
    const current = (mutation: TaxonomyMutation) => listenerRef.current(mutation);
    listeners.add(current);
    return () => {
      listeners.delete(current);
    };
  }, []);
}
