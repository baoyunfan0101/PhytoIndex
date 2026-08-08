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
    <article className={`taxon-card${compact ? " compact" : ""}${active ? " active" : ""}`}>
      <button className="taxon-card-main" type="button" onClick={onClick}>
        <span className="taxon-rank">{taxon.rank}</span>
        <strong>{displayTaxon(taxon)}</strong>
        <span>{taxonCommonNameLine(taxon.names)}</span>
        {description ? <small className="taxon-card-description">{description}</small> : null}
      </button>
      {actions && <div className="taxon-card-actions">{actions}</div>}
    </article>
  );
}
