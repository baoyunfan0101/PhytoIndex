import { useEffect, useRef, useState } from "react";
import { suggestPhotoTaxa, type TaxonSuggestion } from "../../../api/taxonomy";
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

    setSuggestions([]);
    const timer = window.setTimeout(() => {
      setLoading(true);
      void suggestPhotoTaxa(value)
        .then((next) => {
          if (requestGeneration.current !== generation) return;
          setSuggestions(next);
        })
        .catch(() => {
          if (requestGeneration.current !== generation) return;
          setSuggestions([]);
        })
        .finally(() => {
          if (requestGeneration.current === generation) setLoading(false);
        });
    }, 160);

    return () => window.clearTimeout(timer);
  }, [enabled, query]);

  return { loading, suggestions };
}
