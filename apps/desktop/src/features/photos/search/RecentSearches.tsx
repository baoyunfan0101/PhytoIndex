import { X } from "lucide-react";

export function RecentSearches({
  onClear,
  onRemove,
  onSelect,
  searches,
}: {
  onClear: () => void;
  onRemove: (query: string) => void;
  onSelect: (query: string) => void;
  searches: string[];
}) {
  if (searches.length === 0) return null;
  return (
    <section className="recent-searches" aria-label="Recent searches">
      <header><span>Recent searches</span><button type="button" onClick={onClear}>Clear all</button></header>
      <div>
        {searches.map((query) => (
          <div className="recent-search-row" key={query.toLocaleLowerCase()}>
            <button type="button" onClick={() => onSelect(query)}>{query}</button>
            <button type="button" title={`Remove ${query}`} aria-label={`Remove ${query}`} onClick={() => onRemove(query)}><X size={13} /></button>
          </div>
        ))}
      </div>
    </section>
  );
}
