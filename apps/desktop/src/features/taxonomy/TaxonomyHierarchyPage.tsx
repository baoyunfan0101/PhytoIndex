import { ChevronDown, ChevronRight, Images } from "lucide-react";
import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import {
  displayTaxon,
  displayTaxonDetail,
  getTaxonDetail,
  listTaxonChildren,
  type TaxonChild,
  type TaxonDetail,
  type TaxonNameDetail,
} from "../../api/taxonomy";
import { errorMessage } from "../../api/common";
import { Busy, Button, EmptyState } from "../../shared/ui";
import { useCursorPage } from "../../shared/useCursorPage";
import { useTaxonomyMutation } from "./taxonomyMutations";
import {
  createHierarchyNavigationState,
  hierarchyNavigationReducer,
} from "./hierarchyNavigation";

type TaxonomyHierarchyPageProps = {
  initialTaxonId: number;
  onTaxonChange?: (taxonId: number, label: string) => void;
  onOpenPhotos: (taxonId: number, label: string) => void;
};

type NameGroup = {
  label: string;
  records: TaxonNameDetail[];
};

export function TaxonomyHierarchyPage({
  initialTaxonId,
  onTaxonChange,
  onOpenPhotos,
}: TaxonomyHierarchyPageProps) {
  const [navigation, dispatch] = useReducer(
    hierarchyNavigationReducer,
    initialTaxonId,
    createHierarchyNavigationState,
  );
  const [detail, setDetail] = useState<TaxonDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [detailError, setDetailError] = useState("");
  const detailRequest = useRef(0);
  const onTaxonChangeRef = useRef(onTaxonChange);
  onTaxonChangeRef.current = onTaxonChange;
  const currentTaxonId = navigation.currentTaxonId;
  const children = useCursorPage<TaxonChild, number>({
    params: currentTaxonId,
    resetKey: currentTaxonId,
    enabled: navigation.childrenRequested,
    loadPage: (taxonId, cursor) => listTaxonChildren(taxonId, cursor),
  });

  const loadDetail = useCallback(async (taxonId: number, retainDetail = false) => {
    const request = ++detailRequest.current;
    setLoading(true);
    setDetailError("");
    if (!retainDetail) setDetail(null);
    try {
      const next = await getTaxonDetail(taxonId);
      if (request !== detailRequest.current) return;
      setDetail(next);
      onTaxonChangeRef.current?.(next.taxon_id, displayTaxonDetail(next));
    } catch (nextError) {
      if (request !== detailRequest.current) return;
      setDetailError(errorMessage(nextError));
      setDetail(null);
    } finally {
      if (request === detailRequest.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    dispatch({ type: "navigate", taxonId: initialTaxonId });
  }, [initialTaxonId]);

  useEffect(() => {
    void loadDetail(currentTaxonId);
    return () => {
      detailRequest.current += 1;
    };
  }, [currentTaxonId, loadDetail]);

  useTaxonomyMutation(() => {
    void loadDetail(currentTaxonId, true);
    if (navigation.childrenRequested) void children.reload();
  });

  function navigateTo(taxonId: number, label: string) {
    dispatch({ type: "navigate", taxonId });
    onTaxonChange?.(taxonId, label);
  }

  if (loading && detail === null) {
    return <div className="taxonomy-hierarchy-status"><Busy label="Loading taxon..." /></div>;
  }
  if (detail === null) {
    return (
      <div className="taxonomy-hierarchy-status">
        <EmptyState title="Taxon unavailable" detail={detailError || "The taxon could not be loaded."} />
      </div>
    );
  }

  const currentLabel = displayTaxonDetail(detail);
  const nameGroups: NameGroup[] = [
    { label: "Scientific accepted name", records: detail.names.sci_name ? [detail.names.sci_name] : [] },
    { label: "Scientific synonyms", records: detail.names.synonyms },
    { label: "Chinese accepted name", records: detail.names.zh_name ? [detail.names.zh_name] : [] },
    { label: "Chinese aliases", records: detail.names.zh_aliases },
    { label: "English accepted name", records: detail.names.en_name ? [detail.names.en_name] : [] },
    { label: "English aliases", records: detail.names.en_aliases },
  ];

  return (
    <main className="taxonomy-hierarchy-page">
      <div className="taxonomy-hierarchy-scroll">
        <nav className="taxonomy-hierarchy-breadcrumb" aria-label="Taxonomy breadcrumb">
          {detail.breadcrumb.map((item) => (
            <span key={item.taxon_id}>
              <button type="button" onClick={() => navigateTo(item.taxon_id, displayTaxon(item))}>
                {displayTaxon(item)}
              </button>
              <ChevronRight size={12} />
            </span>
          ))}
          <strong>{currentLabel}</strong>
        </nav>

        <header className="taxonomy-hierarchy-heading">
          <div>
            <span className="taxon-rank">{detail.rank}</span>
            <h2>{currentLabel}</h2>
            <small>Taxon {detail.taxon_id}</small>
          </div>
          <Button onClick={() => onOpenPhotos(detail.taxon_id, currentLabel)}>
            <Images size={14} /> Photos
          </Button>
        </header>

        <dl className="taxonomy-detail-meta">
          <div><dt>Rank</dt><dd>{detail.rank}</dd></div>
          <div><dt>Taxon ID</dt><dd>{detail.taxon_id}</dd></div>
          <div><dt>Geological range</dt><dd>{detail.geological_range ?? "-"}</dd></div>
        </dl>

        <section className="taxonomy-name-groups" aria-label="Taxon names">
          {nameGroups.map((group) => (
            <NameGroupView key={group.label} group={group} />
          ))}
        </section>

        <section className="taxonomy-children">
          <button
            className="taxonomy-children-toggle"
            type="button"
            aria-expanded={navigation.childrenExpanded}
            onClick={() => dispatch({ type: "toggle-children" })}
          >
            {navigation.childrenExpanded ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
            <strong>Children</strong>
          </button>
          {navigation.childrenExpanded && (
            <div className="taxonomy-child-list">
              {children.loading && children.items.length === 0 ? <Busy label="Loading children..." /> : null}
              {children.error ? <div className="inline-error">{children.error}</div> : null}
              {!children.loading && !children.error && children.items.length === 0 ? (
                <span className="taxonomy-children-empty">No direct children</span>
              ) : null}
              {children.items.map((child) => (
                <button
                  className="taxonomy-child-button"
                  type="button"
                  key={child.taxon_id}
                  onClick={() => navigateTo(child.taxon_id, displayTaxon(child))}
                >
                  <span className="taxon-rank">{child.rank}</span>
                  <strong>{displayTaxon(child)}</strong>
                  <span>{[child.names.zh_name, child.names.en_name].filter(Boolean).join(" \u00b7 ") || "-"}</span>
                  <ChevronRight size={14} />
                </button>
              ))}
              {children.hasMore ? (
                <Button disabled={children.loading} onClick={() => void children.loadMore()}>
                  {children.loading ? "Loading..." : "Load more"}
                </Button>
              ) : null}
            </div>
          )}
        </section>
      </div>
    </main>
  );
}

function NameGroupView({ group }: { group: NameGroup }) {
  return (
    <section className="taxonomy-name-group">
      <h3>{group.label}</h3>
      {group.records.length === 0 ? <span className="taxonomy-name-empty">-</span> : group.records.map((record) => (
        <article className="taxonomy-name-record" key={record.name_id}>
          <strong>{record.name}</strong>
          <dl>
            <div><dt>Authority</dt><dd>{record.authority_year ?? "-"}</dd></div>
            <div><dt>Source</dt><dd>{record.source ?? "-"}</dd></div>
          </dl>
        </article>
      ))}
    </section>
  );
}
