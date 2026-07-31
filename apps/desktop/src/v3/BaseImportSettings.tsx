import { Beaker, Database, FilePlus2, Play, RotateCcw, Save, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import {
  addBaseImportCsvSource,
  addBaseImportSqliteSource,
  applyBaseImport,
  createBaseImportSession,
  discardBaseImportSession,
  errorMessage,
  executeBaseImportSql,
  getDefaultBaseImportSql,
  getTaxonomyBaseMetadata,
  inspectBaseImportSources,
  removeBaseImportSource,
  resetDefaultBaseImportSql,
  saveDefaultBaseImportSql,
  selectCsvFile,
  selectSqliteDatabase,
  validateBaseImport,
  waitForOperation,
  type BaseImportSource,
  type BaseImportValidationResult,
  type TaxonomyBaseMetadata,
} from "./api";
import { CodeEditor } from "./CodeEditor";
import { Modal, SectionHeader, VirtualList } from "./components";
import { useViewState } from "./viewState";
import { emitTaxonomyMutation } from "./taxonomyMutations";

export function BaseImportSettings({ onApplied }: { onApplied?: () => void }) {
  const [metadata, setMetadata] = useState<TaxonomyBaseMetadata | null>(null);
  const [sessionId, setSessionId] = useViewState<string | null>("base-import.session-id", null);
  const [sources, setSources] = useViewState<BaseImportSource[]>("base-import.sources", []);
  const [sql, setSql] = useViewState("base-import.sql", "");
  const [validation, setValidation] = useViewState<BaseImportValidationResult | null>(
    "base-import.validation",
    null,
  );
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState("");
  const [message, setMessage] = useState("");

  useEffect(() => {
    void getTaxonomyBaseMetadata().then(setMetadata).catch((nextError) => setMessage(errorMessage(nextError)));
    if (!sql) {
      void getDefaultBaseImportSql().then(setSql).catch((nextError) => setMessage(errorMessage(nextError)));
    }
    if (sessionId) {
      void refreshSources(sessionId).catch((nextError) => setMessage(errorMessage(nextError)));
    }
  }, []);

  async function ensureSession(): Promise<string> {
    if (sessionId) return sessionId;
    const session = await createBaseImportSession();
    setSessionId(session.session_id);
    return session.session_id;
  }

  async function refreshSources(id = sessionId) {
    if (!id) return;
    setSources(await inspectBaseImportSources(id));
  }

  async function addSqlite() {
    const path = await selectSqliteDatabase();
    if (!path) return;
    setBusy("Adding SQLite source");
    setMessage("");
    setValidation(null);
    try {
      const id = await ensureSession();
      await addBaseImportSqliteSource(id, path);
      await refreshSources(id);
      setValidation(null);
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  async function addCsv() {
    const path = await selectCsvFile();
    if (!path) return;
    const filename = path.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, "") ?? "source";
    const suggested = filename.replace(/[^A-Za-z0-9_]/g, "_").replace(/^[^A-Za-z_]/, "_$&");
    const tableName = window.prompt("Destination table name", suggested)?.trim();
    if (!tableName) return;
    setBusy("Importing CSV source");
    setMessage("");
    setValidation(null);
    try {
      const id = await ensureSession();
      await addBaseImportCsvSource(id, tableName, path);
      await refreshSources(id);
      setValidation(null);
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
    try {
      const id = await ensureSession();
      const result = await executeBaseImportSql(id, sql);
      setMessage(`${result.statements_executed} statements executed; session revision ${result.session_revision}.`);
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  async function validate() {
    if (!sessionId) return;
    setBusy("Validating candidate");
    setMessage("");
    try {
      const next = await validateBaseImport(sessionId);
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
    if (!sessionId) return;
    setBusy("Applying base import");
    setMessage("Replacing taxonomy database");
    try {
      const operation = await applyBaseImport(sessionId);
      const completed = await waitForOperation("mapping", operation.task_id, (next) => setMessage(next.message));
      if (completed.error) throw new Error(completed.error);
      setMetadata(await getTaxonomyBaseMetadata());
      setSessionId(null);
      setSources([]);
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

  async function discard() {
    setBusy("Discarding import session");
    setMessage("");
    try {
      if (sessionId) await discardBaseImportSession(sessionId);
      setSessionId(null);
      setSources([]);
      setValidation(null);
      setMessage("Import workspace discarded.");
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  async function removeSource(source: BaseImportSource) {
    if (!sessionId) return;
    if (
      validation
      && !window.confirm("Removing this source will invalidate the current validation result.")
    ) {
      return;
    }
    setBusy(`Removing ${source.source_alias}`);
    setMessage("");
    try {
      const result = await removeBaseImportSource(sessionId, source.source_alias);
      setSources(result.sources);
      setValidation(null);
      setMessage(`Source removed; session revision ${result.session_revision}.`);
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  async function saveDefault() {
    try {
      await saveDefaultBaseImportSql(sql);
      setMessage("Default import SQL saved.");
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    }
  }

  async function resetDefault() {
    try {
      setSql(await resetDefaultBaseImportSql());
      setMessage("Built-in import SQL restored.");
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    }
  }

  return (
    <div className="base-import-settings">
      <SectionHeader
        title="Base Import"
        detail="Build and validate an isolated taxonomy candidate before replacement"
        actions={(
          <>
            <button className="secondary-button" type="button" disabled={Boolean(busy)} onClick={() => void addSqlite()}>
              <Database size={13} />Add SQLite
            </button>
            <button className="secondary-button" type="button" disabled={Boolean(busy)} onClick={() => void addCsv()}>
              <FilePlus2 size={13} />Add CSV
            </button>
            {sessionId && (
              <button className="secondary-button" type="button" disabled={Boolean(busy)} onClick={() => void discard()}>
                <Trash2 size={13} />Discard
              </button>
            )}
          </>
        )}
      />
      <div className="base-metadata-grid">
        <Metric label="Current source" value={metadata?.source_path ?? "Not imported"} />
        <Metric label="Taxa" value={metadata ? String(metadata.taxa_count) : "-"} />
        <Metric label="Names" value={metadata ? String(metadata.taxon_names_count) : "-"} />
        <Metric label="Imported" value={metadata?.imported_at ?? "-"} />
      </div>
      <div className="base-session-line">
        <span>Session</span><code>{sessionId ?? "Created when the first source is added"}</code>
      </div>
      <div className="base-import-workbench">
        <aside className="sql-sources">
          <strong>Session sources</strong>
          {sources.length === 0 && <span>No sources attached</span>}
          {sources.map((source) => (
            <section className="base-source" key={source.source_alias}>
              <header>
                <span>
                  <b>{source.source_alias}</b>
                  <i>{source.source_type.toUpperCase()}</i>
                </span>
                <button
                  type="button"
                  title={`Remove ${source.source_alias}`}
                  disabled={Boolean(busy)}
                  onClick={() => void removeSource(source)}
                >
                  <Trash2 size={13} />
                </button>
              </header>
              <code title={source.original_path}>{source.original_path}</code>
              <small className={source.available ? "available" : "unavailable"}>
                {source.available ? "Available" : "Original file unavailable"}
                {" / "}
                Schema {source.schema_status}
              </small>
              {source.schema.objects.map((object) => (
                <details key={`${source.source_alias}:${object.name}`}>
                  <summary>{object.object_type} {object.name}</summary>
                  {object.columns.map((column) => (
                    <span key={column.name}>{column.name}<i>{column.declared_type ?? "untyped"}</i></span>
                  ))}
                </details>
              ))}
            </section>
          ))}
        </aside>
        <div className="base-import-editor">
          <CodeEditor language="sql" ariaLabel="Base import SQL" value={sql} onChange={setSql} />
          <div className="base-import-actions">
            <button className="secondary-button" type="button" onClick={() => void resetDefault()}>
              <RotateCcw size={13} />Reset default
            </button>
            <button className="secondary-button" type="button" onClick={() => void saveDefault()}>
              <Save size={13} />Save as default
            </button>
            <button className="secondary-button" type="button" disabled={Boolean(busy) || !sql.trim()} onClick={() => void execute()}>
              <Play size={13} />Execute
            </button>
            <button className="secondary-button" type="button" disabled={Boolean(busy) || !sessionId} onClick={() => void validate()}>
              <Beaker size={13} />Validate
            </button>
            <button className="primary-button" type="button" disabled={Boolean(busy) || !validation?.can_apply} onClick={() => setConfirming(true)}>
              Apply candidate
            </button>
          </div>
        </div>
      </div>
      {(message || busy) && <div className="editor-message">{busy || message}</div>}
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
