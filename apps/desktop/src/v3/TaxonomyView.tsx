import {
  ChevronDown,
  ChevronRight,
  Download,
  FileUp,
  Images,
  Play,
  RotateCcw,
  Search,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import {
  applyTaxonomyRows,
  displayTaxon,
  downloadCsv,
  errorMessage,
  executeCustomTaxonomySql,
  exportAllTaxonomyOperationsCsv,
  exportTaxonomyOperationCsv,
  getTaxonDetailNode,
  getTaxonomyTemplate,
  listTaxonChildren,
  listTaxonomyOperations,
  parseTaxonomyCsv,
  previewTaxonomyRows,
  revertTaxonomyOperation,
  searchTaxa,
  type TaxonDetailNode,
  type TaxonInputRow,
  type TaxonRowOutcome,
  type TaxonSearchResult,
  type TaxonomyOperation,
} from "./api";
import { EmptyState, SectionHeader, TaxonCard, VirtualList } from "./components";

export function TaxonomySearchView({
  taxonId,
  onOpenPhotos,
}: {
  taxonId?: number;
  onOpenPhotos: (taxonId: number, label: string) => void;
}) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<TaxonSearchResult[]>([]);
  const [selected, setSelected] = useState<TaxonSearchResult | null>(null);
  const [node, setNode] = useState<TaxonDetailNode | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (taxonId === undefined) return;
    getTaxonDetailNode(taxonId).then((next) => {
      setNode(next);
      setSelected({ summary: next.summary, detail: next.detail, matches: [] });
    }).catch((nextError) => setError(errorMessage(nextError)));
  }, [taxonId]);

  useEffect(() => {
    if (taxonId !== undefined) return;
    const value = query.trim();
    if (!value) {
      setResults([]);
      setSelected(null);
      setNode(null);
      return;
    }
    const timer = window.setTimeout(() => {
      searchTaxa(value).then((next) => {
        setResults(next);
        setSelected(next[0] ?? null);
      }).catch((nextError) => setError(errorMessage(nextError)));
    }, 180);
    return () => window.clearTimeout(timer);
  }, [query, taxonId]);

  useEffect(() => {
    if (!selected) {
      setNode(null);
      return;
    }
    getTaxonDetailNode(selected.summary.taxon_id).then(setNode).catch((nextError) => setError(errorMessage(nextError)));
  }, [selected]);

  async function toggleChildren() {
    if (!node) return;
    if (!expanded && node.children.items.length === 0) {
      const page = await listTaxonChildren(node.summary.taxon_id);
      setNode({ ...node, children: page });
    }
    setExpanded(!expanded);
  }

  async function navigateTo(nextTaxonId: number) {
    try {
      const next = await getTaxonDetailNode(nextTaxonId);
      setNode(next);
      setSelected({ summary: next.summary, detail: next.detail, matches: [] });
      setExpanded(false);
    } catch (nextError) {
      setError(errorMessage(nextError));
    }
  }

  const visible = selected ? [selected, ...(expanded && node ? node.children.items.map((child) => ({
    summary: { taxon_id: child.taxon_id, rank: child.rank, names: child.names, breadcrumb: [] },
    detail: {
      taxon_id: child.taxon_id,
      rank: child.rank,
      parent_taxon_id: node.summary.taxon_id,
      geological_range: null,
      names: {
        sci_name: { name_id: 0, name: child.names.sci_name ?? `Taxon ${child.taxon_id}`, authority_year: null, source: null },
        synonyms: [],
        zh_name: child.names.zh_name ? { name_id: 0, name: child.names.zh_name, authority_year: null, source: null } : null,
        zh_aliases: [],
        en_name: child.names.en_name ? { name_id: 0, name: child.names.en_name, authority_year: null, source: null } : null,
        en_aliases: [],
      },
    },
    matches: [],
  })) : [])] : [];

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
              items={results}
              rowHeight={62}
              itemKey={(item) => item.summary.taxon_id}
              renderItem={(item) => (
                <TaxonCard taxon={item.summary} active={selected?.summary.taxon_id === item.summary.taxon_id} onClick={() => setSelected(item)} />
              )}
            />
          </aside>
        )}
        <main className="taxonomy-records">
          {error ? <EmptyState title="Taxonomy unavailable" detail={error} /> : visible.length === 0 ? (
            <EmptyState icon={Search} title={taxonId === undefined ? "Search taxonomy" : "Loading taxon"} detail="Results include accepted names and aliases." />
          ) : (
            <VirtualList
              items={visible}
              rowHeight={expanded ? 194 : 250}
              itemKey={(item) => item.summary.taxon_id}
              renderItem={(item, index) => (
                <TaxonRecord
                  result={item}
                  detail={index === 0 ? node : null}
                  child={index > 0}
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
  result,
  detail,
  child,
  expanded,
  onToggleChildren,
  onOpenTaxon,
  onOpenPhotos,
}: {
  result: TaxonSearchResult;
  detail: TaxonDetailNode | null;
  child: boolean;
  expanded: boolean;
  onToggleChildren: () => void;
  onOpenTaxon: (taxonId: number) => void;
  onOpenPhotos: (taxonId: number, label: string) => void;
}) {
  const label = displayTaxon(result.summary);
  return (
    <article className={`taxon-record${child ? " child" : ""}`}>
      {!child && result.summary.breadcrumb.length > 0 && (
        <div className="taxon-breadcrumb">
          {result.summary.breadcrumb.map((item) => (
            <span key={item.taxon_id}><button type="button" onClick={() => onOpenTaxon(item.taxon_id)}>{displayTaxon(item)}</button><ChevronRight size={11} /></span>
          ))}
        </div>
      )}
      <div className="taxon-record-heading">
        <div><span className="taxon-rank">{result.summary.rank}</span><strong>{label}</strong><small>Taxon {result.summary.taxon_id}</small></div>
        <div className="record-actions">
          {!child && <button type="button" onClick={onToggleChildren}>{expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}Children</button>}
          <button type="button" onClick={() => onOpenPhotos(result.summary.taxon_id, label)}><Images size={14} />Photos</button>
        </div>
      </div>
      <div className="taxon-name-summary">
        <span><b>Scientific</b>{result.detail.names.sci_name.name}</span>
        <span><b>Chinese</b>{result.detail.names.zh_name?.name ?? "-"}</span>
        <span><b>English</b>{result.detail.names.en_name?.name ?? "-"}</span>
        <span><b>Synonyms</b>{result.detail.names.synonyms.map((name) => name.name).join("; ") || "-"}</span>
        <span><b>Range</b>{result.detail.geological_range ?? "-"}</span>
        {detail && <span><b>Children</b>{detail.children.items.length}</span>}
      </div>
    </article>
  );
}

const inputFields: Array<keyof TaxonInputRow> = [
  "kingdom", "order", "family", "genus", "species", "authority_year", "synonyms",
  "zh_name", "zh_alias", "en_name", "en_alias", "geological_range", "source",
];

export function FormattedUpdateView() {
  const [rows, setRows] = useState<TaxonInputRow[]>([{ species: "" }]);
  const [outcomes, setOutcomes] = useState<TaxonRowOutcome[]>([]);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);

  async function importFile(file: File) {
    setRows(await parseTaxonomyCsv(await file.text()));
    setOutcomes([]);
  }

  function updateRow(index: number, field: keyof TaxonInputRow, value: string) {
    setRows((current) => current.map((row, rowIndex) => rowIndex === index ? {
      ...row,
      [field]: ["synonyms", "zh_alias", "en_alias"].includes(field) ? value.split(";") : value || null,
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
      }
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="formatted-view">
      <SectionHeader title="Formatted update" detail="Pipe-delimited UTF-8 input or direct table editing" actions={
        <>
          <label className="secondary-button file-button"><FileUp size={13} />Upload CSV<input type="file" accept=".csv,text/csv" onChange={(event) => {
            const file = event.target.files?.[0];
            if (file) void importFile(file);
          }} /></label>
          <button className="secondary-button" type="button" onClick={() => void getTaxonomyTemplate().then((csv) => downloadCsv("taxonomy-template.csv", csv))}><Download size={13} />Template</button>
          <button className="secondary-button" type="button" disabled={busy} onClick={() => void run("preview")}>Preview</button>
          <button className="primary-button" type="button" disabled={busy} onClick={() => void run("apply")}><Play size={13} />Apply</button>
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
              {inputFields.map((field) => <input key={field} value={Array.isArray(row[field]) ? (row[field] as string[]).join(";") : String(row[field] ?? "")} onChange={(event) => updateRow(index, field, event.target.value)} />)}
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

export function CustomUpdateView() {
  const [sql, setSql] = useState("UPDATE taxa\nSET geological_range = 'Recent'\nWHERE taxon_id = 0;");
  const [csv, setCsv] = useState<{ columns: string[]; rows: string[][] } | null>(null);
  const [message, setMessage] = useState("");

  async function execute() {
    try {
      const result = await executeCustomTaxonomySql(sql, csv);
      setMessage(`Applied custom SQL (${result.changeset_size} bytes changed). This action has no operation log.`);
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    }
  }

  return (
    <div className="custom-update-view">
      <SectionHeader title="Custom update" detail="Direct SQL is intentionally not logged or recoverable" actions={
        <>
          <label className="secondary-button file-button"><FileUp size={13} />Attach CSV<input type="file" accept=".csv,text/csv" onChange={(event) => {
            const file = event.target.files?.[0];
            if (!file) return;
            void file.text().then((text) => {
              const lines = text.split(/\r?\n/).filter(Boolean).map((line) => line.split("|"));
              setCsv(lines.length ? { columns: lines[0], rows: lines.slice(1) } : null);
            });
          }} /></label>
          <button className="primary-button" type="button" onClick={() => void execute()}><Play size={13} />Execute</button>
        </>
      } />
      <div className="code-editor sql-editor"><pre aria-hidden="true">{highlightSql(sql)}</pre><textarea spellCheck={false} value={sql} onChange={(event) => setSql(event.target.value)} /></div>
      <div className="editor-message">{message}</div>
    </div>
  );
}

export function TaxonomyHistoryView() {
  const [items, setItems] = useState<TaxonomyOperation[]>([]);
  const [error, setError] = useState("");
  const load = useCallback(() => {
    listTaxonomyOperations().then((page) => setItems(page.items)).catch((nextError) => setError(errorMessage(nextError)));
  }, []);
  useEffect(load, [load]);

  return (
    <div className="history-view">
      <SectionHeader title="Taxonomy update history" detail={`${items.length} operations`} actions={
        <button className="secondary-button" type="button" onClick={() => void exportAllTaxonomyOperationsCsv().then((csv) => downloadCsv("taxonomy-operations.csv", csv))}><Download size={13} />Export all</button>
      } />
      {error ? <EmptyState title="Unable to load history" detail={error} /> : (
        <VirtualList
          className="history-list"
          items={items}
          rowHeight={72}
          itemKey={(item) => item.operation_id}
          renderItem={(item) => (
            <article className="operation-row">
              <div><strong>Operation {item.operation_id}</strong><span>{item.applied_at} / {item.result.succeeded_rows} succeeded / {item.result.failed_rows} failed</span></div>
              <div className="operation-actions">
                <button type="button" title="Export input" onClick={() => void exportTaxonomyOperationCsv(item.operation_id).then((csv) => downloadCsv(`taxonomy-operation-${item.operation_id}.csv`, csv))}><Download size={14} /></button>
                <button type="button" title="Revert" onClick={() => void revertTaxonomyOperation(item.operation_id).then(load)}><RotateCcw size={14} /></button>
              </div>
            </article>
          )}
        />
      )}
    </div>
  );
}

function highlightSql(value: string) {
  const keywords = /\b(SELECT|UPDATE|INSERT|DELETE|FROM|WHERE|SET|INTO|VALUES|JOIN|ON|AND|OR|NULL|AS)\b/gi;
  return value.split(keywords).map((part, index) => /^(SELECT|UPDATE|INSERT|DELETE|FROM|WHERE|SET|INTO|VALUES|JOIN|ON|AND|OR|NULL|AS)$/i.test(part) ? <mark key={index}>{part}</mark> : part);
}
