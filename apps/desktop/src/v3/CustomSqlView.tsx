import { Download, Play } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import {
  addCustomSqlInput,
  errorMessage,
  executeCustomSql,
  exportCustomSqlQuery,
  getCustomTaxonomySql,
  listCustomSqlInputs,
  removeCustomSqlInput,
  selectCsvDestination,
  type CustomSqlExecutionResult,
  type PersistentSqlInput,
  type SqlResultSet,
  type SqlValue,
} from "./api";
import { CodeEditor } from "./CodeEditor";
import { EmptyState, SectionHeader, VirtualList } from "./components";
import { SqlInputList } from "./SqlInputList";
import { emitTaxonomyMutation } from "./taxonomyMutations";

export function CustomSqlView({
  onStatus,
  mutationDisabled = false,
}: {
  onStatus: (message: string) => void;
  mutationDisabled?: boolean;
}) {
  const [sql, setSql] = useState("");
  const [inputs, setInputs] = useState<PersistentSqlInput[]>([]);
  const [result, setResult] = useState<CustomSqlExecutionResult | null>(null);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    Promise.all([getCustomTaxonomySql(), listCustomSqlInputs()])
      .then(([savedSql, savedInputs]) => {
        setSql(savedSql);
        setInputs(savedInputs);
      })
      .catch((nextError) => setError(errorMessage(nextError)));
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
      const next = await executeCustomSql(sql);
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
      const exported = await exportCustomSqlQuery(sql, destination);
      onStatus(`Exported ${exported.row_count} rows to ${exported.path}`);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  return (
    <div className="custom-sql-view">
      <SectionHeader
        title="Custom SQL"
        detail="Execute typed SQL against taxonomy and file-path data sources"
        actions={(
          <button className="primary-button" type="button" disabled={Boolean(busy) || !sql.trim() || mutationDisabled} onClick={() => void execute()}>
            <Play size={13} />Run
          </button>
        )}
      />
      <div className="custom-sql-workbench">
        <SqlInputList inputs={inputs} busy={Boolean(busy)} onAdd={addInput} onRemove={removeInput} />
        <div className="custom-sql-editor">
          <CodeEditor language="sql" ariaLabel="Custom taxonomy SQL" value={sql} onChange={setSql} />
        </div>
      </div>
      {(error || busy) && <div className={error ? "inline-error" : "editor-message"}>{error || busy}</div>}
      {result && (
        <div className="sql-results">
          <div className="sql-result-summary">
            <span>{result.changeset_size} bytes changed</span>
            <span>{result.operation_id === null ? "No operation created" : `Operation ${result.operation_id}`}</span>
            <span>{result.script_saved ? "Script saved" : "Script not saved"}</span>
            {result.result_sets.some((set) => set.truncated) && (
              <button className="secondary-button" type="button" disabled={Boolean(busy)} onClick={() => void exportQuery()}>
                <Download size={13} />Export full query
              </button>
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
      )}
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
