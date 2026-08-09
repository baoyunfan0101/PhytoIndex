import { ChevronLeft, ClockArrowDown, FileDown, RotateCcw } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  exportPhotoOperationAudit,
  exportPhotoOperationsAudit,
  exportTaxonomyOperationAudit,
  exportTaxonomyOperationInput,
  exportTaxonomyOperationsAudit,
  exportTaxonomyOperationsInput,
  listPhotoOperationAudit,
  listPhotoOperationSummaries,
  listTaxonomyOperationAudit,
  listTaxonomyOperationSummaries,
  rollbackPhotoOperation,
  rollbackTaxonomyOperation,
  type OperationAuditRow,
  type OperationSummary,
} from "../../api/operations";
import { errorMessage } from "../../api/common";
import { selectCsvDestination } from "../../api/dialogs";
import { Button, EmptyState, IconButton, VirtualList } from "../../shared/ui";
import { useCursorPage } from "../../shared/useCursorPage";
import { useViewState } from "../../shared/viewState";
import { emitPhotoMutation } from "../photos/photoMutations";
import { emitTaxonomyMutation, useTaxonomyMutation } from "../taxonomy/taxonomyMutations";
import {
  canExportReplayableInput,
  canRollbackOperations,
  formatAuditJson,
  getRollbackOrder,
  getSelectedOperations,
} from "./historySelection";

type HistoryDomain = "photo" | "taxonomy";

