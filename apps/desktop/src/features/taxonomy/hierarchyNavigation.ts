import type { TaxonNameMatch, TaxonSearchResult } from "../../api/taxonomy";

export type HierarchyPositions = Record<string, number>;

export type HierarchyNavigationState = {
  currentTaxonId: number;
  childrenExpanded: boolean;
  childrenRequested: boolean;
};

export type HierarchyNavigationAction =
  | { type: "toggle-children" }
  | { type: "navigate"; taxonId: number }
  | { type: "reset"; taxonId: number };

export function createHierarchyNavigationState(taxonId: number): HierarchyNavigationState {
  return {
    currentTaxonId: taxonId,
    childrenExpanded: false,
    childrenRequested: false,
  };
}

export function hierarchyNavigationReducer(
  state: HierarchyNavigationState,
  action: HierarchyNavigationAction,
): HierarchyNavigationState {
  if (action.type === "toggle-children") {
    return {
      ...state,
      childrenExpanded: !state.childrenExpanded,
      childrenRequested: state.childrenRequested || !state.childrenExpanded,
    };
  }
  if (action.type === "navigate" && action.taxonId === state.currentTaxonId) return state;
  return createHierarchyNavigationState(action.taxonId);
}

export function currentTaxonForRoot(
  rootTaxonId: number,
  positions: HierarchyPositions,
): number {
  return positions[String(rootTaxonId)] ?? rootTaxonId;
}

export function recordHierarchyPosition(
  positions: HierarchyPositions,
  rootTaxonId: number,
  currentTaxonId: number,
): HierarchyPositions {
  if (positions[String(rootTaxonId)] === currentTaxonId) return positions;
  return { ...positions, [String(rootTaxonId)]: currentTaxonId };
}

export function reconcileSelectedRoot(
  selectedRootTaxonId: number | null,
  resultTaxonIds: number[],
): number | null {
  if (selectedRootTaxonId !== null && resultTaxonIds.includes(selectedRootTaxonId)) {
    return selectedRootTaxonId;
  }
  return resultTaxonIds[0] ?? null;
}

const nonAcceptedNameLabels: Partial<Record<TaxonNameMatch["name_type"], string>> = {
  synonym: "Matched synonym",
  zh_alias: "Matched Chinese alias",
  en_alias: "Matched English alias",
};

export function taxonSearchMatchExplanation(result: TaxonSearchResult): string | null {
  if (result.matches.some((match) => (
    match.name_type === "sci_name"
    || match.name_type === "zh_name"
    || match.name_type === "en_name"
  ))) {
    return null;
  }
  const explanations = result.matches.flatMap((match) => {
    const label = nonAcceptedNameLabels[match.name_type];
    return label ? [`${label}: ${match.name}`] : [];
  });
  return explanations.length > 0 ? [...new Set(explanations)].join("; ") : null;
}
