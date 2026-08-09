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
  const searchRootRef = useRef<HTMLDivElement>(null);
  const resolvedInputRef = inputRef ?? localInputRef;
  const [selectedIndex, setSelectedIndex] = useState(-1);
  const [suggestionsOpen, setSuggestionsOpen] = useState(false);
  const { loading, suggestions } = useSearchSuggestions(controller.query, enabled);
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

  useEffect(() => {
    const closeSuggestions = (event: PointerEvent) => {
      const target = event.target;
      if (target instanceof Node && !searchRootRef.current?.contains(target)) {
        setSuggestionsOpen(false);
        setSelectedIndex(-1);
      }
    };
    document.addEventListener("pointerdown", closeSuggestions);
    return () => document.removeEventListener("pointerdown", closeSuggestions);
  }, []);

  function submitSuggestion(index: number) {
    const suggestion = suggestions[index];
    if (!suggestion) return;
    const value = suggestionLabel(suggestion, controller.query);
    controller.setQuery(value);
    setSuggestionsOpen(false);
    void controller.submit(value);
  }

  return (
    <div
      className="photo-search"
      ref={searchRootRef}
      onFocusCapture={() => {
        if (hasQuery) setSuggestionsOpen(true);
      }}
    >
      <SearchInput
        activeDescendant={selectedIndex >= 0 ? `${idPrefix}-option-${selectedIndex}` : undefined}
        expanded={suggestionsOpen && hasQuery && (loading || suggestions.length > 0)}
        inputRef={resolvedInputRef}
        listboxId={`${idPrefix}-listbox`}
        value={controller.query}
        onChange={(value) => {
          controller.setQuery(value);
          setSuggestionsOpen(true);
        }}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing) return;
          if (suggestions.length > 0 && event.key === "ArrowDown") {
            event.preventDefault();
            setSuggestionsOpen(true);
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
            else {
              setSuggestionsOpen(false);
              void controller.submit();
            }
            return;
          }
          if (event.key === "Escape") {
            setSuggestionsOpen(false);
            setSelectedIndex(-1);
          }
        }}
      />
      {suggestionsOpen && hasQuery && (
        <SearchSuggestions
          idPrefix={idPrefix}
          loading={loading}
          suggestions={suggestions}
          selectedIndex={selectedIndex}
          onHover={setSelectedIndex}
          onSelect={(suggestion) => {
            const value = suggestionLabel(suggestion, controller.query);
            controller.setQuery(value);
            setSuggestionsOpen(false);
            void controller.submit(value);
          }}
        />
      )}
      {controller.error && <div className="photo-search-error" role="alert">{controller.error}</div>}
    </div>
  );
}
