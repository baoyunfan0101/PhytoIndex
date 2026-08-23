import type { TaxonomyNameType } from "../../api/taxonomy";

type TaxonMatchedName = {
  name_type: TaxonomyNameType;
  name: string;
};

const nonAcceptedNameLabels: Partial<Record<TaxonomyNameType, string>> = {
  synonym: "Matched synonym",
  zh_alias: "Matched Chinese alias",
  en_alias: "Matched English alias",
};

export function taxonMatchExplanation(matches: TaxonMatchedName[]): string | null {
  if (matches.some((match) => (
    match.name_type === "sci_name"
    || match.name_type === "zh_name"
    || match.name_type === "en_name"
  ))) {
    return null;
  }
  const explanations = matches.flatMap((match) => {
    const label = nonAcceptedNameLabels[match.name_type];
    return label ? [`${label}: ${match.name}`] : [];
  });
  return explanations.length > 0 ? [...new Set(explanations)].join("; ") : null;
}
