import type { RefObject } from "react";
import { normalizeSearchQuery } from "./recentSearchStorage";
import { PhotoSearch } from "./PhotoSearch";
import { RecentSearches } from "./RecentSearches";
import { usePhotoSearch, type SearchSubmitter } from "./usePhotoSearch";

export function EmptyWorkspace({
  inputRef,
  onClearRecent,
  onRemoveRecent,
  onSubmit,
  recentSearches,
  suggestionsEnabled,
}: {
  inputRef: RefObject<HTMLInputElement>;
  onClearRecent: () => void;
  onRemoveRecent: (query: string) => void;
  onSubmit: SearchSubmitter;
  recentSearches: string[];
  suggestionsEnabled: boolean;
}) {
  const search = usePhotoSearch(onSubmit);
  const hasQuery = Boolean(normalizeSearchQuery(search.query));
  return (
    <div className="empty-workspace">
      <div className="empty-workspace-search">
        <PhotoSearch
          autoFocus
          controller={search}
          enabled={suggestionsEnabled}
          idPrefix="empty-workspace-search"
          inputRef={inputRef}
        />
        {!hasQuery && (
          <RecentSearches
            searches={recentSearches}
            onClear={onClearRecent}
            onRemove={onRemoveRecent}
            onSelect={(query) => {
              search.setQuery(query);
              void search.submit(query);
            }}
          />
        )}
      </div>
    </div>
  );
}
