import {
  ChevronDown,
  ChevronRight,
  Download,
  FileUp,
  Images,
  Play,
  Search,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  applyTaxonomyRows,
  displayTaxon,
  downloadCsv,
  errorMessage,
  getTaxonDetailNode,
  getTaxonomyNameSeparator,
  getTaxonomyTemplate,
  listTaxonChildren,
  parseTaxonomyCsv,
  previewTaxonomyRows,
  type TaxonChild,
  type TaxonDetail,
  type TaxonDetailNode,
  type TaxonInputRow,
  type TaxonRowOutcome,
  type TaxonSearchResult,
  type TaxonSummary,
} from "./api";
import { EmptyState, SectionHeader, TaxonCard, VirtualList } from "./components";
import { useMetadataChange } from "./metadataChanges";
import { useCursorPage } from "./useCursorPage";
import { useTaxonSearch } from "./useTaxonSearch";
import { useViewState } from "./viewState";
import { emitTaxonomyMutation, useTaxonomyMutation } from "./taxonomyMutations";

type TaxonomyRecordItem =
  | { kind: "selected"; result: TaxonSearchResult }
  | { kind: "child"; child: TaxonChild };

export function TaxonomySearchView({
  taxonId,
  onTaxonChange,
  onOpenPhotos,
}: {
  taxonId?: number;
  onTaxonChange?: (taxonId: number, label: string) => void;
  onOpenPhotos: (taxonId: number, label: string) => void;
}) {
  const [query, setQuery] = useViewState("taxonomy-search.query", "");
  const [selected, setSelected] = useViewState<TaxonSearchResult | null>("taxonomy-search.selected", null);
  const [node, setNode] = useViewState<TaxonDetailNode | null>("taxonomy-search.node", null);
  const [expanded, setExpanded] = useViewState("taxonomy-search.expanded", false);
  const [error, setError] = useState("");
  const [refreshKey, setRefreshKey] = useState(0);
  const loadedTaxonId = useRef<number | null>(null);
  const selectedTaxonId = useRef(selected?.summary.taxon_id ?? null);
  const taxonomySearch = useTaxonSearch(query, {
    enabled: taxonId === undefined,
    stateKey: "taxonomy-search.results",
    refreshKey,
  });
  const children = useCursorPage<TaxonChild, number | null>({
    params: selected?.summary.taxon_id ?? null,
    resetKey: selected?.summary.taxon_id ?? null,
    stateKey: "taxonomy-search.children",
    enabled: expanded && selected !== null,
    loadPage: (selectedTaxonId, cursor) => listTaxonChildren(selectedTaxonId!, cursor),
  });
  useTaxonomyMutation(() => {
    loadedTaxonId.current = null;
    setNode(null);
    setRefreshKey((current) => current + 1);
    if (expanded) void children.reload();
  });

  useEffect(() => {
    if (taxonId === undefined) return;
    if (loadedTaxonId.current === taxonId) return;
    getTaxonDetailNode(taxonId).then((next) => {
      loadedTaxonId.current = next.summary.taxon_id;
      setNode(next);
      setSelected({ summary: next.summary, detail: next.detail, matches: [] });
      onTaxonChange?.(next.summary.taxon_id, displayTaxon(next.summary));
    }).catch((nextError) => setError(errorMessage(nextError)));
  }, [taxonId]);

  useEffect(() => {
    if (taxonId !== undefined) return;
    if (!query.trim()) {
      setSelected(null);
      setNode(null);
      return;
    }
    setSelected((current) => current
      ? taxonomySearch.results.find((item) => item.summary.taxon_id === current.summary.taxon_id)
        ?? taxonomySearch.results[0]
        ?? null
      : taxonomySearch.results[0] ?? null);
  }, [query, taxonId, taxonomySearch.results]);

  useEffect(() => {
    if (!selected) {
      loadedTaxonId.current = null;
      selectedTaxonId.current = null;
      setNode(null);
      return;
    }
    const selectionChanged = selectedTaxonId.current !== selected.summary.taxon_id;
    selectedTaxonId.current = selected.summary.taxon_id;
    if (selectionChanged) setExpanded(false);
    if (node?.summary.taxon_id === selected.summary.taxon_id) {
      loadedTaxonId.current = selected.summary.taxon_id;
      return;
    }
    if (loadedTaxonId.current === selected.summary.taxon_id) return;
    getTaxonDetailNode(selected.summary.taxon_id).then((next) => {
      loadedTaxonId.current = next.summary.taxon_id;
      setNode(next);
    }).catch((nextError) => setError(errorMessage(nextError)));
  }, [selected]);

  function toggleChildren() {
    if (node) setExpanded((current) => !current);
  }

  async function navigateTo(nextTaxonId: number) {
    try {
      const next = await getTaxonDetailNode(nextTaxonId);
      loadedTaxonId.current = next.summary.taxon_id;
      setNode(next);
      setSelected({ summary: next.summary, detail: next.detail, matches: [] });
      setExpanded(false);
      onTaxonChange?.(next.summary.taxon_id, displayTaxon(next.summary));
    } catch (nextError) {
      setError(errorMessage(nextError));
    }
  }

  const visible: TaxonomyRecordItem[] = selected ? [
    { kind: "selected", result: selected },
    ...(expanded ? children.items.map((child) => ({ kind: "child" as const, child })) : []),
  ] : [];

  return (
    <div className="taxonomy-search-view">
      {taxonId === undefined && (
        <header className="workbench-toolbar">
          <label className="search-field taxonomy-search-field">
            <Search size={14} />
            <input autoFocus value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search scientific, Chinese, or English names" />
          </label>
        </header>
      )}
      <div className="taxonomy-columns">
        {taxonId === undefined && (
          <aside className="taxonomy-results">
            <VirtualList
              stateKey="taxonomy-search.results-list"
              items={taxonomySearch.results}
              rowHeight={62}
              itemKey={(item) => item.summary.taxon_id}
              renderItem={(item) => (
                <TaxonCard taxon={item.summary} active={selected?.summary.taxon_id === item.summary.taxon_id} onClick={() => setSelected(item)} />
              )}
            />
          </aside>
        )}
        <main className="taxonomy-records">
          {(error || taxonomySearch.error || children.error) ? <EmptyState title="Taxonomy unavailable" detail={error || taxonomySearch.error || children.error} /> : visible.length === 0 ? (
            <EmptyState icon={Search} title={taxonId === undefined ? "Search taxonomy" : "Loading taxon"} detail="Results include accepted names and aliases." />
          ) : (
            <VirtualList
              stateKey="taxonomy-search.records-list"
              items={visible}
              rowHeight={expanded ? 194 : 250}
              itemKey={(item) => item.kind === "selected" ? item.result.summary.taxon_id : item.child.taxon_id}
              onNearEnd={() => void children.loadMore()}
              renderItem={(item, index) => (
                <TaxonRecord
                  summary={item.kind === "selected" ? item.result.summary : item.child}
                  detail={item.kind === "selected" ? item.result.detail : null}
                  breadcrumb={item.kind === "selected" ? item.result.summary.breadcrumb : []}
                  loadedChildCount={index === 0 ? node?.children.items.length ?? null : null}
                  child={item.kind === "child"}
                  expanded={expanded}
                  onToggleChildren={() => void toggleChildren()}
                  onOpenTaxon={(nextTaxonId) => void navigateTo(nextTaxonId)}
                  onOpenPhotos={onOpenPhotos}
                />
              )}
            />
          )}
        </main>
      </div>
    </div>
  );
}

