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
    .sort((left, right) => {
      const leftActive = left.state === "queued" || left.state === "running";
      const rightActive = right.state === "queued" || right.state === "running";
      if (leftActive !== rightActive) return leftActive ? -1 : 1;
      return (
        right.started_at ?? right.finished_at ?? ""
      ).localeCompare(left.started_at ?? left.finished_at ?? "");
    })[0];
}
