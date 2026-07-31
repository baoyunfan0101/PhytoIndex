import { Beaker, Play } from "lucide-react";
import { useEffect, useState } from "react";
import {
  addBaseImportInput,
  applyBaseImport,
  errorMessage,
  executeBaseImportSql,
  getBaseImportSql,
  getTaxonomyBaseMetadata,
  listBaseImportInputs,
  removeBaseImportInput,
  validateBaseImport,
  waitForOperation,
  type BaseImportValidationResult,
  type PersistentSqlInput,
  type SqlStatementMessage,
  type TaxonomyBaseMetadata,
} from "./api";
import { CodeEditor } from "./CodeEditor";
import { Modal, SectionHeader, VirtualList } from "./components";
import { SqlInputList } from "./SqlInputList";
import { emitTaxonomyMutation } from "./taxonomyMutations";

export function BaseImportSettings({ onApplied }: { onApplied?: () => void }) {
  const [metadata, setMetadata] = useState<TaxonomyBaseMetadata | null>(null);
  const [inputs, setInputs] = useState<PersistentSqlInput[]>([]);
  const [sql, setSql] = useState("");
  const [validation, setValidation] = useState<BaseImportValidationResult | null>(null);
  const [executionMessages, setExecutionMessages] = useState<SqlStatementMessage[]>([]);
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState("");
  const [message, setMessage] = useState("");

  useEffect(() => {
    Promise.all([getTaxonomyBaseMetadata(), getBaseImportSql(), listBaseImportInputs()])
      .then(([nextMetadata, savedSql, savedInputs]) => {
        setMetadata(nextMetadata);
        setSql(savedSql);
        setInputs(savedInputs);
      })
      .catch((nextError) => setMessage(errorMessage(nextError)));
  }, []);

  async function addInput(kind: "csv" | "sqlite", alias: string, path: string) {
    setBusy("Adding data source");
    setMessage("");
    setValidation(null);
    try {
      const input = await addBaseImportInput(kind, alias, path);
      setInputs((current) => [...current, input].sort((left, right) => left.alias.localeCompare(right.alias)));
      setExecutionMessages([]);
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  async function execute() {
    setBusy("Executing import SQL");
    setMessage("");
    setValidation(null);
    setExecutionMessages([]);
    try {
      const result = await executeBaseImportSql(sql);
      setExecutionMessages(result.messages);
      setMessage(`${result.statements_executed} statements executed successfully. SQL saved.`);
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  async function validate() {
    setBusy("Validating candidate");
    setMessage("");
    try {
      const next = await validateBaseImport();
      setValidation(next);
      setMessage(next.can_apply
        ? "Candidate is valid and ready to apply."
        : `Validation found ${next.total_error_count} errors.`);
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  async function apply() {
    setBusy("Applying base import");
    setMessage("Replacing taxonomy database");
    try {
      const operation = await applyBaseImport();
      const completed = await waitForOperation("mapping", operation.task_id, (next) => setMessage(next.message));
      if (completed.error) throw new Error(completed.error);
      setMetadata(await getTaxonomyBaseMetadata());
      setValidation(null);
      setConfirming(false);
      onApplied?.();
      emitTaxonomyMutation({ kind: "replacement" });
      setMessage("Taxonomy database replaced successfully. Photo mappings are being rebuilt in the background.");
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  async function removeInput(input: PersistentSqlInput) {
    if (
      validation
      && !window.confirm("Removing this source will invalidate the current validation result.")
    ) {
      return;
    }
    setBusy(`Removing ${input.alias}`);
    setMessage("");
    try {
      const result = await removeBaseImportInput(input.alias);
      setInputs(result.inputs);
      setValidation(null);
      setExecutionMessages([]);
      setMessage(result.warnings.length > 0 ? result.warnings.join(" ") : "Data source removed.");
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  return (
    <div className="base-import-settings">
      <SectionHeader
        title="Base Import"
        detail="Build and validate a taxonomy candidate from persistent data sources"
      />
      <div className="base-metadata-grid">
        <Metric label="Current source" value={metadata?.source_path ?? "Not imported"} />
        <Metric label="Taxa" value={metadata ? String(metadata.taxa_count) : "-"} />
        <Metric label="Names" value={metadata ? String(metadata.taxon_names_count) : "-"} />
        <Metric label="Imported" value={metadata?.imported_at ?? "-"} />
      </div>
      <div className="base-import-workbench">
        <SqlInputList inputs={inputs} busy={Boolean(busy)} onAdd={addInput} onRemove={removeInput} />
        <div className="base-import-editor">
          <CodeEditor language="sql" ariaLabel="Base import SQL" value={sql} onChange={setSql} />
          <div className="base-import-actions">
            <button className="secondary-button" type="button" disabled={Boolean(busy) || !sql.trim()} onClick={() => void execute()}>
              <Play size={13} />Execute
            </button>
            <button className="secondary-button" type="button" disabled={Boolean(busy)} onClick={() => void validate()}>
              <Beaker size={13} />Validate
            </button>
            <button className="primary-button" type="button" disabled={Boolean(busy) || !validation?.can_apply} onClick={() => setConfirming(true)}>
              Apply candidate
            </button>
          </div>
        </div>
      </div>
      {(message || busy) && <div className="editor-message">{busy || message}</div>}
      {executionMessages.map((item) => (
        <p className="sql-message" key={`${item.statement_index}:${item.message}`}>
          Statement {item.statement_index}: {item.message}
          {item.affected_rows !== null ? ` (${item.affected_rows} rows)` : ""}
        </p>
      ))}
      {validation && (
        <div className="base-validation">
          <div className="base-metadata-grid">
            <Metric label="Candidate taxa" value={String(validation.taxa_count)} />
            <Metric label="Normalization changes" value={String(validation.normalization_changes)} />
            <Metric label="Warnings" value={String(validation.total_warning_count)} />
            <Metric label="Errors" value={String(validation.total_error_count)} />
          </div>
          <div className="validation-counts">
            {validation.name_counts.map((item) => <span key={item.name_type}>{item.name_type}: {item.count}</span>)}
          </div>
          <VirtualList
            className="validation-issues"
            items={[...validation.errors, ...validation.warnings]}
            rowHeight={58}
            itemKey={(item, index) => `${item.code}:${item.row_identifier}:${index}`}
            renderItem={(item) => (
              <div className="validation-issue">
                <strong>{item.code}</strong>
                <span>{item.message}</span>
                <code>{[item.table, item.row_identifier].filter(Boolean).join(" / ")}</code>
              </div>
            )}
          />
        </div>
      )}
      {confirming && (
        <Modal
          title="Apply base import"
          onClose={() => !busy && setConfirming(false)}
          actions={(
            <>
              <button className="secondary-button" type="button" disabled={Boolean(busy)} onClick={() => setConfirming(false)}>Cancel</button>
              <button className="primary-button" type="button" disabled={Boolean(busy)} onClick={() => void apply()}>
                Replace taxonomy
              </button>
            </>
          )}
        >
          <p>This replaces the taxonomy database, clears taxonomy history, and schedules every registered photo library for remapping.</p>
          <p>The validated candidate has {validation?.taxa_count ?? 0} taxa.</p>
        </Modal>
      )}
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return <div><span>{label}</span><strong title={value}>{value}</strong></div>;
}
