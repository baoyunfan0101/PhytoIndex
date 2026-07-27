import { useCallback, useEffect, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { errorMessage, type Page } from "./api";
import { useViewState } from "./viewState";

type CursorPageOptions<T, P> = {
  params: P;
  resetKey: string | number | boolean | null;
  loadPage: (params: P, cursor: string | null) => Promise<Page<T>>;
  enabled?: boolean;
  debounceMs?: number;
  onPageLoaded?: (page: Page<T>, append: boolean) => void;
  stateKey?: string;
};

export type CursorPageController<T> = {
  items: T[];
  nextCursor: string | null;
  hasMore: boolean;
  loading: boolean;
  error: string;
  loadMore: () => Promise<void>;
  reload: () => Promise<void>;
  updateItems: Dispatch<SetStateAction<T[]>>;
};

export function useCursorPage<T, P>({
  params,
  resetKey,
  loadPage,
  enabled = true,
  debounceMs = 0,
  onPageLoaded,
  stateKey,
}: CursorPageOptions<T, P>): CursorPageController<T> {
  const [items, setItems] = useViewState<T[]>(stateKey ? `${stateKey}.items` : null, []);
  const [nextCursor, setNextCursor] = useViewState<string | null>(stateKey ? `${stateKey}.cursor` : null, null);
  const [storedResetKey, setStoredResetKey] = useViewState<typeof resetKey>(
    stateKey ? `${stateKey}.reset-key` : null,
    resetKey,
  );
  const [loading, setLoading] = useState(false);
  const [error, setError] = useViewState(stateKey ? `${stateKey}.error` : null, "");
  const paramsRef = useRef(params);
  const loadPageRef = useRef(loadPage);
  const onPageLoadedRef = useRef(onPageLoaded);
  const cursorRef = useRef<string | null>(nextCursor);
  const loadingRef = useRef(false);
  const requestRef = useRef(0);

  paramsRef.current = params;
  loadPageRef.current = loadPage;
  onPageLoadedRef.current = onPageLoaded;

  const load = useCallback(async (append: boolean) => {
    if (!enabled || loadingRef.current || (append && cursorRef.current === null)) return;
    const request = ++requestRef.current;
    const cursor = append ? cursorRef.current : null;
    loadingRef.current = true;
    setLoading(true);
    setError("");
    try {
      const page = await loadPageRef.current(paramsRef.current, cursor);
      if (request !== requestRef.current) return;
      setItems((current) => append ? [...current, ...page.items] : page.items);
      cursorRef.current = page.next_cursor;
      setNextCursor(page.next_cursor);
      onPageLoadedRef.current?.(page, append);
    } catch (nextError) {
      if (request === requestRef.current) setError(errorMessage(nextError));
    } finally {
      if (request === requestRef.current) {
        loadingRef.current = false;
        setLoading(false);
      }
    }
  }, [enabled]);

  const reload = useCallback(async () => {
    requestRef.current += 1;
    loadingRef.current = false;
    cursorRef.current = null;
    setNextCursor(null);
    await load(false);
  }, [load]);

  const loadMore = useCallback(() => load(true), [load]);

  useEffect(() => {
    const reset = !Object.is(storedResetKey, resetKey);
    requestRef.current += 1;
    loadingRef.current = false;
    cursorRef.current = reset ? null : nextCursor;
    if (reset) {
      setItems([]);
      setNextCursor(null);
      setStoredResetKey(resetKey);
    }
    setError("");
    setLoading(false);
    if (!enabled) return;
    const timer = window.setTimeout(() => void load(false), debounceMs);
    return () => {
      window.clearTimeout(timer);
      requestRef.current += 1;
      loadingRef.current = false;
    };
  }, [debounceMs, enabled, load, resetKey]);

  return {
    items,
    nextCursor,
    hasMore: nextCursor !== null,
    loading,
    error,
    loadMore,
    reload,
    updateItems: setItems,
  };
}
