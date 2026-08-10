import type { OperationState } from "../api/tasks";

export function observedSuccessfulCompletion(
  previous: OperationState | undefined,
  current: OperationState | undefined,
): boolean {
  if (!current?.task_id || current.state !== "completed" || current.error) return false;
  return previous?.task_id !== current.task_id || previous.state !== "completed";
}
