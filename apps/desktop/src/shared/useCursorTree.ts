import { useCallback, useRef } from "react";
import { errorMessage, type Page } from "../api/common";
import { useViewState } from "./viewState";

export type CursorTreeNode<T> = {
  expanded: boolean;
  items: T[];
  nextCursor: string | null;
  loading: boolean;
  error: string;
};

type CursorTreeOptions<T, K extends string | number> = {
  loadPage: (key: K, cursor: string | null) => Promise<Page<T>>;
  stateKey?: string;
};

const emptyNode = <T,>(expanded = false): CursorTreeNode<T> => ({
  expanded,
  items: [],
  nextCursor: null,
  loading: false,
  error: "",
});

export function useCursorTree<T, K extends string | number>({
  loadPage,
  stateKey,
}: CursorTreeOptions<T, K>) {
  const [nodes, setNodes] = useViewState<Map<K, CursorTreeNode<T>>>(
    stateKey ? `${stateKey}.nodes` : null,
    () => new Map(),
  );
  const nodesRef = useRef(nodes);
  const loadPageRef = useRef(loadPage);
  const requests = useRef(new Map<K, number>());

  nodesRef.current = nodes;
  loadPageRef.current = loadPage;

  const load = useCallback(async (key: K, append: boolean) => {
    const current = nodesRef.current.get(key);
    if (current?.loading || (append && current?.nextCursor === null)) return;
    const cursor = append ? current?.nextCursor ?? null : null;
    const request = (requests.current.get(key) ?? 0) + 1;
    requests.current.set(key, request);
    setNodes((previous) => {
      const next = new Map(previous);
      next.set(key, {
        ...(next.get(key) ?? emptyNode<T>(true)),
        loading: true,
        error: "",
      });
      return next;
    });
    try {
      const page = await loadPageRef.current(key, cursor);
      if (requests.current.get(key) !== request) return;
      setNodes((previous) => {
        const next = new Map(previous);
        const node = next.get(key) ?? emptyNode<T>(true);
        next.set(key, {
          ...node,
          items: append ? [...node.items, ...page.items] : page.items,
          nextCursor: page.next_cursor,
          loading: false,
          error: "",
        });
        return next;
      });
    } catch (nextError) {
      if (requests.current.get(key) !== request) return;
      setNodes((previous) => {
        const next = new Map(previous);
        const node = next.get(key) ?? emptyNode<T>(true);
        next.set(key, { ...node, loading: false, error: errorMessage(nextError) });
        return next;
      });
    }
  }, []);

  const toggle = useCallback((key: K) => {
    const current = nodesRef.current.get(key);
    const expanded = !current?.expanded;
    setNodes((previous) => {
      const next = new Map(previous);
      next.set(key, { ...(next.get(key) ?? emptyNode<T>()), expanded });
      return next;
    });
    if (expanded && (!current || current.items.length === 0)) void load(key, false);
  }, [load]);

  const loadMore = useCallback((key: K) => load(key, true), [load]);

  const expandSubtree = useCallback(async (
    key: K,
    childKeyForItem: (item: T) => K | null | undefined,
  ) => {
    const visited = new Set<K>();
    const expand = async (currentKey: K): Promise<void> => {
      if (visited.has(currentKey)) return;
      visited.add(currentKey);
      const request = (requests.current.get(currentKey) ?? 0) + 1;
      requests.current.set(currentKey, request);
      setNodes((previous) => {
        const next = new Map(previous);
        next.set(currentKey, {
          ...(next.get(currentKey) ?? emptyNode<T>(true)),
          expanded: true,
          loading: true,
          error: "",
        });
        return next;
      });
      try {
        const items: T[] = [];
        let cursor: string | null = null;
        do {
          const page = await loadPageRef.current(currentKey, cursor);
          items.push(...page.items);
          cursor = page.next_cursor;
        } while (cursor !== null);
        if (requests.current.get(currentKey) !== request) return;
        setNodes((previous) => {
          const next = new Map(previous);
          next.set(currentKey, {
            expanded: true,
            items,
            nextCursor: null,
            loading: false,
            error: "",
          });
          return next;
        });
        for (const item of items) {
          const childKey = childKeyForItem(item);
          if (childKey !== null && childKey !== undefined) await expand(childKey);
        }
      } catch (nextError) {
        if (requests.current.get(currentKey) !== request) return;
        setNodes((previous) => {
          const next = new Map(previous);
          const node = next.get(currentKey) ?? emptyNode<T>(true);
          next.set(currentKey, { ...node, expanded: true, loading: false, error: errorMessage(nextError) });
          return next;
        });
      }
    };

    await expand(key);
  }, []);

  const reloadExpanded = useCallback(async () => {
    const keys = [...nodesRef.current.entries()]
      .filter(([, node]) => node.expanded)
      .map(([key]) => key);
    await Promise.all(keys.map((key) => load(key, false)));
  }, [load]);

  const clear = useCallback(() => {
    requests.current.clear();
    nodesRef.current = new Map();
    setNodes(new Map());
  }, []);

  return {
    nodes,
    toggle,
    expandSubtree,
    loadMore,
    reloadExpanded,
    clear,
  };
}
