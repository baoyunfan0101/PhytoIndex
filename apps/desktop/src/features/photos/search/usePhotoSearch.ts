import { useCallback, useState } from "react";
import { errorMessage } from "../../../api/common";
import { normalizeSearchQuery } from "./recentSearchStorage";

export type SearchSubmitter = (query: string) => Promise<void>;

export function usePhotoSearch(onSubmit: SearchSubmitter) {
  const [query, setQueryValue] = useState("");
  const [error, setError] = useState<string | null>(null);

  const setQuery = useCallback((value: string) => {
    setQueryValue(value);
    setError(null);
  }, []);

  const submit = useCallback(async (candidate = query) => {
    const normalized = normalizeSearchQuery(candidate);
    if (!normalized) return false;
    try {
      await onSubmit(normalized);
      setError(null);
      return true;
    } catch (nextError) {
      setError(errorMessage(nextError));
      return false;
    }
  }, [onSubmit, query]);

  return { error, query, setQuery, submit };
}

export type PhotoSearchController = ReturnType<typeof usePhotoSearch>;
