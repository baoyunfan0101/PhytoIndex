import { useEffect, useRef, useState } from "react";
import { getOperationsStatus, type OperationsStatus } from "./api";
import { emitPhotoMutation } from "./photoMutations";

export function useOperationObserver() {
  const [operations, setOperations] = useState<OperationsStatus>({});
  const previous = useRef<OperationsStatus>({});

  useEffect(() => {
    let active = true;
    let timer = 0;
    const poll = async () => {
      try {
        const next = await getOperationsStatus();
        if (!active) return;
        const priorMapping = previous.current.mapping;
        const nextMapping = next.mapping;
        if (priorMapping?.running && nextMapping && !nextMapping.running && !nextMapping.error) {
          emitPhotoMutation({ photoId: null, kind: "mapping" });
        }
        previous.current = next;
        setOperations(next);
      } finally {
        if (active) timer = window.setTimeout(poll, 500);
      }
    };
    void poll();
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, []);

  return operations;
}
