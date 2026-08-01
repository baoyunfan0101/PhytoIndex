import { Link2Off, Search, Sparkles } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import {
  clearPhotoMapping,
  getPhotoMapping,
  getPhotoMappingCandidates,
  setPhotoMapping,
  type PhotoMappingDetail,
} from "../../api/mapping";
import type { Photo } from "../../api/photos";
import { displayTaxon, getTaxonDetailNode, type TaxonSummary } from "../../api/taxonomy";
import { errorMessage } from "../../api/common";
import { Busy, EmptyState, VirtualList } from "../../shared/ui";
import { PhotoStage } from "../photos/PhotoMedia";
import { TaxonCard } from "../taxonomy/TaxonCard";
import { MappingBadge } from "./MappingBadge";
import { emitPhotoMutation, usePhotoMutation } from "../photos/photoMutations";
import { useTaxonSearch } from "../taxonomy/useTaxonSearch";
import { useViewState } from "../../shared/viewState";

export function MappingEditor({
  photo,
  embedded = false,
  refreshKey = 0,
}: {
  photo: Photo;
  embedded?: boolean;
  refreshKey?: number;
}) {
  const [match, setMatch] = useViewState<PhotoMappingDetail | null>("mapping-editor.match", null);
  const [mappedTaxon, setMappedTaxon] = useViewState<TaxonSummary | null>("mapping-editor.taxon", null);
  const [query, setQuery] = useViewState("mapping-editor.query", "");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [mappingRefresh, setMappingRefresh] = useState(0);
  const taxonomySearch = useTaxonSearch(query, {
    limit: 60,
    stateKey: "mapping-editor.search",
  });
  usePhotoMutation((mutation) => {
    if (
      mutation.kind === "mapping"
      && (mutation.photoId === null || mutation.photoId === photo.photo_id)
    ) {
      setMappingRefresh((current) => current + 1);
    }
  });

  const reload = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const [mapping, candidates] = await Promise.all([
        getPhotoMapping(photo.photo_id),
        getPhotoMappingCandidates(photo.photo_id),
      ]);
      const nextMatch = { mapping, candidates };
      setMatch(nextMatch);
      if (nextMatch.mapping.status === "matched" && nextMatch.mapping.taxon_id !== null) {
        setMappedTaxon((await getTaxonDetailNode(nextMatch.mapping.taxon_id)).summary);
      } else {
        setMappedTaxon(null);
      }
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setLoading(false);
    }
  }, [photo.photo_id, refreshKey, mappingRefresh]);

  useEffect(() => {
    void reload();
  }, [reload]);

  async function mutate(label: string, action: () => Promise<unknown>) {
    setBusy(label);
    setError("");
    try {
      await action();
      await reload();
      emitPhotoMutation({ photoId: photo.photo_id, kind: "mapping" });
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

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
              <div>
                <span>{mappedTaxon?.rank ?? "Taxon"}</span>
                <strong>{mappedTaxon ? displayTaxon(mappedTaxon) : match.mapping.taxon_id}</strong>
              </div>
              <button className="secondary-button" type="button" disabled={Boolean(busy)} onClick={() => void mutate("Clearing", () => clearPhotoMapping(photo.photo_id))}>
                <Link2Off size={13} /> Clear mapping
              </button>
            </div>
          ) : match?.mapping.status === "ambiguous" ? (
            <VirtualList
              stateKey="mapping-editor.candidates"
              className="candidate-stack"
              items={match.candidates}
              rowHeight={60}
              itemKey={(candidate) => candidate.summary.taxon_id}
              renderItem={(candidate) => (
                <TaxonCard
                  compact
                  taxon={candidate.summary}
                  actions={
                    <button className="small-button" type="button" onClick={() => void mutate("Selecting", () => setPhotoMapping(photo.photo_id, candidate.summary.taxon_id))}>
                      Select
                    </button>
                  }
                />
              )}
            />
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
            stateKey="mapping-editor.results"
            className="mapping-search-results"
            items={taxonomySearch.results}
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
        {(error || taxonomySearch.error) && <div className="inline-error">{error || taxonomySearch.error}</div>}
      </div>
    </div>
  );
}