function TaxonRecord({
  summary,
  detail,
  breadcrumb,
  loadedChildCount,
  child,
  expanded,
  onToggleChildren,
  onOpenTaxon,
  onOpenPhotos,
}: {
  summary: Pick<TaxonSummary, "taxon_id" | "rank" | "names">;
  detail: TaxonDetail | null;
  breadcrumb: TaxonSummary["breadcrumb"];
  loadedChildCount: number | null;
  child: boolean;
  expanded: boolean;
  onToggleChildren: () => void;
  onOpenTaxon: (taxonId: number) => void;
  onOpenPhotos: (taxonId: number, label: string) => void;
}) {
  const label = displayTaxon(summary);
  return (
    <article className={`taxon-record${child ? " child" : ""}`}>
      {!child && breadcrumb.length > 0 && (
        <div className="taxon-breadcrumb">
          {breadcrumb.map((item) => (
            <span key={item.taxon_id}><button type="button" onClick={() => onOpenTaxon(item.taxon_id)}>{displayTaxon(item)}</button><ChevronRight size={11} /></span>
          ))}
        </div>
      )}
      <div className="taxon-record-heading">
        <div><span className="taxon-rank">{summary.rank}</span><strong>{label}</strong><small>Taxon {summary.taxon_id}</small></div>
        <div className="record-actions">
          {!child && <button type="button" onClick={onToggleChildren}>{expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}Children</button>}
          <button type="button" onClick={() => onOpenPhotos(summary.taxon_id, label)}><Images size={14} />Photos</button>
        </div>
      </div>
      <div className="taxon-name-summary">
        <span><b>Scientific</b>{detail?.names.sci_name.name ?? summary.names.sci_name ?? "-"}</span>
        <span><b>Chinese</b>{detail?.names.zh_name?.name ?? summary.names.zh_name ?? "-"}</span>
        <span><b>English</b>{detail?.names.en_name?.name ?? summary.names.en_name ?? "-"}</span>
        {detail && <span><b>Synonyms</b>{detail.names.synonyms.map((name) => name.name).join("; ") || "-"}</span>}
        {detail && <span><b>Range</b>{detail.geological_range ?? "-"}</span>}
        {loadedChildCount !== null && <span><b>Children loaded</b>{loadedChildCount}</span>}
      </div>
    </article>
  );
}

