import type { TaxonDisplayNames } from "../../api/taxonomy";

export function taxonCommonNameLine(names: TaxonDisplayNames): string {
  return [names.zh_name, names.en_name]
    .filter((name): name is string => Boolean(name))
    .join(" \u00b7 ") || "-";
}
