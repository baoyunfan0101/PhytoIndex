import { X } from "lucide-react";
import { Button, IconButton } from "../../../shared/ui";

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
      <header><span>Recent searches</span><Button variant="ghost" size="small" onClick={onClear}>Clear all</Button></header>
      <div>
        {searches.map((query) => (
          <div className="recent-search-row" key={query.toLocaleLowerCase()}>
            <button type="button" onClick={() => onSelect(query)}>{query}</button>
            <IconButton size="small" title={`Remove ${query}`} aria-label={`Remove ${query}`} onClick={() => onRemove(query)}><X size={13} /></IconButton>
          </div>
        ))}
      </div>
    </section>
  );
}
