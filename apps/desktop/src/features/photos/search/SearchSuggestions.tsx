import type { TaxonSuggestion } from "../../../api/taxonomy";

export function suggestionLabel(suggestion: TaxonSuggestion, fallback: string): string {
  return suggestion.names.sci_name
    ?? suggestion.names.zh_name
    ?? suggestion.names.en_name
    ?? fallback;
}

export function SearchSuggestions({
  idPrefix,
  onHover,
  onSelect,
  selectedIndex,
  suggestions,
}: {
  idPrefix: string;
  onHover: (index: number) => void;
  onSelect: (suggestion: TaxonSuggestion) => void;
  selectedIndex: number;
  suggestions: TaxonSuggestion[];
}) {
  if (suggestions.length === 0) return null;
  return (
    <div className="photo-search-suggestions" id={`${idPrefix}-listbox`} role="listbox">
      {suggestions.map((suggestion, index) => (
        <button
          className={index === selectedIndex ? "active" : ""}
          id={`${idPrefix}-option-${index}`}
          role="option"
          aria-selected={index === selectedIndex}
          type="button"
          key={suggestion.taxon_id}
          onMouseEnter={() => onHover(index)}
          onClick={() => onSelect(suggestion)}
        >
          <strong>{suggestion.names.sci_name ?? `Taxon ${suggestion.taxon_id}`}</strong>
          <span>{suggestion.rank} / {suggestion.names.zh_name ?? suggestion.names.en_name ?? ""}</span>
        </button>
      ))}
    </div>
  );
}
