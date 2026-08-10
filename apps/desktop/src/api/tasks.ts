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
  operation: string | null;
  running: boolean;
  started_at: string | null;
  finished_at: string | null;
  message: string;
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
    operation: null,
    running: false,
    started_at: null,
    finished_at: null,
    message,
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
    if (!operation || operation.task_id !== taskId || !operation.running) {
      return operation ?? demoOperation(module, "Complete");
    }
    await new Promise((resolve) => window.setTimeout(resolve, 250));
  }
}
