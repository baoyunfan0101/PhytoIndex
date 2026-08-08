import { ChevronLeft, Download, FileInput, RotateCcw } from "lucide-react";
import { useMemo, useState } from "react";
import {
  exportAllPhotoOperationAudit,
  exportAllReplayableTaxonomyInputs,
  exportAllTaxonomyOperationAudit,
  exportPhotoOperationAudit,
  exportTaxonomyOperationAudit,
  exportTaxonomyOperationInput,
  listPhotoOperationAudit,
  listPhotoOperationSummaries,
  listTaxonomyOperationAudit,
  listTaxonomyOperationSummaries,
  rollbackPhotoOperation,
  rollbackTaxonomyOperation,
  type OperationAuditRow,
  type OperationSummary,
} from "../../api/operations";
import { downloadCsv, errorMessage } from "../../api/common";
import { selectCsvDestination } from "../../api/dialogs";
import { Button, EmptyState, SectionHeader, VirtualList } from "../../shared/ui";
import { emitPhotoMutation } from "../photos/photoMutations";
import { useCursorPage } from "../../shared/useCursorPage";
import { useViewState } from "../../shared/viewState";
import { emitTaxonomyMutation, useTaxonomyMutation } from "../taxonomy/taxonomyMutations";

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
  const selected = useMemo(
    () => summaries.items.find((item) => item.operation_id === selectedOperationId) ?? null,
    [selectedOperationId, summaries.items],
  );
  useTaxonomyMutation(() => {
    if (domain === "taxonomy") void summaries.reload();
  });

  async function exportAllAudit() {
    const destination = await selectCsvDestination(`${domain}-operation-audit.csv`);
    if (!destination) return;
    setBusy("Exporting audit");
    try {
      if (domain === "photo") {
        await exportAllPhotoOperationAudit(destination);
      } else {
        await exportAllTaxonomyOperationAudit(destination);
      }
      onStatus(`Audit exported to ${destination}`);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  async function exportAllInput() {
    setBusy("Exporting replayable input");
    setError("");
    try {
      downloadCsv("taxonomy-formatted-input.csv", await exportAllReplayableTaxonomyInputs());
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  if (selected) {
    return (
      <OperationAuditDetail
        domain={domain}
        operation={selected}
        onBack={() => setSelectedOperationId(null)}
        onRolledBack={async () => {
          setSelectedOperationId(null);
          await summaries.reload();
        }}
        onStatus={onStatus}
      />
    );
  }

  return (
    <div className="history-view">
      <SectionHeader
        title={domain === "photo" ? "Rename history" : "Taxonomy history"}
        detail={busy || (summaries.loading
          ? "Loading operations..."
          : `${summaries.items.length} operations loaded`)}
        actions={(
          <>
            {domain === "taxonomy" && (
              <Button disabled={Boolean(busy)} onClick={() => void exportAllInput()}>
                <FileInput size={13} />{busy === "Exporting replayable input" ? "Exporting..." : "Export replayable input"}
              </Button>
            )}
            <Button disabled={Boolean(busy)} onClick={() => void exportAllAudit()}>
              <Download size={13} />{busy === "Exporting audit" ? "Exporting..." : "Export all audit"}
            </Button>
          </>
        )}
      />
      {(error || summaries.error) && <div className="inline-error">{error || summaries.error}</div>}
      {summaries.items.length === 0 && !summaries.loading ? (
        <EmptyState title="No operations" detail="Completed operations will appear here." />
      ) : (
        <VirtualList
          stateKey={`${domain}-history.summary-list`}
          className="history-list"
          items={summaries.items}
          rowHeight={72}
          itemKey={(item) => item.operation_id}
          onNearEnd={() => void summaries.loadMore()}
          renderItem={(item) => (
            <button
              className="operation-row operation-summary-row"
              type="button"
              onClick={() => setSelectedOperationId(item.operation_id)}
            >
              <div>
                <strong>{item.kind} #{item.operation_id}</strong>
                <span>{item.applied_at} / {item.source}</span>
              </div>
              <div className="operation-counts">
                <span><b>{item.total_items}</b> total</span>
                <span><b>{item.succeeded_items}</b> succeeded</span>
                <span><b>{item.failed_items}</b> failed</span>
                <span>{item.rollbackable ? "Rollbackable" : "Audit only"}</span>
              </div>
            </button>
          )}
        />
      )}
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
    const destination = await selectCsvDestination(`${domain}-operation-${operation.operation_id}-audit.csv`);
    if (!destination) return;
    setBusy("Exporting");
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
    setBusy("Exporting formatted input");
    setError("");
    try {
      downloadCsv(
        `taxonomy-operation-${operation.operation_id}-input.csv`,
        await exportTaxonomyOperationInput(operation.operation_id),
      );
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  async function rollback() {
    setBusy("Rolling back");
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
    <div className="history-view">
      <SectionHeader
        title={`${operation.kind} #${operation.operation_id}`}
        detail={busy || (audit.loading
          ? "Loading audit rows..."
          : `${operation.applied_at} / ${operation.total_items} audit rows`)}
        actions={(
          <>
            <Button onClick={onBack}>
              <ChevronLeft size={13} />Operations
            </Button>
            {domain === "taxonomy" && operation.has_formatted_input && (
              <Button disabled={Boolean(busy)} onClick={() => void exportInput()}>
                <FileInput size={13} />{busy === "Exporting formatted input" ? "Exporting..." : "Formatted input"}
              </Button>
            )}
            <Button disabled={Boolean(busy)} onClick={() => void exportAudit()}>
              <Download size={13} />{busy === "Exporting" ? "Exporting..." : "Export audit"}
            </Button>
            <Button
              disabled={Boolean(busy) || !operation.rollbackable}
              onClick={() => void rollback()}
            >
              <RotateCcw size={13} />{busy === "Rolling back" ? "Rolling back..." : "Rollback"}
            </Button>
          </>
        )}
      />
      {(error || audit.error) && <div className="inline-error">{error || audit.error}</div>}
      <VirtualList
        stateKey={`${domain}-history.audit-list.${operation.operation_id}`}
        className="history-list"
        items={audit.items}
        rowHeight={94}
        itemKey={(item) => `${item.operation_id}:${item.sequence}`}
        onNearEnd={() => void audit.loadMore()}
        renderItem={(item) => <AuditRow row={item} />}
      />
    </div>
  );
}

function AuditRow({ row }: { row: OperationAuditRow }) {
  const before = formatAuditState(row.before_json);
  const after = formatAuditState(row.after_json);
  return (
    <article className={`operation-row audit-row${row.succeeded ? "" : " failed"}`}>
      <b>{row.sequence}</b>
      <div>
        <strong>{row.entity_type} / {row.action}</strong>
        <span>{row.message}</span>
        {(before || after) && <code>{before || "-"} -&gt; {after || "-"}</code>}
      </div>
    </article>
  );
}

function formatAuditState(value: unknown): string {
  if (!value) return "";
  if (typeof value === "object") {
    const record = value as Record<string, unknown>;
    const path = record.directory_relative_path;
    const filename = record.filename;
    if (typeof filename === "string") {
      return typeof path === "string" && path ? `${path}/${filename}` : filename;
    }
  }
  return JSON.stringify(value);
}
