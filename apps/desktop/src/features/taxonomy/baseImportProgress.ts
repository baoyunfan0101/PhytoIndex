import type { OperationProgress } from "../../api/tasks";

const stageLabels: Record<string, string> = {
  preparing_input_sources: "Preparing input sources",
  executing_sql: "Executing SQL",
  building_staging_database: "Building staging database",
  normalizing_names: "Normalizing names",
  building_candidate_taxa: "Building candidate taxa",
  building_candidate_names: "Building candidate names",
  validating_taxonomy: "Validating taxonomy",
  ready_to_apply: "Ready to apply",
  validation_failed: "Validation failed",
  operational_failure: "Validation could not be completed",
};

export function describeBaseImportProgress(progress: OperationProgress | null): string {
  if (!progress) return "Preparing input sources";
  const label = stageLabels[progress.stage] ?? progress.stage.replace(/_/g, " ");
  if (
    progress.stage === "executing_sql"
    && progress.statement_index !== null
    && progress.statement_total !== null
  ) {
    return `${label} statement ${progress.statement_index} / ${progress.statement_total}`;
  }
  if (progress.current !== null && progress.total !== null) {
    return `${label}: ${progress.current.toLocaleString()} / ${progress.total.toLocaleString()}`;
  }
  return label;
}

export function formatElapsed(milliseconds: number): string {
  const totalSeconds = Math.max(0, Math.floor(milliseconds / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  }
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}
