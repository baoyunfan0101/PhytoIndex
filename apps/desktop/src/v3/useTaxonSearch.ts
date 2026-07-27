import { useEffect, useRef, useState } from "react";
import { errorMessage, searchTaxa, type TaxonSearchResult } from "./api";

export function useTaxonSearch(
  query: string,
  {
    enabled = true,
    limit = 80,
    debounceMs = 180,
  }: {
    enabled?: boolean;
    limit?: number;
    debounceMs?: number;
  } = {},
) {
  const [results, setResults] = useState<TaxonSearchResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const requestRef = useRef(0);

  useEffect(() => {
    const value = query.trim();
    const request = ++requestRef.current;
    setError("");
    if (!enabled || !value) {
      setResults([]);
      setLoading(false);
      return;
    }
    setResults([]);
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
  }, [debounceMs, enabled, limit, query]);

  return { results, loading, error };
}
