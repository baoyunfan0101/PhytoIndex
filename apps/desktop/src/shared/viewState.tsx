import {
  createContext,
  useCallback,
  useContext,
  useRef,
  useState,
  type Dispatch,
  type ReactNode,
  type SetStateAction,
} from "react";

export type ViewStateStore = Map<string, unknown>;

const ViewStateContext = createContext<ViewStateStore | null>(null);

export function ViewStateProvider({
  children,
  store,
}: {
  children: ReactNode;
  store: ViewStateStore;
}) {
  return <ViewStateContext.Provider value={store}>{children}</ViewStateContext.Provider>;
}

export function useViewState<S>(
  key: string | null,
  initialState: S | (() => S),
): [S, Dispatch<SetStateAction<S>>] {
  const store = useContext(ViewStateContext);
  const [state, setLocalState] = useState<S>(() => {
    if (key !== null && store?.has(key)) return store.get(key) as S;
    const initial = initialState instanceof Function ? initialState() : initialState;
    if (key !== null) store?.set(key, initial);
    return initial;
  });
  const stateRef = useRef(state);
  stateRef.current = state;

  const setState = useCallback<Dispatch<SetStateAction<S>>>((action) => {
    const next = action instanceof Function ? action(stateRef.current) : action;
    stateRef.current = next;
    if (key !== null) store?.set(key, next);
    setLocalState(next);
  }, [key, store]);

  return [state, setState];
}
