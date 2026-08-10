import { useEffect, useRef, useState } from "react";
import { getOperationsStatus, type OperationsStatus } from "../api/tasks";
import { emitPhotoMutation } from "../features/photos/photoMutations";
import { observedSuccessfulCompletion } from "./operationTransitions";

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
        for (const operation of Object.values(next)) {
          const prior = operation.task_id ? previous.current[operation.task_id] : undefined;
          const stage = operation.progress?.stage ?? operation.message;
          const progressed = operation.running
            && operation.processed !== prior?.processed;
          if (operation.module === "photos" && progressed) {
            emitPhotoMutation({
              photoId: null,
              kind: stage.toLowerCase().includes("metadata") ? "metadata" : "index",
            });
          }
          if (operation.module === "mapping" && observedSuccessfulCompletion(prior, operation)) {
            emitPhotoMutation({ photoId: null, kind: "mapping" });
          }
          if (operation.module === "photos" && observedSuccessfulCompletion(prior, operation)) {
            emitPhotoMutation({ photoId: null, kind: "photo" });
          }
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
