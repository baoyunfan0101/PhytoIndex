import type { ReactNode } from "react";
import { displayTaxon, type TaxonSummary } from "../../api/taxonomy";
import { taxonCommonNameLine } from "./taxonCardNames";

export function TaxonCard({
  taxon,
  compact = false,
  active = false,
  actions,
  onClick,
}: {
  taxon: TaxonSummary;
  compact?: boolean;
  active?: boolean;
  actions?: ReactNode;
  onClick?: () => void;
}) {
  return (
    <article className={`taxon-card${compact ? " compact" : ""}${active ? " active" : ""}`}>
      <button className="taxon-card-main" type="button" onClick={onClick}>
        <span className="taxon-rank">{taxon.rank}</span>
        <strong>{displayTaxon(taxon)}</strong>
        <span>{taxonCommonNameLine(taxon.names)}</span>
      </button>
      {actions && <div className="taxon-card-actions">{actions}</div>}
    </article>
  );
}
