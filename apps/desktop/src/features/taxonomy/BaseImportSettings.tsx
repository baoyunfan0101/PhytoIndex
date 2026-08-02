import { Send, ShieldCheck } from "lucide-react";
import { useEffect, useState } from "react";
import {
  addBaseImportInput,
  applyBaseImport,
  getBaseImportSql,
  listBaseImportInputs,
  removeBaseImportInput,
  validateBaseImport,
  type BaseImportValidationResult,
  type TaxonomyBaseReplaceResult,
} from "../../api/baseImport";
import type { PersistentSqlInput, SqlStatementMessage } from "../../api/customSql";
import { errorMessage } from "../../api/common";
import { waitForOperation } from "../../api/tasks";
import { CodeEditor } from "../../shared/CodeEditor";
import { Button, Modal, SectionHeader, VirtualList } from "../../shared/ui";
import { SqlInputList } from "./SqlInputList";
import { emitTaxonomyMutation } from "./taxonomyMutations";

export function BaseImportSettings({ onApplied }: { onApplied?: () => void }) {
  const [inputs, setInputs] = useState<PersistentSqlInput[]>([]);
  const [sql, setSql] = useState("");
  const [validation, setValidation] = useState<BaseImportValidationResult | null>(null);
  const [executionMessages, setExecutionMessages] = useState<SqlStatementMessage[]>([]);
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState("");
  const [message, setMessage] = useState("");

  useEffect(() => {
    Promise.all([getBaseImportSql(), listBaseImportInputs()])
      .then(([savedSql, savedInputs]) => {
        setSql(savedSql);
        setInputs(savedInputs);
      })
      .catch((nextError) => setMessage(errorMessage(nextError)));
  }, []);

  async function addInput(kind: "csv" | "sqlite", alias: string, path: string) {
    setBusy("Adding data source");
    setMessage("");
    try {
      const result = await addBaseImportInput(kind, alias, path);
      setInputs(result.inputs);
      setValidation(null);
      setExecutionMessages([]);
      setMessage(result.warnings.length > 0 ? result.warnings.join(" ") : "Data source added.");
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  async function validate() {
    setBusy("Executing SQL and validating candidate");
    setMessage("");
    try {
      const result = await validateBaseImport(sql);
      setValidation(result.validation);
      setExecutionMessages(result.execution.messages);
      const saveStatus = result.execution.script_saved ? "SQL saved." : "SQL was not saved.";
      setMessage([
        `${result.execution.statements_executed} statements executed successfully.`,
        saveStatus,
        result.can_apply
          ? "Candidate is valid and ready to apply."
          : `Validation found ${result.validation.total_error_count} errors.`,
        ...result.warnings,
      ].join(" "));
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
      const result = completed.result as TaxonomyBaseReplaceResult | null;
      if (!result) throw new Error("Base import completed without a replacement result");
      setValidation(null);
      setConfirming(false);
      onApplied?.();
      emitTaxonomyMutation({ kind: "replacement" });
      setMessage([
        "Taxonomy database replaced successfully. Photo mappings are being rebuilt in the background.",
        ...result.warnings,
      ].join(" "));
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
        title="Taxonomy Databases"
        detail="Build, validate, and apply a replacement taxonomy database."
        actions={(
          <>
            <Button disabled={Boolean(busy) || !sql.trim()} onClick={() => void validate()}>
              <ShieldCheck size={13} />Validate
            </Button>
            <Button variant="primary" disabled={Boolean(busy) || !validation?.can_apply} onClick={() => setConfirming(true)}>
              <Send size={13} />Apply
            </Button>
          </>
        )}
      />
      <div className="base-import-workbench">
        <SqlInputList inputs={inputs} busy={Boolean(busy)} onAdd={addInput} onRemove={removeInput} />
        <div className="base-import-editor">
          <CodeEditor language="sql" ariaLabel="Base import SQL" value={sql} onChange={(value) => {
            setSql(value);
            setValidation(null);
            setExecutionMessages([]);
          }} />
        </div>
      </div>
      <div className="base-import-feedback">
        {(message || busy) && <div className="editor-message">{busy || message}</div>}
        {executionMessages.map((item) => (
          <p className="sql-message" key={`${item.statement_index}:${item.message}`}>
            Statement {item.statement_index}: {item.message}
            {item.affected_rows !== null ? ` (${item.affected_rows} rows)` : ""}
          </p>
        ))}
      </div>
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
              <Button disabled={Boolean(busy)} onClick={() => setConfirming(false)}>Cancel</Button>
              <Button variant="primary" disabled={Boolean(busy)} onClick={() => void apply()}>
                Replace taxonomy
              </Button>
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
