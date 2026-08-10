import type { OperationState, OperationsStatus } from "../api/tasks";

export function operationByTaskId(
  operations: OperationsStatus,
  taskId: string | null,
): OperationState | undefined {
  if (!taskId) return undefined;
  return operations[taskId]
    ?? Object.values(operations).find((operation) => operation.task_id === taskId);
}

export function latestOperationForModule(
  operations: OperationsStatus,
  module: string,
): OperationState | undefined {
  return Object.values(operations)
    .filter((operation) => operation.module === module)
    .sort((left, right) => (
      right.started_at ?? ""
    ).localeCompare(left.started_at ?? ""))[0];
}
