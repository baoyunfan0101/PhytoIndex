import { DatabaseArrowUp, LoaderCircle } from "lucide-react";
import { useState } from "react";
import {
  replaceTaxonomyBaseDatabase,
  type TaxonomyBaseReplaceResult,
} from "../../api/baseImport";
import { errorMessage } from "../../api/common";
import { selectSqliteDatabase } from "../../api/dialogs";
import { getDatabaseLocations } from "../../api/storage";
import { waitForOperation, type OperationState } from "../../api/tasks";
import { Button, SectionHeader } from "../../shared/ui";
import { formatBaseImportApplyMessage } from "./baseImportMessages";
import { emitTaxonomyMutation } from "./taxonomyMutations";

export function DirectImportSettings({ onApplied }: { onApplied?: () => void }) {
  const [operation, setOperation] = useState<OperationState | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");

  async function importDatabase() {
    setMessage("");
    setError("");
    try {
      const locations = await getDatabaseLocations();
      const sourcePath = await selectSqliteDatabase(locations.default_taxonomy_directory);
      if (!sourcePath) return;

      setBusy(true);
      const started = await replaceTaxonomyBaseDatabase(sourcePath);
      setOperation(started);
      const completed = started.task_id
        ? await waitForOperation(started.module, started.task_id, setOperation)
        : started;
      if (completed.error) throw new Error(completed.error);
      const result = completed.result as TaxonomyBaseReplaceResult | null;
      if (!result) throw new Error("Direct import completed without a replacement result");

      onApplied?.();
      emitTaxonomyMutation({ kind: "replacement" });
      setMessage(formatBaseImportApplyMessage(result.warnings));
    } catch (nextError) {
      setMessage("");
      setError(errorMessage(nextError));
    } finally {
      setBusy(false);
      setOperation(null);
    }
  }

  const progressMessage = operation?.progress?.stage
    ?? operation?.message
    ?? "Importing taxonomy database";

  return (
    <div className="settings-section direct-import-settings">
      <SectionHeader
        title="Direct Import"
        detail="Replace the current taxonomy with a ready-to-use SQLite database."
        actions={(
          <Button variant="primary" disabled={busy} onClick={() => void importDatabase()}>
            <DatabaseArrowUp size={13} />{busy ? "Importing..." : "Import"}
          </Button>
        )}
      />
      <div className="direct-import-description">
        <strong>Import a taxonomy database directly</strong>
        <p>The selected SQLite database must contain valid taxa and taxon_names tables using the current schema.</p>
        <p>A successful import replaces the current taxonomy, clears taxonomy history, and schedules every registered photo library for remapping.</p>
      </div>
      {busy ? (
        <div className="base-import-progress" role="status" aria-live="polite">
          <LoaderCircle className="spin" size={15} />
          <strong>{progressMessage}</strong>
        </div>
      ) : error ? (
        <div className="inline-error base-import-status" role="alert">{error}</div>
      ) : message ? (
        <div className="editor-message base-import-status">{message}</div>
      ) : null}
    </div>
  );
}
