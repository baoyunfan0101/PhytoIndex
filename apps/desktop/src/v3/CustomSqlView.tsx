import { Database, Download, FilePlus2, Play, Table2, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";
import {
  errorMessage,
  executeCustomSql,
  exportCustomSqlQuery,
  inspectSqlDataSource,
  selectCsvDestination,
  selectCsvFile,
  selectSqliteDatabase,
  type CustomSqlExecutionResult,
  type SqlDataSource,
  type SqlResultSet,
  type SqlSourceSchema,
  type SqlValue,
} from "./api";
import { CodeEditor } from "./CodeEditor";
import { EmptyState, SectionHeader, VirtualList } from "./components";
import { useViewState } from "./viewState";
import { emitTaxonomyMutation } from "./taxonomyMutations";

export function CustomSqlView({
  onStatus,
  mutationDisabled = false,
}: {
  onStatus: (message: string) => void;
  mutationDisabled?: boolean;
}) {
  const [sql, setSql] = useViewState(
    "custom-sql.draft",
    "SELECT taxon_id, rank, geological_range\nFROM taxa\nORDER BY taxon_id\nLIMIT 100;",
  );
  const [sources, setSources] = useViewState<SqlDataSource[]>("custom-sql.sources", []);
  const [schemas, setSchemas] = useViewState<SqlSourceSchema[]>("custom-sql.schemas", []);
  const [result, setResult] = useViewState<CustomSqlExecutionResult | null>("custom-sql.result", null);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");

  async function addSource(kind: "csv" | "sqlite") {
    const path = kind === "csv" ? await selectCsvFile() : await selectSqliteDatabase();
    if (!path) return;
    const suggestedAlias = sourceAlias(path, sources);
    const alias = window.prompt("SQL source alias", suggestedAlias)?.trim();
    if (!alias) return;
    const source: SqlDataSource = { kind, alias, path };
    setBusy("Inspecting source");
    setError("");
    try {
      const schema = await inspectSqlDataSource(source);
      setSources((current) => [...current, source]);
      setSchemas((current) => [...current, schema]);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  function removeSource(alias: string) {
    setSources((current) => current.filter((source) => source.alias !== alias));
    setSchemas((current) => current.filter((schema) => schema.alias !== alias));
  }

  async function execute() {
    setBusy("Executing SQL");
    setError("");
    try {
      const next = await executeCustomSql(sql, sources);
      setResult(next);
      const outcome = next.operation_id === null
        ? "Query completed without creating an operation"
        : `Mutation created taxonomy operation ${next.operation_id}`;
      if (next.operation_id !== null) emitTaxonomyMutation();
      onStatus(outcome);
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
      const exported = await exportCustomSqlQuery(sql, sources, destination);
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
          <>
            <button className="secondary-button" type="button" disabled={Boolean(busy)} onClick={() => void addSource("csv")}>
              <FilePlus2 size={13} />CSV source
            </button>
            <button className="secondary-button" type="button" disabled={Boolean(busy)} onClick={() => void addSource("sqlite")}>
              <Database size={13} />SQLite source
            </button>
            <button className="primary-button" type="button" disabled={Boolean(busy) || !sql.trim() || mutationDisabled} onClick={() => void execute()}>
              <Play size={13} />Run
            </button>
          </>
        )}
      />
      <div className="custom-sql-workbench">
        <aside className="sql-sources">
          <strong>Sources</strong>
          {schemas.length === 0 && <span>No external sources</span>}
          {schemas.map((schema) => (
            <section key={schema.alias}>
              <header>
                <b>{schema.alias}</b>
                <button type="button" title="Remove source" onClick={() => removeSource(schema.alias)}>
                  <Trash2 size={12} />
                </button>
              </header>
              {schema.objects.map((object) => (
                <details key={`${schema.alias}:${object.name}`}>
                  <summary><Table2 size={12} />{object.name}</summary>
                  {object.columns.map((column) => (
                    <span key={column.name}>{column.name}<i>{column.declared_type ?? "untyped"}</i></span>
                  ))}
                </details>
              ))}
            </section>
          ))}
        </aside>
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

function sourceAlias(path: string, existing: SqlDataSource[]): string {
  const stem = path.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, "") ?? "source";
  const normalized = stem.replace(/[^A-Za-z0-9_]/g, "_").replace(/^[^A-Za-z_]/, "_$&") || "source";
  let candidate = normalized;
  let suffix = 2;
  while (existing.some((source) => source.alias === candidate)) {
    candidate = `${normalized}_${suffix}`;
    suffix += 1;
  }
  return candidate;
}
