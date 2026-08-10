import type { OperationState } from "../api/tasks";

export function observedSuccessfulCompletion(
  previous: OperationState | undefined,
  current: OperationState | undefined,
): boolean {
  if (!current?.task_id || current.running || current.error) return false;
  return previous?.task_id !== current.task_id || Boolean(previous.running);
}
