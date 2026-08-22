import type { TaxonNameParts } from "../../api/general";
import type { TaxonDisplayNames } from "../../api/taxonomy";

export function formatTaxonName(
  names: TaxonDisplayNames,
  visibleNameParts: TaxonNameParts,
  fallback = "",
): string {
  const selected = [
    visibleNameParts.sci_name ? names.sci_name : null,
    visibleNameParts.zh_name ? names.zh_name : null,
    visibleNameParts.en_name ? names.en_name : null,
  ].filter((name): name is string => Boolean(name));
  return selected.join(" \u00b7 ") || fallback;
}
