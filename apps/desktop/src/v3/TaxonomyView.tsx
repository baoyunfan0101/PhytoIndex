import { useEffect, useMemo, useState } from "react";
import {
  Braces,
  Database,
  FileClock,
  History,
  Search,
  SlidersHorizontal,
  TreeDeciduous,
} from "lucide-react";
import {
  listTaxonomyOperations,
  searchTaxa,
  type TaxonSearchResult,
  type TaxonomyOperation,
} from "./api";
import {
  BusyState,
  EmptyState,
  PanelTitle,
  Tabs,
  errorMessage,
} from "./components";

type TaxonomyMode = "Search" | "Quick" | "Custom" | "History";
const taxonomyModes = ["Search", "Quick", "Custom", "History"] as const;

export function TaxonomyView({
  onStatus,
}: {
  onStatus: (message: string, busy?: boolean) => void;
}) {
  const [mode, setMode] = useState<TaxonomyMode>("Search");
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<TaxonSearchResult[]>([]);
  const [selected, setSelected] = useState<TaxonSearchResult | null>(null);
  const [operations, setOperations] = useState<TaxonomyOperation[]>([]);
  const [selectedOperation, setSelectedOperation] =
    useState<TaxonomyOperation | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (mode !== "Search") {
      return;
    }
    const value = query.trim();
    if (!value) {
      setResults([]);
      setSelected(null);
      setLoading(false);
      onStatus("Search the taxonomy");
      return;
    }
    let active = true;
    const timer = window.setTimeout(() => {
      setLoading(true);
      setError("");
      searchTaxa(value)
        .then((nextResults) => {
          if (!active) {
            return;
          }
          setResults(nextResults);
          setSelected((current) => {
            if (!current) {
              return nextResults[0] ?? null;
            }
            return (
              nextResults.find(
                (result) =>
                  result.summary.taxon_id === current.summary.taxon_id,
              ) ??
              nextResults[0] ??
              null
            );
          });
          onStatus(`${nextResults.length} taxonomy results`);
        })
        .catch((nextError) => {
          if (!active) {
            return;
          }
          const message = errorMessage(nextError);
          setError(message);
          onStatus(message);
        })
        .finally(() => {
          if (active) {
            setLoading(false);
          }
        });
    }, 180);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [mode, onStatus, query]);

  useEffect(() => {
    if (mode !== "History") {
      return;
    }
    setLoading(true);
    setError("");
    listTaxonomyOperations()
      .then((page) => {
        setOperations(page.items);
        setSelectedOperation(page.items[0] ?? null);
        onStatus(`${page.items.length} taxonomy operations`);
      })
      .catch((nextError) => {
        const message = errorMessage(nextError);
        setError(message);
        onStatus(message);
      })
      .finally(() => setLoading(false));
  }, [mode, onStatus]);

  const acceptedNames = useMemo(() => {
    if (!selected) {
      return [];
    }
    return [
      ...selected.detail.names.scientific.map((name) => ({
        ...name,
        kind: "Scientific",
      })),
      ...selected.detail.names.english.map((name) => ({
        ...name,
        kind: "English",
      })),
      ...selected.detail.names.chinese.map((name) => ({
        ...name,
        kind: "Chinese",
      })),
    ];
  }, [selected]);

  return (
    <section className="module-view">
      <div className="topbar">
        <label className="command-field">
          <Search size={14} />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search names, taxa, or identifiers"
            aria-label="Search taxonomy"
            disabled={mode !== "Search"}
          />
        </label>
        <span className="topbar-context">Taxonomy</span>
      </div>
      <Tabs items={taxonomyModes} value={mode} onChange={setMode} />

      {mode === "Search" && (
        <div className="workspace-grid taxonomy-grid">
          <aside className="panel sidebar-panel">
            <PanelTitle trailing={<span className="counter">{results.length}</span>}>
              Results
            </PanelTitle>
            {loading ? (
              <BusyState label="Searching taxonomy" />
            ) : error ? (
              <EmptyState title="Search failed" detail={error} />
            ) : !query.trim() ? (
              <EmptyState
                icon={Search}
                title="Search taxonomy"
                detail="Use a scientific, English, or Chinese name."
              />
            ) : results.length === 0 ? (
              <EmptyState
                icon={TreeDeciduous}
                title="No results"
                detail={`Nothing matched "${query.trim()}".`}
              />
            ) : (
              <div className="result-list">
                {results.map((result) => (
                  <button
                    className={`result-row${selected?.summary.taxon_id === result.summary.taxon_id ? " active" : ""}`}
                    key={result.summary.taxon_id}
                    type="button"
                    onClick={() => setSelected(result)}
                  >
                    <span>{displayTaxonName(result)}</span>
                    <strong>{result.summary.rank}</strong>
                  </button>
                ))}
              </div>
            )}
          </aside>
          <main className="panel taxonomy-panel">
            <PanelTitle>Taxon</PanelTitle>
            {selected ? (
              <div className="taxon-detail">
                <div className="taxon-heading">
                  <TreeDeciduous size={20} />
                  <div>
                    <strong>{displayTaxonName(selected)}</strong>
                    <span>
                      {selected.summary.rank} / {selected.summary.taxon_id}
                    </span>
                  </div>
                </div>
                <div className="breadcrumb-line">
                  {selected.summary.breadcrumb
                    .map((item) =>
                      item.names.scientific ??
                      item.names.english ??
                      `Taxon ${item.taxon_id}`,
                    )
                    .join(" / ")}
                </div>
                <dl className="compact-details standalone">
                  <dt>Scientific</dt>
                  <dd>{selected.summary.names.scientific ?? "-"}</dd>
                  <dt>English</dt>
                  <dd>{selected.summary.names.english ?? "-"}</dd>
                  <dt>Chinese</dt>
                  <dd>{selected.summary.names.chinese ?? "-"}</dd>
                  <dt>Range</dt>
                  <dd>{selected.detail.geological_range ?? "-"}</dd>
                </dl>
              </div>
            ) : (
              <EmptyState
                icon={Database}
                title="No taxon selected"
                detail="Select a result to inspect its record."
              />
            )}
          </main>
          <aside className="panel impact-panel">
            <PanelTitle>Names</PanelTitle>
            {acceptedNames.length > 0 ? (
              <div className="name-list">
                {acceptedNames.map((name, index) => (
                  <div className="name-row" key={`${name.kind}:${name.name}:${index}`}>
                    <div>
                      <strong>{name.name}</strong>
                      <span>{name.kind}</span>
                    </div>
                    <span className={`pill ${name.is_accepted ? "green" : "gray"}`}>
                      {name.is_accepted ? "accepted" : "synonym"}
                    </span>
                  </div>
                ))}
              </div>
            ) : (
              <EmptyState title="No names" />
            )}
          </aside>
        </div>
      )}

      {mode === "Quick" && (
        <div className="single-panel">
          <EmptyState
            icon={SlidersHorizontal}
            title="Quick update"
            detail="Select a taxon in Search before editing its structured fields."
          />
        </div>
      )}

      {mode === "Custom" && (
        <div className="single-panel">
          <EmptyState
            icon={Braces}
            title="Custom update"
            detail="Validated SQL updates will be composed and previewed here."
          />
        </div>
      )}

      {mode === "History" && (
        <div className="workspace-grid history-grid">
          <main className="panel list-panel">
            <PanelTitle trailing={<span className="counter">{operations.length}</span>}>
              Changes
            </PanelTitle>
            {loading ? (
              <BusyState label="Loading taxonomy history" />
            ) : error ? (
              <EmptyState title="Unable to load history" detail={error} />
            ) : operations.length === 0 ? (
              <EmptyState
                icon={History}
                title="No taxonomy history"
                detail="Applied taxonomy updates will appear here."
              />
            ) : (
              <div className="photo-list">
                {operations.map((operation) => (
                  <button
                    className={`photo-row${selectedOperation?.operation_id === operation.operation_id ? " active" : ""}`}
                    key={operation.operation_id}
                    type="button"
                    onClick={() => setSelectedOperation(operation)}
                  >
                    <FileClock size={14} />
                    <div>
                      <strong>Operation {operation.operation_id}</strong>
                      <span>{operation.status}</span>
                    </div>
                  </button>
                ))}
              </div>
            )}
          </main>
          <aside className="panel preview-panel">
            <PanelTitle>Details</PanelTitle>
            {selectedOperation ? (
              <dl className="compact-details standalone">
                <dt>Batch</dt>
                <dd>{selectedOperation.batch_id}</dd>
                <dt>Row</dt>
                <dd>{selectedOperation.row_number}</dd>
                <dt>Status</dt>
                <dd>{selectedOperation.status}</dd>
                <dt>Changeset</dt>
                <dd>{selectedOperation.changeset_size} bytes</dd>
                <dt>Applied</dt>
                <dd>{selectedOperation.applied_at}</dd>
              </dl>
            ) : (
              <EmptyState title="No operation selected" />
            )}
          </aside>
        </div>
      )}
    </section>
  );
}

function displayTaxonName(result: TaxonSearchResult): string {
  return (
    result.summary.names.scientific ??
    result.summary.names.english ??
    result.summary.names.chinese ??
    `Taxon ${result.summary.taxon_id}`
  );
}
