import { useEffect, useRef, useState } from "react";
import { errorMessage } from "../../api/common";
import { searchTaxa, type TaxonSearchResult } from "../../api/taxonomy";
import { useViewState } from "../../shared/viewState";

export function useTaxonSearch(
  query: string,
  {
    enabled = true,
    limit = 80,
    debounceMs = 180,
    stateKey,
    refreshKey = 0,
  }: {
    enabled?: boolean;
    limit?: number;
    debounceMs?: number;
    stateKey?: string;
    refreshKey?: number;
  } = {},
) {
  const [results, setResults] = useViewState<TaxonSearchResult[]>(
    stateKey ? `${stateKey}.results` : null,
    [],
  );
  const [lastQuery, setLastQuery] = useViewState(
    stateKey ? `${stateKey}.query` : null,
    "",
  );
  const [loading, setLoading] = useState(false);
  const [error, setError] = useViewState(stateKey ? `${stateKey}.error` : null, "");
  const requestRef = useRef(0);

  useEffect(() => {
    const value = query.trim();
    const request = ++requestRef.current;
    setError("");
    if (!enabled || !value) {
      setResults([]);
      setLastQuery("");
      setLoading(false);
      return;
    }
    if (lastQuery !== value) setResults([]);
    setLastQuery(value);
    setLoading(true);
    const timer = window.setTimeout(() => {
      searchTaxa(value, limit)
        .then((next) => {
          if (request === requestRef.current) setResults(next);
        })
        .catch((nextError) => {
          if (request === requestRef.current) setError(errorMessage(nextError));
        })
        .finally(() => {
          if (request === requestRef.current) setLoading(false);
        });
    }, debounceMs);
    return () => window.clearTimeout(timer);
  }, [debounceMs, enabled, limit, query, refreshKey]);

  return { results, loading, error };
}
