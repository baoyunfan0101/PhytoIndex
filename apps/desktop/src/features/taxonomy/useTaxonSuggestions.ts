import { useEffect, useRef, useState } from "react";
import { suggestTaxa, type TaxonSuggestion } from "../../api/taxonomy";

export function useTaxonSuggestions(query: string, enabled = true) {
  const [suggestions, setSuggestions] = useState<TaxonSuggestion[]>([]);
  const [loading, setLoading] = useState(false);
  const requestGeneration = useRef(0);

  useEffect(() => {
    const value = query.trim();
    const generation = ++requestGeneration.current;
    if (!enabled || !value) {
      setSuggestions([]);
      setLoading(false);
      return;
    }

    setSuggestions([]);
    const timer = window.setTimeout(() => {
      setLoading(true);
      void suggestTaxa(value)
        .then((next) => {
          if (requestGeneration.current === generation) setSuggestions(next);
        })
        .catch(() => {
          if (requestGeneration.current === generation) setSuggestions([]);
        })
        .finally(() => {
          if (requestGeneration.current === generation) setLoading(false);
        });
    }, 160);

    return () => window.clearTimeout(timer);
  }, [enabled, query]);

  return { loading, suggestions };
}
