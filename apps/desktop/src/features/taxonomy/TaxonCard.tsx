import type { ReactNode } from "react";
import {
  displayTaxon,
  type TaxonDisplayNames,
  type TaxonRank,
} from "../../api/taxonomy";
import { taxonCommonNameLine } from "./taxonCardNames";

type TaxonCardTaxon = {
  taxon_id: number;
  rank: TaxonRank;
  names: TaxonDisplayNames;
};

export function TaxonCard({
  taxon,
  compact = false,
  active = false,
  actions,
  description,
  onClick,
}: {
  taxon: TaxonCardTaxon;
  compact?: boolean;
  active?: boolean;
  actions?: ReactNode;
  description?: string | null;
  onClick?: () => void;
}) {
  return (
    <article
      className={`taxon-card${compact ? " compact" : ""}${active ? " active" : ""}${onClick ? " clickable" : ""}`}
      onClick={onClick ? (event) => {
        const target = event.target;
        if (target instanceof Element && target.closest(".taxon-card-actions button")) return;
        const selection = window.getSelection();
        if (selection && !selection.isCollapsed && selection.toString().length > 0) return;
        onClick();
      } : undefined}
    >
      <button className="taxon-card-main" type="button">
        <span className="taxon-rank">{taxon.rank}</span>
        <strong>{displayTaxon(taxon)}</strong>
        <span>{taxonCommonNameLine(taxon.names)}</span>
        {description ? <small className="taxon-card-description">{description}</small> : null}
      </button>
      {actions && <div className="taxon-card-actions">{actions}</div>}
    </article>
  );
}