const inputFields: Array<keyof TaxonInputRow> = [
  "kingdom", "order", "family", "genus", "species", "authority_year", "synonyms",
  "zh_name", "zh_alias", "en_name", "en_alias", "geological_range", "source",
];

export function FormattedUpdateView({ mutationDisabled = false }: { mutationDisabled?: boolean }) {
  const [rows, setRows] = useState<TaxonInputRow[]>([{ species: "" }]);
  const [outcomes, setOutcomes] = useState<TaxonRowOutcome[]>([]);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const [separator, setSeparator] = useState(";");

  useEffect(() => {
    getTaxonomyNameSeparator()
      .then(setSeparator)
      .catch((nextError) => setMessage(errorMessage(nextError)));
  }, []);
  useMetadataChange((change) => {
    if (change.key === "taxonomy_name_separator") setSeparator(change.value);
  });
  useTaxonomyMutation((mutation) => {
    if (mutation.kind !== "replacement") return;
    setOutcomes([]);
    setMessage("Previous preview cleared because the taxonomy database was replaced.");
  });

  async function importFile(file: File) {
    setRows(await parseTaxonomyCsv(await file.text()));
    setOutcomes([]);
  }

  function updateRow(index: number, field: keyof TaxonInputRow, value: string) {
    setRows((current) => current.map((row, rowIndex) => rowIndex === index ? {
      ...row,
      [field]: ["synonyms", "zh_alias", "en_alias"].includes(field)
        ? value === "" ? [] : value.split(separator)
        : value || null,
    } : row));
  }

  async function run(kind: "preview" | "apply") {
    setBusy(true);
    setMessage("");
    try {
      if (kind === "preview") {
        const result = await previewTaxonomyRows(rows);
        setOutcomes(result.rows);
        setMessage(`${result.rows.length} rows previewed`);
      } else {
        const result = await applyTaxonomyRows(rows);
        setOutcomes(result.rows);
        setMessage(`${result.succeeded_rows} succeeded, ${result.failed_rows} failed`);
        emitTaxonomyMutation();
      }
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="formatted-view">
      <SectionHeader title="Formatted update" detail={`Pipe-delimited UTF-8 input or direct table editing. Multiple names use "${separator}".`} actions={
        <>
          <label className="secondary-button file-button"><FileUp size={13} />Upload CSV<input type="file" accept=".csv,text/csv" onChange={(event) => {
            const file = event.target.files?.[0];
            if (file) void importFile(file);
          }} /></label>
          <button className="secondary-button" type="button" onClick={() => void getTaxonomyTemplate().then((csv) => downloadCsv("taxonomy-template.csv", csv))}><Download size={13} />Template</button>
          <button className="secondary-button" type="button" disabled={busy} onClick={() => void run("preview")}>Preview</button>
          <button className="primary-button" type="button" disabled={busy || mutationDisabled} onClick={() => void run("apply")}><Play size={13} />Apply</button>
        </>
      } />
      <div className="input-table">
        <div className="input-table-head"><span>#</span>{inputFields.map((field) => <span key={field}>{field}</span>)}</div>
        <VirtualList
          items={rows}
          rowHeight={38}
          itemKey={(_, index) => index}
          renderItem={(row, index) => (
            <div className="input-table-row">
              <span>{index + 1}</span>
              {inputFields.map((field) => <input key={field} value={Array.isArray(row[field]) ? (row[field] as string[]).join(separator) : String(row[field] ?? "")} onChange={(event) => updateRow(index, field, event.target.value)} />)}
            </div>
          )}
        />
        <button className="table-add-row" type="button" onClick={() => setRows((current) => [...current, {}])}>+ Add row</button>
      </div>
      <div className="formatted-log">
        <SectionHeader title="Result log" detail={message || "Preview and apply return the same log format"} />
        <VirtualList
          items={outcomes}
          rowHeight={64}
          itemKey={(item) => item.row_number}
          renderItem={(item) => (
            <div className="log-row">
              <b>{item.row_number}</b>
              <div><strong>{item.operation_types.join(" + ")}</strong><span>{item.message}</span></div>
              <code>{item.changes.map((change) => `${change.field}: ${change.old_value ?? "-"} -> ${change.new_value ?? "-"}`).join(" | ")}</code>
            </div>
          )}
        />
      </div>
    </div>
  );
}
