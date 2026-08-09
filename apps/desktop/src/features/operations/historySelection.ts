import type { OperationSummary } from "../../api/operations";

export function getSelectedOperations(
  operations: OperationSummary[],
  selectedIds: number[],
): OperationSummary[] {
  const selected = new Set(selectedIds);
  return operations.filter((operation) => selected.has(operation.operation_id));
}

export function getRollbackOrder(operations: OperationSummary[]): OperationSummary[] {
  return [...operations].sort((left, right) => right.operation_id - left.operation_id);
}

export function canRollbackOperations(operations: OperationSummary[]): boolean {
  return operations.length > 0 && operations.every((operation) => operation.rollbackable);
}

export function canExportReplayableInput(operations: OperationSummary[]): boolean {
  return operations.length > 0 && operations.every((operation) => operation.has_formatted_input);
}

export function formatAuditJson(value: unknown): string {
  if (value === null || value === undefined) return "-";
  return JSON.stringify(value, null, 2);
}
