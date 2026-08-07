import { ChevronDown, Tags, type LucideIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { PhotoTaxonUsage } from "../../api/mapping";
import { errorMessage } from "../../api/common";

export function TaxonContextMenu({
  taxon,
  x,
  y,
  onClose,
  onExpandAll,
  onOpenTaxonomy,
}: {
  taxon: PhotoTaxonUsage;
  x: number;
  y: number;
  onClose: () => void;
  onExpandAll?: (taxon: PhotoTaxonUsage) => Promise<void> | void;
  onOpenTaxonomy: (taxon: PhotoTaxonUsage) => void;
}) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    const close = (event: MouseEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) onClose();
    };
    const closeKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", closeKey);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", closeKey);
    };
  }, [onClose]);

  async function run(label: string, action: () => Promise<void> | void) {
    setBusy(label);
    setError("");
    try {
      await action();
      onClose();
    } catch (nextError) {
      setError(errorMessage(nextError));
      setBusy("");
    }
  }

  return (
    <div
      className="context-menu"
      ref={menuRef}
      role="menu"
      style={{ left: Math.min(x, window.innerWidth - 250), top: Math.min(y, window.innerHeight - 130) }}
    >
      {onExpandAll && (
        <>
          <MenuButton
            icon={ChevronDown}
            label="Expand all subtrees"
            disabled={Boolean(busy)}
            onClick={() => void run("Expanding", () => onExpandAll(taxon))}
          />
          <div className="context-separator" role="separator" />
        </>
      )}
      <MenuButton
        icon={Tags}
        label="Go to taxonomy"
        disabled={Boolean(busy)}
        onClick={() => {
          onOpenTaxonomy(taxon);
          onClose();
        }}
      />
      {error && <div className="context-error">{error}</div>}
    </div>
  );
}

function MenuButton({
  icon: Icon,
  label,
  disabled,
  onClick,
}: {
  icon: LucideIcon;
  label: string;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button type="button" role="menuitem" disabled={disabled} onClick={onClick}>
      <Icon size={14} />
      <span>{label}</span>
    </button>
  );
}
