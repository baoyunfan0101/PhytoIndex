import { LoaderCircle } from "lucide-react";
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
  loading = false,
}: {
  idPrefix: string;
  loading?: boolean;
  onHover: (index: number) => void;
  onSelect: (suggestion: TaxonSuggestion) => void;
  selectedIndex: number;
  suggestions: TaxonSuggestion[];
}) {
  if (!loading && suggestions.length === 0) return null;
  return (
    <div className="photo-search-suggestions" id={`${idPrefix}-listbox`} role="listbox">
      {loading && suggestions.length === 0 ? (
        <div className="suggestion-loading" role="status" aria-live="polite">
          <LoaderCircle className="spin" size={13} /> Loading suggestions...
        </div>
      ) : null}
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
          <SuggestionNames suggestion={suggestion} />
          <span className="suggestion-rank">{suggestion.rank}</span>
        </button>
      ))}
    </div>
  );
}

function SuggestionNames({ suggestion }: { suggestion: TaxonSuggestion }) {
  const { sci_name: scientific, zh_name: chinese, en_name: english } = suggestion.names;
  if (!scientific && !chinese && !english) {
    return <div className="suggestion-name-line"><span>Taxon {suggestion.taxon_id}</span></div>;
  }
  return (
    <div className="suggestion-name-line">
      {scientific && <strong>{scientific}</strong>}
      {scientific && chinese && <span className="suggestion-separator">{"\u00b7"}</span>}
      {chinese && <span>{chinese}</span>}
      {(scientific || chinese) && english && <span className="suggestion-separator">{"\u00b7"}</span>}
      {english && <span>{english}</span>}
    </div>
  );
}
