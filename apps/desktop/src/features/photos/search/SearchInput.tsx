import { Search } from "lucide-react";
import type { KeyboardEventHandler, RefObject } from "react";

export function SearchInput({
  activeDescendant,
  expanded,
  inputRef,
  listboxId,
  onChange,
  onKeyDown,
  value,
}: {
  activeDescendant?: string;
  expanded: boolean;
  inputRef: RefObject<HTMLInputElement>;
  listboxId: string;
  onChange: (value: string) => void;
  onKeyDown: KeyboardEventHandler<HTMLInputElement>;
  value: string;
}) {
  return (
    <label className="photo-search-input">
      <Search size={16} />
      <input
        ref={inputRef}
        role="combobox"
        aria-autocomplete="list"
        aria-controls={listboxId}
        aria-expanded={expanded}
        aria-activedescendant={activeDescendant}
        autoComplete="off"
        spellCheck={false}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={onKeyDown}
        placeholder="Search filenames and photo taxonomy"
      />
    </label>
  );
}
