import { CircleQuestionMark, Download, Play } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import {
  addCustomSqlInput,
  executeCustomSql,
  exportCustomSqlQuery,
  getCustomTaxonomySql,
  listCustomSqlDatabaseSchemas,
  listCustomSqlInputs,
  removeCustomSqlInput,
  type CustomSqlExecutionResult,
  type PersistentSqlInput,
  type SqlResultSet,
  type SqlSourceSchema,
  type SqlValue,
} from "../../api/customSql";
import { errorMessage } from "../../api/common";
import { selectCsvDestination } from "../../api/dialogs";
import { CodeEditor } from "../../shared/CodeEditor";
import { ResizablePanels } from "../../shared/ResizablePanels";
import { Busy, Button, EmptyState, SectionHeader, VirtualList } from "../../shared/ui";
import { SqlInputList } from "./SqlInputList";
import { SqlEnumHelpModal } from "./TaxonomyHelpModal";
import { canExportFullQuery } from "./sqlResults";
import { resolveSqlWorkbenchLoads } from "./sqlWorkbenchLoading";
import { emitTaxonomyMutation } from "./taxonomyMutations";

export function CustomSqlView({
  onStatus,
  taskOwnerId,
  mutationDisabled = false,
}: {
  onStatus: (message: string) => void;
  taskOwnerId: string;
  mutationDisabled?: boolean;
}) {
  const [sql, setSql] = useState("");
  const [inputs, setInputs] = useState<PersistentSqlInput[]>([]);
  const [databaseSchemas, setDatabaseSchemas] = useState<SqlSourceSchema[]>([]);
  const [result, setResult] = useState<CustomSqlExecutionResult | null>(null);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [loadingWorkbench, setLoadingWorkbench] = useState(true);
  const [helpOpen, setHelpOpen] = useState(false);

  useEffect(() => {
    void Promise.allSettled([
      getCustomTaxonomySql(),
      listCustomSqlInputs(),
      listCustomSqlDatabaseSchemas(),
    ])
      .then(([sqlResult, inputsResult, schemasResult]) => {
        const loaded = resolveSqlWorkbenchLoads(sqlResult, inputsResult);
        if (loaded.sql !== undefined) setSql(loaded.sql);
        if (loaded.inputs !== undefined) setInputs(loaded.inputs);
        if (schemasResult.status === "fulfilled") setDatabaseSchemas(schemasResult.value);
        const schemasError = schemasResult.status === "rejected" ? errorMessage(schemasResult.reason) : "";
        setError([loaded.error, schemasError].filter(Boolean).join(" "));
      })
      .finally(() => setLoadingWorkbench(false));
  }, []);

  async function addInput(kind: "csv" | "sqlite", alias: string, path: string) {
    setBusy("Adding data source");
    setError("");
    try {
      const result = await addCustomSqlInput(kind, alias, path);
      setInputs(result.inputs);
      if (result.warnings.length > 0) onStatus(result.warnings.join(" "));
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  async function removeInput(input: PersistentSqlInput) {
    setBusy(`Removing ${input.alias}`);
    setError("");
    try {
      const result = await removeCustomSqlInput(input.alias);
      setInputs(result.inputs);
      if (result.warnings.length > 0) onStatus(result.warnings.join(" "));
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  async function execute() {
    setBusy("Executing SQL");
    setError("");
    try {
      const next = await executeCustomSql(sql, taskOwnerId);
      setResult(next);
      const outcome = next.operation_id === null
        ? "Query completed without creating an operation"
        : `Mutation created taxonomy operation ${next.operation_id}`;
      if (next.operation_id !== null) emitTaxonomyMutation();
      const saveStatus = next.script_saved ? "SQL saved." : "SQL was not saved.";
      onStatus([outcome, saveStatus, ...next.warnings].join(" "));
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  async function exportQuery() {
    const destination = await selectCsvDestination("taxonomy-query.csv");
    if (!destination) return;
    setBusy("Exporting full query");
    setError("");
    try {
      const exported = await exportCustomSqlQuery(sql, destination, taskOwnerId);
      onStatus(`Exported ${exported.row_count} rows to ${exported.path}`);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  const workbench = (
    <ResizablePanels
      className="custom-sql-workbench"
      initialSize={250}
      minFirst={180}
      minSecond={320}
      separatorLabel="Resize Input sources"
      stateKey="custom-sql.inputs"
      first={<SqlInputList inputs={inputs} workspaceSchemas={[]} databaseSchemas={databaseSchemas} busy={Boolean(busy) || loadingWorkbench} operation={busy} onAdd={addInput} onRemove={removeInput} />}
      second={(<div className="custom-sql-editor">
        <CodeEditor language="sql" ariaLabel="Custom taxonomy SQL" value={sql} onChange={setSql} />
      </div>)}
    />
  );

  const primary = (
    <div className="sql-workbench-primary">
      {workbench}
      {error ? (
        <div className="inline-error" role="alert">{error}</div>
      ) : loadingWorkbench ? (
        <div className="editor-message" role="status" aria-live="polite">
          <Busy label="Loading custom SQL workspace..." />
        </div>
      ) : null}
    </div>
  );

  const output = result ? (
    <div className="sql-results">
      <div className="sql-result-summary">
        <span>{result.changeset_size} bytes changed</span>
        <span>{result.operation_id === null ? "No operation created" : `Operation ${result.operation_id}`}</span>
        <span>{result.script_saved ? "Script saved" : "Script not saved"}</span>
        {canExportFullQuery(result) && (
          <Button disabled={Boolean(busy)} onClick={() => void exportQuery()}>
            <Download size={13} />{busy === "Exporting full query" ? "Exporting..." : "Export full query"}
          </Button>
        )}
      </div>
      {result.messages.map((message) => (
        <p className="sql-message" key={`${message.statement_index}:${message.message}`}>
          Statement {message.statement_index}: {message.message}
          {message.affected_rows !== null ? ` (${message.affected_rows} rows)` : ""}
        </p>
      ))}
      {result.result_sets.length === 0 ? (
        <EmptyState title="No result sets" detail="Mutation messages are shown above." />
      ) : result.result_sets.map((set) => <SqlResultTable key={set.statement_index} result={set} />)}
    </div>
  ) : null;

  return (
    <div className="custom-sql-view">
      <SectionHeader
        title="Custom SQL"
        detail="Execute typed SQL against taxonomy and file-path data sources"
        actions={(
          <>
            <Button onClick={() => setHelpOpen(true)}><CircleQuestionMark size={13} />Help</Button>
            <Button variant="primary" disabled={Boolean(busy) || loadingWorkbench || !sql.trim() || mutationDisabled} onClick={() => void execute()}>
              <Play size={13} />{busy === "Executing SQL" ? "Running..." : "Run"}
            </Button>
          </>
        )}
      />
      {output ? (
        <ResizablePanels
          className="sql-output-split"
          direction="vertical"
          initialRatio={0.55}
          minFirst={230}
          minSecond={130}
          separatorLabel="Resize SQL output"
          stateKey="custom-sql.output"
          first={primary}
          second={output}
        />
      ) : primary}
      {helpOpen && <SqlEnumHelpModal onClose={() => setHelpOpen(false)} />}
    </div>
  );
}

function SqlResultTable({ result }: { result: SqlResultSet }) {
  const template = useMemo(
    () => `repeat(${Math.max(result.columns.length, 1)}, minmax(130px, 1fr))`,
    [result.columns.length],
  );
  return (
    <section className="sql-result-set">
      <header>
        <strong>Statement {result.statement_index}</strong>
        <span>{result.rows.length} rows{result.truncated ? " (preview truncated)" : ""}</span>
      </header>
      <div className="sql-result-header" style={{ gridTemplateColumns: template }}>
        {result.columns.map((column) => (
          <span key={column.name}>{column.name}<i>{column.declared_type ?? ""}</i></span>
        ))}
      </div>
      <VirtualList
        className="sql-result-rows"
        items={result.rows}
        rowHeight={32}
        itemKey={(_, index) => index}
        renderItem={(row) => (
          <div className="sql-result-row" style={{ gridTemplateColumns: template }}>
            {row.map((value, index) => <code key={index}>{displaySqlValue(value)}</code>)}
          </div>
        )}
      />
    </section>
  );
}

function displaySqlValue(value: SqlValue): string {
  if (value.type === "null") return "NULL";
  if (value.type === "blob") return `[blob/base64] ${value.value}`;
  return String(value.value);
}
