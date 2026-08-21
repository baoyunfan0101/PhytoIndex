import { call } from "./client";
import { operationByTaskId } from "../app/operationRegistry";

export type OperationProgress = {
  stage: string;
  current: number | null;
  total: number | null;
  unit: "items" | "files" | "photos" | "names" | "taxa" | "bytes" | "statements" | null;
};

export type OperationState = {
  module: string;
  task_id: string | null;
  task_kind: string | null;
  task_scope: string | null;
  state: "queued" | "running" | "completed" | "failed";
  operation: string | null;
  started_at: string | null;
  finished_at: string | null;
  progress: OperationProgress | null;
  result: unknown;
  error: string | null;
};

export type OperationsStatus = Record<string, OperationState>;

export function demoOperation(module: string, _message: string): OperationState {
  return {
    module,
    task_id: null,
    task_kind: null,
    task_scope: null,
    state: "completed",
    operation: null,
    started_at: null,
    finished_at: null,
    progress: null,
    result: null,
    error: null,
  };
}

export const getOperationsStatus = () =>
  call<OperationsStatus>("get_operations_status", undefined, () => ({}));

export async function waitForOperation(
  module: string,
  taskId: string | null,
  onChange?: (operation: OperationState) => void,
): Promise<OperationState> {
  if (!taskId) return demoOperation(module, "Complete");
  while (true) {
    const operation = operationByTaskId(await getOperationsStatus(), taskId);
    if (operation) onChange?.(operation);
    if (!operation || operation.task_id !== taskId || !["queued", "running"].includes(operation.state)) {
      return operation ?? demoOperation(module, "Complete");
    }
    await new Promise((resolve) => window.setTimeout(resolve, 250));
  }
}
