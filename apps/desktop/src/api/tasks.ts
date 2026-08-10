import { call } from "./client";
import { operationByTaskId } from "../app/operationRegistry";

export type OperationProgress = {
  stage: string;
  current: number | null;
  total: number | null;
  statement_index: number | null;
  statement_total: number | null;
};

export type OperationState = {
  module: string;
  task_id: string | null;
  task_kind: string | null;
  task_scope: string | null;
  state: "queued" | "running" | "completed" | "failed";
  operation: string | null;
  running: boolean;
  started_at: string | null;
  finished_at: string | null;
  message: string;
  completed: number;
  processed: number;
  total: number | null;
  progress: OperationProgress | null;
  result: unknown;
  error: string | null;
};

export type OperationsStatus = Record<string, OperationState>;

export function demoOperation(module: string, message: string): OperationState {
  return {
    module,
    task_id: null,
    task_kind: null,
    task_scope: null,
    state: "completed",
    operation: null,
    running: false,
    started_at: null,
    finished_at: null,
    message,
    completed: 0,
    processed: 0,
    total: null,
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
