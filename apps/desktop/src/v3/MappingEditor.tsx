import { Link2Off, Search, Sparkles } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import {
  clearPhotoMapping,
  displayTaxon,
  errorMessage,
  getPhotoTaxonMatch,
  searchTaxa,
  selectPhotoTaxon,
  setPhotoMapping,
  type Photo,
  type PhotoTaxonMatch,
  type TaxonSearchResult,
} from "./api";
import { Busy, EmptyState, MappingBadge, PhotoStage, TaxonCard, VirtualList } from "./components";

export function MappingEditor({
  photo,
  embedded = false,
  onChanged,
}: {
  photo: Photo;
  embedded?: boolean;
  onChanged?: () => void;
}) {
  const [match, setMatch] = useState<PhotoTaxonMatch | null>(null);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<TaxonSearchResult[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");

  const reload = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      setMatch(await getPhotoTaxonMatch(photo.photo_id));
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setLoading(false);
    }
  }, [photo.photo_id]);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    const value = query.trim();
    if (!value) {
      setResults([]);
      return;
    }
    const timer = window.setTimeout(() => {
      searchTaxa(value, 60).then(setResults).catch((nextError) => setError(errorMessage(nextError)));
    }, 180);
    return () => window.clearTimeout(timer);
  }, [query]);

  async function mutate(label: string, action: () => Promise<unknown>) {
    setBusy(label);
    setError("");
    try {
      await action();
      await reload();
      onChanged?.();
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  const mappedTaxon =
    match?.mapping.taxon_id !== null
      ? results.find((item) => item.summary.taxon_id === match?.mapping.taxon_id)?.summary ?? null
      : null;

  return (
    <div className={`mapping-editor${embedded ? " embedded" : ""}`}>
      <div className="editor-photo-column">
        <div className="two-line-heading">
          <strong>{photo.filename}</strong>
          <span>{mappedTaxon ? displayTaxon(mappedTaxon) : match?.mapping.taxon_id ? `Taxon ${match.mapping.taxon_id}` : "No mapped taxon"}</span>
        </div>
        <PhotoStage photo={photo} />
      </div>
      <div className="editor-mapping-column">
        <section className="mapping-current">
          <header>
            <strong>Current mapping</strong>
            {match && <MappingBadge status={match.mapping.status} />}
          </header>
          {loading ? (
            <Busy label="Loading mapping" />
          ) : match?.mapping.status === "matched" ? (
            <div className="current-mapping-card">
              <div><span>Taxon</span><strong>{match.mapping.taxon_id}</strong></div>
              <button className="secondary-button" type="button" disabled={Boolean(busy)} onClick={() => void mutate("Clearing", () => clearPhotoMapping(photo.photo_id))}>
                <Link2Off size={13} /> Clear mapping
              </button>
            </div>
          ) : match?.mapping.status === "ambiguous" ? (
            <div className="candidate-stack">
              {match.candidates.map((candidate) => (
                <TaxonCard
                  compact
                  key={candidate.summary.taxon_id}
                  taxon={candidate.summary}
                  actions={
                    <button className="small-button" type="button" onClick={() => void mutate("Selecting", () => selectPhotoTaxon(photo.photo_id, candidate.summary.taxon_id))}>
                      Select
                    </button>
                  }
                />
              ))}
            </div>
          ) : (
            <EmptyState title="No automatic match" detail="Search below to assign any taxon." icon={Sparkles} />
          )}
        </section>
        <section className="mapping-search">
          <label className="search-field">
            <Search size={14} />
            <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search taxonomy" />
          </label>
          <VirtualList
            className="mapping-search-results"
            items={results}
            rowHeight={58}
            itemKey={(item) => item.summary.taxon_id}
            renderItem={(item) => (
              <TaxonCard
                compact
                taxon={item.summary}
                actions={
                  <button className="small-button" type="button" disabled={Boolean(busy)} onClick={() => void mutate("Mapping", () => setPhotoMapping(photo.photo_id, item.summary.taxon_id))}>
                    Map
                  </button>
                }
              />
            )}
          />
        </section>
        {busy && <div className="floating-progress"><Busy label={busy} /></div>}
        {error && <div className="inline-error">{error}</div>}
      </div>
    </div>
  );
}
