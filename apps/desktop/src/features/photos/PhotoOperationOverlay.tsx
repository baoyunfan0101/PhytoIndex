import type { OperationState } from "../../api/tasks";
import { Busy } from "../../shared/ui";
import { describePhotoOperation, photoOperationProgress } from "./photoOperation";

export function PhotoOperationOverlay({ operation }: { operation: OperationState }) {
  const progress = photoOperationProgress(operation);
  return (
    <div className="photo-operation-overlay" role="status" aria-live="polite">
      <Busy label={describePhotoOperation(operation)} />
      {progress && <progress value={progress.value} max={progress.max} />}
    </div>
  );
}
