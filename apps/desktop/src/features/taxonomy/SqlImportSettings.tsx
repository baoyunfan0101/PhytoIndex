import { LoaderCircle, Send, ShieldCheck } from "lucide-react";
import { useEffect, useState } from "react";
import {
  addSqlImportInput,
  applySqlImport,
  getSqlImportSql,
  listSqlImportInputs,
  removeSqlImportInput,
  startSqlImportValidation,
  type SqlImportValidationResult,
  type ValidateSqlImportResult,
} from "../../api/sqlImport";
import type { TaxonomyImportResult } from "../../api/taxonomyImport";
import type { PersistentSqlInput } from "../../api/customSql";
import { errorMessage } from "../../api/common";
import { waitForOperation } from "../../api/tasks";
import { CodeEditor } from "../../shared/CodeEditor";
import { ResizablePanels } from "../../shared/ResizablePanels";
import { Button, Modal, SectionHeader, VirtualList } from "../../shared/ui";
import { SqlInputList } from "./SqlInputList";
import { emitTaxonomyMutation } from "./taxonomyMutations";
import { formatTaxonomyImportApplyMessage } from "./taxonomyImportMessages";
import { resolveSqlWorkbenchLoads } from "./sqlWorkbenchLoading";

export function SqlImportSettings({ onApplied }: { onApplied?: () => void }) {
  const [inputs, setInputs] = useState<PersistentSqlInput[]>([]);
  const [sql, setSql] = useState("");
  const [validation, setValidation] = useState<SqlImportValidationResult | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState("");
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [loadingWorkbench, setLoadingWorkbench] = useState(true);

  useEffect(() => {
    void Promise.allSettled([getSqlImportSql(), listSqlImportInputs()])
      .then(([sqlResult, inputsResult]) => {
        const loaded = resolveSqlWorkbenchLoads(sqlResult, inputsResult);
        if (loaded.sql !== undefined) setSql(loaded.sql);
        if (loaded.inputs !== undefined) setInputs(loaded.inputs);
        setError(loaded.error);
      })
      .finally(() => setLoadingWorkbench(false));
  }, []);

  async function addInput(kind: "csv" | "sqlite", alias: string, path: string) {
    setBusy("Adding data source");
    setMessage("");
    setError("");
    try {
      const result = await addSqlImportInput(kind, alias, path);
      setInputs(result.inputs);
      setValidation(null);
      setMessage(result.warnings.length > 0 ? result.warnings.join(" ") : "Data source added.");
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  async function validate() {
    setBusy("Validating SQL import");
    setMessage("");
    setError("");
    setValidation(null);
    try {
      const started = await startSqlImportValidation(sql);
      const completed = started.task_id
        ? await waitForOperation("sql_import", started.task_id)
        : started;
      if (completed.error) throw new Error(completed.error);
      const result = completed.result as ValidateSqlImportResult | null;
      if (!result) throw new Error("SQL import validation completed without a result");
      setValidation(result.validation);
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
      setError(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  async function apply() {
    setBusy("Applying SQL import");
    setMessage("");
    setError("");
    try {
      const operation = await applySqlImport();
      const completed = await waitForOperation(operation.module, operation.task_id);
      if (completed.error) throw new Error(completed.error);
      const result = completed.result as TaxonomyImportResult | null;
      if (!result) throw new Error("SQL import completed without a replacement result");
      setValidation(null);
      setConfirming(false);
      onApplied?.();
      emitTaxonomyMutation({ kind: "replacement" });
      setMessage(formatTaxonomyImportApplyMessage(result.warnings));
    } catch (nextError) {
      setMessage("");
      setError(errorMessage(nextError));
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
    setError("");
    try {
      const result = await removeSqlImportInput(input.alias);
      setInputs(result.inputs);
      setValidation(null);
      setMessage(result.warnings.length > 0 ? result.warnings.join(" ") : "Data source removed.");
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  const primary = (
    <div className="sql-workbench-primary">
      <ResizablePanels
        className="sql-import-workbench"
        initialSize={250}
        minFirst={180}
        minSecond={320}
        separatorLabel="Resize Input sources"
        stateKey="sql-import.inputs"
        first={<SqlInputList inputs={inputs} busy={Boolean(busy) || loadingWorkbench} operation={busy} onAdd={addInput} onRemove={removeInput} />}
        second={(<div className="sql-import-editor">
          <CodeEditor language="sql" ariaLabel="SQL import SQL" value={sql} onChange={(value) => {
            setSql(value);
            setValidation(null);
          }} />
        </div>)}
      />
      {error ? (
        <div className="inline-error sql-import-status" role="alert">{error}</div>
      ) : loadingWorkbench ? (
        <div className="sql-import-progress" role="status" aria-live="polite">
          <LoaderCircle className="spin" size={15} />
          <strong>Loading taxonomy database workspace...</strong>
        </div>
      ) : message ? (
        <div className="editor-message sql-import-status">{message}</div>
      ) : null}
    </div>
  );

  const output = validation ? (
    <div className="sql-import-validation">
      <div className="sql-import-metadata-grid">
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
            <span>{item.message}</span>
            <code>{issueContext(item)}</code>
          </div>
        )}
      />
    </div>
  ) : null;

  return (
    <div className="sql-import-settings">
      <SectionHeader
        title="SQL Import"
        detail="Build, validate, and apply a replacement taxonomy database."
        actions={(
          <>
            <Button disabled={Boolean(busy) || loadingWorkbench || !sql.trim()} onClick={() => void validate()}>
              <ShieldCheck size={13} />{busy === "Validating SQL import" ? "Validating..." : "Validate"}
            </Button>
            <Button variant="primary" disabled={Boolean(busy) || loadingWorkbench || !validation?.can_apply} onClick={() => setConfirming(true)}>
              <Send size={13} />Apply
            </Button>
          </>
        )}
      />
      {output ? (
        <ResizablePanels
          className="sql-output-split"
          direction="vertical"
          initialRatio={0.55}
          minFirst={250}
          minSecond={150}
          separatorLabel="Resize validation output"
          stateKey="sql-import.output"
          first={primary}
          second={output}
        />
      ) : primary}
      {confirming && (
        <Modal
          title="Apply SQL import"
          onClose={() => !busy && setConfirming(false)}
          actions={(
            <>
              <Button disabled={Boolean(busy)} onClick={() => setConfirming(false)}>Cancel</Button>
              <Button variant="primary" disabled={Boolean(busy)} onClick={() => void apply()}>
                {busy === "Applying SQL import" ? "Applying..." : "Replace taxonomy"}
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

function issueContext(issue: SqlImportValidationResult["errors"][number]): string {
  const context = [];
  if (issue.taxon_id !== null) context.push(`Taxon ${issue.taxon_id}`);
  if (issue.related_taxon_id !== null) context.push(`Related taxon ${issue.related_taxon_id}`);
  if (context.length === 0) context.push(...[issue.table, issue.row_identifier].filter(Boolean));
  return context.join(" / ");
}
