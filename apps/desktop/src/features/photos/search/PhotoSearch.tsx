import { useEffect, useRef, useState, type RefObject } from "react";
import { normalizeSearchQuery } from "./recentSearchStorage";
import { SearchInput } from "./SearchInput";
import { SearchSuggestions, suggestionLabel } from "./SearchSuggestions";
import type { PhotoSearchController } from "./usePhotoSearch";
import { useSearchSuggestions } from "./useSearchSuggestions";
import { moveSuggestionSelection } from "../../../shared/suggestionNavigation";

export function PhotoSearch({
  autoFocus = false,
  controller,
  enabled,
  idPrefix,
  inputRef,
}: {
  autoFocus?: boolean;
  controller: PhotoSearchController;
  enabled: boolean;
  idPrefix: string;
  inputRef?: RefObject<HTMLInputElement>;
}) {
  const localInputRef = useRef<HTMLInputElement>(null);
  const resolvedInputRef = inputRef ?? localInputRef;
  const [selectedIndex, setSelectedIndex] = useState(-1);
  const { suggestions } = useSearchSuggestions(controller.query, enabled);
  const hasQuery = Boolean(normalizeSearchQuery(controller.query));

  useEffect(() => {
    setSelectedIndex(-1);
  }, [controller.query]);

  useEffect(() => {
    if (selectedIndex >= suggestions.length) setSelectedIndex(-1);
  }, [selectedIndex, suggestions.length]);

  useEffect(() => {
    if (autoFocus) resolvedInputRef.current?.focus();
  }, [autoFocus, resolvedInputRef]);

  function submitSuggestion(index: number) {
    const suggestion = suggestions[index];
    if (!suggestion) return;
    const value = suggestionLabel(suggestion, controller.query);
    controller.setQuery(value);
    void controller.submit(value);
  }

  return (
    <div className="photo-search">
      <SearchInput
        activeDescendant={selectedIndex >= 0 ? `${idPrefix}-option-${selectedIndex}` : undefined}
        expanded={hasQuery && suggestions.length > 0}
        inputRef={resolvedInputRef}
        listboxId={`${idPrefix}-listbox`}
        value={controller.query}
        onChange={controller.setQuery}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing) return;
          if (suggestions.length > 0 && event.key === "ArrowDown") {
            event.preventDefault();
            setSelectedIndex((current) => moveSuggestionSelection(current, suggestions.length, 1));
            return;
          }
          if (suggestions.length > 0 && event.key === "ArrowUp") {
            event.preventDefault();
            setSelectedIndex((current) => moveSuggestionSelection(current, suggestions.length, -1));
            return;
          }
          if (event.key === "ArrowRight" && selectedIndex >= 0) {
            event.preventDefault();
            const suggestion = suggestions[selectedIndex];
            if (suggestion) controller.setQuery(suggestionLabel(suggestion, controller.query));
            return;
          }
          if (event.key === "Enter") {
            event.preventDefault();
            if (selectedIndex >= 0) submitSuggestion(selectedIndex);
            else void controller.submit();
          }
        }}
      />
      {hasQuery && (
        <SearchSuggestions
          idPrefix={idPrefix}
          suggestions={suggestions}
          selectedIndex={selectedIndex}
          onHover={setSelectedIndex}
          onSelect={(suggestion) => {
            const value = suggestionLabel(suggestion, controller.query);
            controller.setQuery(value);
            void controller.submit(value);
          }}
        />
      )}
      {controller.error && <div className="photo-search-error" role="alert">{controller.error}</div>}
    </div>
  );
}
