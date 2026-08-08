import {
  ChevronDown,
  ChevronRight,
  Download,
  FileUp,
  Images,
  Play,
  Search,
  Trash2,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  applyTaxonomyRows,
  displayTaxon,
  getTaxonDetailNode,
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
} from "../../api/taxonomy";
import { downloadCsv, errorMessage } from "../../api/common";
import { getTaxonomyNameSeparator } from "../../api/settings";
import { Button, EmptyState, SectionHeader, VirtualList } from "../../shared/ui";
import { TaxonCard } from "./TaxonCard";
import { useMetadataChange } from "../../shared/metadataChanges";
import { useCursorPage } from "../../shared/useCursorPage";
import { useTaxonSearch } from "./useTaxonSearch";
import { useViewState } from "../../shared/viewState";
import { emitTaxonomyMutation, useTaxonomyMutation } from "./taxonomyMutations";
import { ResizablePanels } from "../../shared/ResizablePanels";
import { moveSuggestionSelection } from "../../shared/suggestionNavigation";
import { SearchSuggestions, suggestionLabel } from "../photos/search/SearchSuggestions";
import { useTaxonSuggestions } from "./useTaxonSuggestions";

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
  const [submittedQuery, setSubmittedQuery] = useViewState("taxonomy-search.submitted-query", query);
  const [selected, setSelected] = useViewState<TaxonSearchResult | null>("taxonomy-search.selected", null);
  const [node, setNode] = useViewState<TaxonDetailNode | null>("taxonomy-search.node", null);
  const [expanded, setExpanded] = useViewState("taxonomy-search.expanded", false);
  const [error, setError] = useState("");
  const [refreshKey, setRefreshKey] = useState(0);
  const [selectedSuggestionIndex, setSelectedSuggestionIndex] = useState(-1);
  const [suggestionsOpen, setSuggestionsOpen] = useState(false);
  const loadedTaxonId = useRef<number | null>(null);
  const selectedTaxonId = useRef(selected?.summary.taxon_id ?? null);
  const taxonomySearch = useTaxonSearch(submittedQuery, {
    enabled: taxonId === undefined,
    stateKey: "taxonomy-search.results",
    refreshKey,
  });
  const taxonomySuggestions = useTaxonSuggestions(query, taxonId === undefined);
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
    setSelectedSuggestionIndex(-1);
  }, [query]);

  useEffect(() => {
    if (selectedSuggestionIndex >= taxonomySuggestions.suggestions.length) {
      setSelectedSuggestionIndex(-1);
    }
  }, [selectedSuggestionIndex, taxonomySuggestions.suggestions.length]);

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
    if (!submittedQuery.trim()) {
      setSelected(null);
      setNode(null);
      return;
    }
    setSelected((current) => current
      ? taxonomySearch.results.find((item) => item.summary.taxon_id === current.summary.taxon_id)
        ?? taxonomySearch.results[0]
        ?? null
      : taxonomySearch.results[0] ?? null);
  }, [submittedQuery, taxonId, taxonomySearch.results]);

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

  function submitTaxonomySearch(value = query) {
    const normalized = value.trim();
    if (!normalized) return;
    setQuery(normalized);
    setSubmittedQuery(normalized);
    setSelected(null);
    setNode(null);
    setError("");
    setSelectedSuggestionIndex(-1);
    setSuggestionsOpen(false);
  }

  const visible: TaxonomyRecordItem[] = selected ? [
    { kind: "selected", result: selected },
    ...(expanded ? children.items.map((child) => ({ kind: "child" as const, child })) : []),
  ] : [];
  const resultsPane = (
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
  );
  const recordsPane = (
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
  );

  return (
    <div className="taxonomy-search-view">
      {taxonId === undefined && (
        <header className="workbench-toolbar">
          <div className="taxonomy-search-combobox">
            <label className="search-field taxonomy-search-field">
              <Search size={14} />
              <input
                role="combobox"
                aria-autocomplete="list"
                aria-controls="taxonomy-search-suggestions-listbox"
                aria-expanded={suggestionsOpen && taxonomySuggestions.suggestions.length > 0}
                aria-activedescendant={selectedSuggestionIndex >= 0 ? `taxonomy-search-suggestions-option-${selectedSuggestionIndex}` : undefined}
                autoFocus
                autoCapitalize="none"
                autoComplete="off"
                autoCorrect="off"
                spellCheck={false}
                value={query}
                onChange={(event) => {
                  setQuery(event.target.value);
                  setSuggestionsOpen(true);
                }}
                onKeyDown={(event) => {
                  if (event.nativeEvent.isComposing) return;
                  const suggestions = taxonomySuggestions.suggestions;
                  if (suggestions.length > 0 && event.key === "ArrowDown") {
                    event.preventDefault();
                    setSuggestionsOpen(true);
                    setSelectedSuggestionIndex((current) => moveSuggestionSelection(current, suggestions.length, 1));
                    return;
                  }
                  if (suggestions.length > 0 && event.key === "ArrowUp") {
                    event.preventDefault();
                    setSelectedSuggestionIndex((current) => moveSuggestionSelection(current, suggestions.length, -1));
                    return;
                  }
                  if (event.key === "Enter") {
                    event.preventDefault();
                    const suggestion = suggestions[selectedSuggestionIndex];
                    submitTaxonomySearch(suggestion ? suggestionLabel(suggestion, query) : query);
                  }
                }}
                placeholder="Search scientific, Chinese, or English names"
              />
            </label>
            {suggestionsOpen && query.trim() && (
              <SearchSuggestions
                idPrefix="taxonomy-search-suggestions"
                suggestions={taxonomySuggestions.suggestions}
                selectedIndex={selectedSuggestionIndex}
                onHover={setSelectedSuggestionIndex}
                onSelect={(suggestion) => submitTaxonomySearch(suggestionLabel(suggestion, query))}
              />
            )}
          </div>
        </header>
      )}
      {taxonId === undefined ? (
        <ResizablePanels
          className="taxonomy-columns"
          initialRatio={0.28}
          minFirst={220}
          minSecond={360}
          separatorLabel="Resize taxonomy results and details"
          stateKey="taxonomy-search.columns"
          first={resultsPane}
          second={recordsPane}
        />
      ) : <div className="taxonomy-columns taxonomy-columns-single">{recordsPane}</div>}
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

  function deleteRow(index: number) {
    if (index === 0) return;
    setRows((current) => current.filter((_, rowIndex) => rowIndex !== index));
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
          <label className="button button-secondary file-button"><FileUp size={13} />Upload CSV<input type="file" accept=".csv,text/csv" onChange={(event) => {
            const file = event.target.files?.[0];
            if (file) void importFile(file);
          }} /></label>
          <Button onClick={() => void getTaxonomyTemplate().then((csv) => downloadCsv("taxonomy-template.csv", csv))}><Download size={13} />Template</Button>
          <Button disabled={busy} onClick={() => void run("preview")}>Preview</Button>
          <Button variant="primary" disabled={busy || mutationDisabled} onClick={() => void run("apply")}><Play size={13} />Apply</Button>
        </>
      } />
      <ResizablePanels
        className="formatted-split"
        direction="vertical"
        initialRatio={0.52}
        minFirst={190}
        minSecond={150}
        separatorLabel="Resize formatted input and result log"
        stateKey="formatted-update.rows"
        first={(<div className="input-table">
        <div className="input-table-head"><span>#</span>{inputFields.map((field) => <span key={field}>{field}</span>)}<span /></div>
        <VirtualList
          items={rows}
          rowHeight={38}
          itemKey={(_, index) => index}
          renderItem={(row, index) => (
            <div className="input-table-row">
              <span>{index + 1}</span>
              {inputFields.map((field) => <input key={field} value={Array.isArray(row[field]) ? (row[field] as string[]).join(separator) : String(row[field] ?? "")} onChange={(event) => updateRow(index, field, event.target.value)} />)}
              <button
                className="input-row-delete"
                type="button"
                aria-label={`Delete row ${index + 1}`}
                title={index === 0 ? "The first row cannot be deleted" : "Delete row"}
                disabled={index === 0}
                onClick={() => deleteRow(index)}
              ><Trash2 size={13} /></button>
            </div>
          )}
        />
        <Button className="table-add-row" variant="ghost" onClick={() => setRows((current) => [...current, {}])}>+ Add row</Button>
        </div>)}
        second={(<div className="formatted-log">
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
        </div>)}
      />
    </div>
  );
}