export function OperationHistoryView({
  domain,
  onStatus,
}: {
  domain: HistoryDomain;
  onStatus: (message: string) => void;
}) {
  const [selectedOperationId, setSelectedOperationId] = useViewState<number | null>(
    `${domain}-history.selected-operation`,
    null,
  );
  const [checkedOperationIds, setCheckedOperationIds] = useState<number[]>([]);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const summaries = useCursorPage<OperationSummary, HistoryDomain>({
    params: domain,
    resetKey: domain,
    stateKey: `${domain}-history.summaries`,
    loadPage: (nextDomain, cursor) => nextDomain === "photo"
      ? listPhotoOperationSummaries(cursor)
      : listTaxonomyOperationSummaries(cursor),
  });
  const selectedOperation = useMemo(
    () => summaries.items.find((item) => item.operation_id === selectedOperationId) ?? null,
    [selectedOperationId, summaries.items],
  );
  const checkedOperations = useMemo(
    () => getSelectedOperations(summaries.items, checkedOperationIds),
    [checkedOperationIds, summaries.items],
  );
  const allLoadedSelected = summaries.items.length > 0
    && checkedOperations.length === summaries.items.length;
  const someLoadedSelected = checkedOperations.length > 0 && !allLoadedSelected;

  useEffect(() => {
    const loadedIds = new Set(summaries.items.map((item) => item.operation_id));
    setCheckedOperationIds((current) => {
      const next = current.filter((operationId) => loadedIds.has(operationId));
      return next.length === current.length ? current : next;
    });
  }, [summaries.items]);

  useTaxonomyMutation(() => {
    if (domain === "taxonomy") void summaries.reload();
  });

  function toggleOperation(operationId: number, checked: boolean) {
    setCheckedOperationIds((current) => {
      if (checked) {
        return current.includes(operationId) ? current : [...current, operationId];
      }
      return current.filter((item) => item !== operationId);
    });
  }

  function toggleAllLoaded(checked: boolean) {
    setCheckedOperationIds(checked ? summaries.items.map((item) => item.operation_id) : []);
  }

  async function exportSelectedAudit() {
    const destination = await selectCsvDestination(`${domain}-operation-audit.csv`);
    if (!destination) return;
    setBusy("audit");
    setError("");
    try {
      const operationIds = checkedOperations.map((operation) => operation.operation_id);
      if (domain === "photo") {
        await exportPhotoOperationsAudit(operationIds, destination);
      } else {
        await exportTaxonomyOperationsAudit(operationIds, destination);
      }
      onStatus(`Audit exported to ${destination}`);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  async function exportSelectedInput() {
    const destination = await selectCsvDestination("taxonomy-replayable-input.csv");
    if (!destination) return;
    setBusy("input");
    setError("");
    try {
      await exportTaxonomyOperationsInput(
        checkedOperations.map((operation) => operation.operation_id),
        destination,
      );
      onStatus(`Replayable input exported to ${destination}`);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  async function rollbackSelected() {
    setBusy("rollback");
    setError("");
    let completed = 0;
    let failedOperationId: number | null = null;
    try {
      for (const operation of getRollbackOrder(checkedOperations)) {
        failedOperationId = operation.operation_id;
        if (domain === "photo") {
          await rollbackPhotoOperation(operation.operation_id);
        } else {
          await rollbackTaxonomyOperation(operation.operation_id);
        }
        completed += 1;
      }
      onStatus(`${completed} operation${completed === 1 ? "" : "s"} rolled back`);
    } catch (nextError) {
      const prefix = failedOperationId === null
        ? "Rollback failed"
        : `Rollback stopped at operation ${failedOperationId} after ${completed} completed`;
      setError(`${prefix}: ${errorMessage(nextError)}`);
    } finally {
      if (completed > 0) {
        if (domain === "photo") {
          emitPhotoMutation({ photoId: null, kind: "photo" });
        } else {
          emitTaxonomyMutation();
        }
        setCheckedOperationIds([]);
        await summaries.reload();
      }
      setBusy("");
    }
  }

  if (selectedOperation) {
    return (
      <OperationAuditDetail
        domain={domain}
        operation={selectedOperation}
        onBack={() => setSelectedOperationId(null)}
        onRolledBack={async () => {
          setSelectedOperationId(null);
          setCheckedOperationIds([]);
          await summaries.reload();
        }}
        onStatus={onStatus}
      />
    );
  }

  const actionDisabled = Boolean(busy) || checkedOperations.length === 0;
  return (
    <div className="history-view">
      <header className="history-toolbar">
        <div className="history-toolbar-left">
          <SelectAllCheckbox
            checked={allLoadedSelected}
            disabled={Boolean(busy) || summaries.items.length === 0}
            indeterminate={someLoadedSelected}
            onChange={toggleAllLoaded}
          />
          <div className="history-title">
            <strong>{domain === "photo" ? "Rename history" : "Taxonomy history"}</strong>
            <span>{summaries.loading
              ? "Loading operations..."
              : `${summaries.items.length} operations loaded${checkedOperations.length > 0
                ? ` / ${checkedOperations.length} selected`
                : ""}`}</span>
          </div>
        </div>
        <HistoryActions
          busy={busy}
          canExportAudit={!actionDisabled}
          canExportInput={domain === "taxonomy"
            && !busy
            && canExportReplayableInput(checkedOperations)}
          canRollback={!busy && canRollbackOperations(checkedOperations)}
          domain={domain}
          onExportAudit={() => void exportSelectedAudit()}
          onExportInput={() => void exportSelectedInput()}
          onRollback={() => void rollbackSelected()}
        />
      </header>
      <div className="history-body">
        {(error || summaries.error) && (
          <div className="inline-error" role="alert">{error || summaries.error}</div>
        )}
        {summaries.items.length === 0 && !summaries.loading ? (
          <EmptyState title="No operations" detail="Completed operations will appear here." />
        ) : (
          <VirtualList
            stateKey={`${domain}-history.summary-list`}
            className="history-list"
            items={summaries.items}
            rowHeight={48}
            itemKey={(item) => item.operation_id}
            onNearEnd={() => void summaries.loadMore()}
            renderItem={(item) => {
              const checked = checkedOperationIds.includes(item.operation_id);
              return (
                <article className={`operation-summary-row${checked ? " selected" : ""}`}>
                  <label className="operation-select" title={`Select operation ${item.operation_id}`}>
                    <input
                      aria-label={`Select operation ${item.operation_id}`}
                      checked={checked}
                      disabled={Boolean(busy)}
                      type="checkbox"
                      onChange={(event) => toggleOperation(item.operation_id, event.target.checked)}
                    />
                  </label>
                  <button
                    className="operation-summary-main"
                    type="button"
                    onClick={() => setSelectedOperationId(item.operation_id)}
                  >
                    <strong>{item.kind} #{item.operation_id}</strong>
                    <span>{item.applied_at}</span>
                    <span>{item.source}</span>
                    <span>
                      {item.total_items} total / {item.succeeded_items} succeeded / {item.failed_items} failed
                    </span>
                    <span>{item.rollbackable ? "Rollbackable" : "Audit only"}</span>
                  </button>
                </article>
              );
            }}
          />
        )}
      </div>
    </div>
  );
}

function SelectAllCheckbox({
  checked,
  disabled,
  indeterminate,
  onChange,
}: {
  checked: boolean;
  disabled: boolean;
  indeterminate: boolean;
  onChange: (checked: boolean) => void;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (inputRef.current) inputRef.current.indeterminate = indeterminate;
  }, [indeterminate]);
  return (
    <label className="history-select-all" title="Select all loaded operations">
      <input
        ref={inputRef}
        aria-label="Select all loaded operations"
        checked={checked}
        disabled={disabled}
        type="checkbox"
        onChange={(event) => onChange(event.target.checked)}
      />
    </label>
  );
}

function HistoryActions({
  busy,
  canExportAudit,
  canExportInput,
  canRollback,
  domain,
  onExportAudit,
  onExportInput,
  onRollback,
}: {
  busy: string;
  canExportAudit: boolean;
  canExportInput: boolean;
  canRollback: boolean;
  domain: HistoryDomain;
  onExportAudit: () => void;
  onExportInput: () => void;
  onRollback: () => void;
}) {
  return (
    <div className="history-actions">
      {domain === "taxonomy" && (
        <Button disabled={!canExportInput} onClick={onExportInput}>
          <FileDown size={14} />
          {busy === "input" ? "Exporting..." : "Export replayable input"}
        </Button>
      )}
      <Button disabled={!canExportAudit} onClick={onExportAudit}>
        <ClockArrowDown size={14} />
        {busy === "audit" ? "Exporting..." : "Export audit"}
      </Button>
      <Button disabled={!canRollback} onClick={onRollback}>
        <RotateCcw size={14} />
        {busy === "rollback" ? "Rolling back..." : "Rollback"}
      </Button>
    </div>
  );
}

function OperationAuditDetail({
  domain,
  operation,
  onBack,
  onRolledBack,
  onStatus,
}: {
  domain: HistoryDomain;
  operation: OperationSummary;
  onBack: () => void;
  onRolledBack: () => Promise<void>;
  onStatus: (message: string) => void;
}) {
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const audit = useCursorPage<OperationAuditRow, { domain: HistoryDomain; operationId: number }>({
    params: { domain, operationId: operation.operation_id },
    resetKey: `${domain}:${operation.operation_id}`,
    stateKey: `${domain}-history.audit.${operation.operation_id}`,
    loadPage: (params, cursor) => params.domain === "photo"
      ? listPhotoOperationAudit(params.operationId, cursor)
      : listTaxonomyOperationAudit(params.operationId, cursor),
  });

  async function exportAudit() {
    const destination = await selectCsvDestination(
      `${domain}-operation-${operation.operation_id}-audit.csv`,
    );
    if (!destination) return;
    setBusy("audit");
    setError("");
    try {
      if (domain === "photo") {
        await exportPhotoOperationAudit(operation.operation_id, destination);
      } else {
        await exportTaxonomyOperationAudit(operation.operation_id, destination);
      }
      onStatus(`Audit exported to ${destination}`);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  async function exportInput() {
    const destination = await selectCsvDestination(
      `taxonomy-operation-${operation.operation_id}-input.csv`,
    );
    if (!destination) return;
    setBusy("input");
    setError("");
    try {
      await exportTaxonomyOperationInput(operation.operation_id, destination);
      onStatus(`Replayable input exported to ${destination}`);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  async function rollback() {
    setBusy("rollback");
    setError("");
    try {
      if (domain === "photo") {
        await rollbackPhotoOperation(operation.operation_id);
        emitPhotoMutation({ photoId: null, kind: "photo" });
      } else {
        await rollbackTaxonomyOperation(operation.operation_id);
        emitTaxonomyMutation();
      }
      onStatus(`Operation ${operation.operation_id} rolled back`);
      await onRolledBack();
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  return (
    <div className="history-view history-detail-view">
      <header className="history-toolbar">
        <div className="history-detail-heading">
          <IconButton aria-label="Back to operations" title="Back to operations" onClick={onBack}>
            <ChevronLeft size={16} />
          </IconButton>
          <div className="history-title">
            <strong>{operation.kind} #{operation.operation_id}</strong>
            <span>{audit.loading
              ? "Loading audit rows..."
              : `${operation.applied_at} / ${operation.total_items} audit rows`}</span>
          </div>
        </div>
        <HistoryActions
          busy={busy}
          canExportAudit={!busy}
          canExportInput={domain === "taxonomy" && !busy && operation.has_formatted_input}
          canRollback={!busy && operation.rollbackable}
          domain={domain}
          onExportAudit={() => void exportAudit()}
          onExportInput={() => void exportInput()}
          onRollback={() => void rollback()}
        />
      </header>
      <div className="history-body">
        {(error || audit.error) && (
          <div className="inline-error" role="alert">{error || audit.error}</div>
        )}
        {audit.items.length === 0 && !audit.loading ? (
          <EmptyState title="No audit rows" detail="This operation has no audit details." />
        ) : (
          <div className="history-audit-scroll">
            {audit.items.map((item) => (
              <AuditRow key={`${item.operation_id}:${item.sequence}`} row={item} />
            ))}
            {audit.loading && <div className="history-load-status">Loading audit rows...</div>}
            {audit.hasMore && !audit.loading && (
              <div className="history-load-more">
                <Button onClick={() => void audit.loadMore()}>Load more</Button>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function AuditRow({ row }: { row: OperationAuditRow }) {
  return (
    <article className={`audit-row${row.succeeded ? "" : " failed"}`}>
      <header>
        <b>#{row.sequence}</b>
        <strong>{row.entity_type} / {row.action}</strong>
        <span>{row.message || "-"}</span>
      </header>
      <div className="audit-json-grid">
        <section>
          <b>Before</b>
          <pre>{formatAuditJson(row.before_json)}</pre>
        </section>
        <section>
          <b>After</b>
          <pre>{formatAuditJson(row.after_json)}</pre>
        </section>
      </div>
    </article>
  );
}
