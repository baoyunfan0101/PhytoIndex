import { startTransition, useEffect, useRef, useState } from "react";
import { suggestPhotoTaxa, type TaxonSuggestion } from "../../../api/taxonomy";
import { SUGGESTION_DEBOUNCE_MS } from "../../../shared/suggestionNavigation";
import { normalizeSearchQuery } from "./recentSearchStorage";

export function useSearchSuggestions(query: string, enabled = true) {
  const [suggestions, setSuggestions] = useState<TaxonSuggestion[]>([]);
  const [loading, setLoading] = useState(false);
  const requestGeneration = useRef(0);

  useEffect(() => {
    const value = normalizeSearchQuery(query);
    const generation = ++requestGeneration.current;
    if (!enabled || !value) {
      setSuggestions([]);
      setLoading(false);
      return;
    }

    startTransition(() => {
      if (requestGeneration.current === generation) setSuggestions([]);
    });
    const timer = window.setTimeout(() => {
      setLoading(true);
      void suggestPhotoTaxa(value)
        .then((next) => {
          if (requestGeneration.current !== generation) return;
          startTransition(() => {
            if (requestGeneration.current === generation) setSuggestions(next);
          });
        })
        .catch(() => {
          if (requestGeneration.current !== generation) return;
          startTransition(() => {
            if (requestGeneration.current === generation) setSuggestions([]);
          });
        })
        .finally(() => {
          if (requestGeneration.current === generation) setLoading(false);
        });
    }, SUGGESTION_DEBOUNCE_MS);

    return () => window.clearTimeout(timer);
  }, [enabled, query]);

  return { loading, suggestions };
}
