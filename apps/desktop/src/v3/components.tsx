import {
  Camera,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  LoaderCircle,
  X,
  type LucideIcon,
} from "lucide-react";
import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type ReactNode,
  type UIEvent,
} from "react";
import {
  displayTaxon,
  photoUrl,
  type Photo,
  type PhotoTaxonStatus,
  type TaxonSummary,
} from "./api";

export type IconComponent = LucideIcon;

export function VirtualList<T>({
  items,
  rowHeight = 42,
  className = "",
  overscan = 8,
  itemKey,
  renderItem,
  onNearEnd,
  onTypeSelect,
}: {
  items: T[];
  rowHeight?: number;
  className?: string;
  overscan?: number;
  itemKey: (item: T, index: number) => string | number;
  renderItem: (item: T, index: number) => ReactNode;
  onNearEnd?: () => void;
  onTypeSelect?: (query: string) => void;
}) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const typeBuffer = useRef("");
  const typeTimer = useRef<number | null>(null);
  const [height, setHeight] = useState(0);
  const [scrollTop, setScrollTop] = useState(0);

  useEffect(() => {
    const element = viewportRef.current;
    if (!element) return;
    const update = () => setHeight(element.clientHeight);
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const start = Math.max(0, Math.floor(scrollTop / rowHeight) - overscan);
  const end = Math.min(items.length, Math.ceil((scrollTop + height) / rowHeight) + overscan);
  const visible = items.slice(start, end);

  function handleScroll(event: UIEvent<HTMLDivElement>) {
    const element = event.currentTarget;
    setScrollTop(element.scrollTop);
    if (element.scrollHeight - element.scrollTop - element.clientHeight < rowHeight * 5) {
      onNearEnd?.();
    }
  }

  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (!onTypeSelect || event.metaKey || event.ctrlKey || event.altKey || event.key.length !== 1) {
      return;
    }
    typeBuffer.current += event.key.toLocaleLowerCase();
    onTypeSelect(typeBuffer.current);
    if (typeTimer.current !== null) window.clearTimeout(typeTimer.current);
    typeTimer.current = window.setTimeout(() => {
      typeBuffer.current = "";
      typeTimer.current = null;
    }, 700);
  }

  return (
    <div
      className={`virtual-viewport ${className}`}
      ref={viewportRef}
      onScroll={handleScroll}
      onKeyDown={handleKeyDown}
      tabIndex={0}
    >
      <div className="virtual-spacer" style={{ height: items.length * rowHeight }}>
        <div className="virtual-window" style={{ transform: `translateY(${start * rowHeight}px)` }}>
          {visible.map((item, offset) => (
            <div className="virtual-row" style={{ height: rowHeight }} key={itemKey(item, start + offset)}>
              {renderItem(item, start + offset)}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

export function VirtualGrid<T>({
  items,
  minColumnWidth = 156,
  rowHeight = 146,
  itemKey,
  renderItem,
  onNearEnd,
}: {
  items: T[];
  minColumnWidth?: number;
  rowHeight?: number;
  itemKey: (item: T) => string | number;
  renderItem: (item: T, index: number) => ReactNode;
  onNearEnd?: () => void;
}) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const [scrollTop, setScrollTop] = useState(0);

  useEffect(() => {
    const element = viewportRef.current;
    if (!element) return;
    const update = () => setSize({ width: element.clientWidth, height: element.clientHeight });
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const columns = Math.max(1, Math.floor(Math.max(size.width - 16, minColumnWidth) / minColumnWidth));
  const rows = Math.ceil(items.length / columns);
  const startRow = Math.max(0, Math.floor(scrollTop / rowHeight) - 3);
  const endRow = Math.min(rows, Math.ceil((scrollTop + size.height) / rowHeight) + 3);
  const indexes: number[] = [];
  for (let row = startRow; row < endRow; row += 1) {
    for (let column = 0; column < columns; column += 1) {
      const index = row * columns + column;
      if (index < items.length) indexes.push(index);
    }
  }

  return (
    <div
      className="virtual-grid-viewport"
      ref={viewportRef}
      onScroll={(event) => {
        const element = event.currentTarget;
        setScrollTop(element.scrollTop);
        if (element.scrollHeight - element.scrollTop - element.clientHeight < rowHeight * 3) onNearEnd?.();
      }}
    >
      <div className="virtual-grid-spacer" style={{ height: rows * rowHeight }}>
        <div
          className="virtual-grid-window"
          style={{
            gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
            transform: `translateY(${startRow * rowHeight}px)`,
          }}
        >
          {indexes.map((index) => (
            <div className="virtual-grid-cell" style={{ height: rowHeight }} key={itemKey(items[index])}>
              {renderItem(items[index], index)}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

export function PhotoStage({
  photo,
  compact = false,
  onContextMenu,
}: {
  photo: Photo | null;
  compact?: boolean;
  onContextMenu?: (event: React.MouseEvent, photo: Photo) => void;
}) {
  const [loaded, setLoaded] = useState(false);
  useEffect(() => setLoaded(false), [photo?.photo_id]);

  if (!photo) {
    return (
      <div className="photo-stage empty">
        <Camera size={28} />
        <span>Select a photo</span>
      </div>
    );
  }
  return (
    <div
      className={`photo-stage${compact ? " compact" : ""}`}
      onContextMenu={(event) => {
        event.preventDefault();
        onContextMenu?.(event, photo);
      }}
    >
      {!loaded && <LoaderCircle className="spin photo-loader" size={20} />}
      <img
        src={photoUrl(photo)}
        alt={photo.filename}
        draggable={false}
        onLoad={() => setLoaded(true)}
      />
      <div className="photo-stage-caption">{photo.filename}</div>
    </div>
  );
}

export function PhotoThumb({
  photo,
  selected,
  onClick,
  onContextMenu,
}: {
  photo: Photo;
  selected: boolean;
  onClick: () => void;
  onContextMenu?: (event: React.MouseEvent) => void;
}) {
  return (
    <button
      className={`photo-thumb${selected ? " selected" : ""}`}
      type="button"
      onClick={onClick}
      onContextMenu={(event) => {
        event.preventDefault();
        onContextMenu?.(event);
      }}
    >
      <img src={photoUrl(photo, true)} alt="" loading="lazy" draggable={false} />
      <span>{photo.filename}</span>
    </button>
  );
}

export function MappingBadge({ status }: { status: PhotoTaxonStatus }) {
  return <span className={`mapping-badge ${status}`}>{status}</span>;
}

export function TaxonCard({
  taxon,
  compact = false,
  active = false,
  actions,
  onClick,
}: {
  taxon: TaxonSummary;
  compact?: boolean;
  active?: boolean;
  actions?: ReactNode;
  onClick?: () => void;
}) {
  return (
    <article className={`taxon-card${compact ? " compact" : ""}${active ? " active" : ""}`}>
      <button className="taxon-card-main" type="button" onClick={onClick}>
        <span className="taxon-rank">{taxon.rank}</span>
        <strong>{displayTaxon(taxon)}</strong>
        <span>{taxon.names.zh_name ?? taxon.names.en_name ?? "No common name"}</span>
      </button>
      {actions && <div className="taxon-card-actions">{actions}</div>}
    </article>
  );
}

export function SectionHeader({
  title,
  detail,
  actions,
}: {
  title: string;
  detail?: string;
  actions?: ReactNode;
}) {
  return (
    <header className="section-header">
      <div>
        <strong>{title}</strong>
        {detail && <span>{detail}</span>}
      </div>
      {actions && <div className="section-actions">{actions}</div>}
    </header>
  );
}

export function EmptyState({
  title,
  detail,
  icon: Icon = CircleAlert,
  action,
}: {
  title: string;
  detail?: string;
  icon?: LucideIcon;
  action?: ReactNode;
}) {
  return (
    <div className="empty-state">
      <Icon size={24} strokeWidth={1.5} />
      <strong>{title}</strong>
      {detail && <span>{detail}</span>}
      {action}
    </div>
  );
}

export function Modal({
  title,
  children,
  actions,
  onClose,
  width = 520,
}: {
  title: string;
  children: ReactNode;
  actions?: ReactNode;
  onClose: () => void;
  width?: number;
}) {
  useEffect(() => {
    const close = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [onClose]);

  return (
    <div className="modal-backdrop" onMouseDown={onClose}>
      <section className="modal-card" style={{ "--modal-width": `${width}px` } as CSSProperties} onMouseDown={(event) => event.stopPropagation()}>
        <header>
          <strong>{title}</strong>
          <button className="icon-button" type="button" onClick={onClose} aria-label="Close"><X size={15} /></button>
        </header>
        <div className="modal-body">{children}</div>
        {actions && <footer>{actions}</footer>}
      </section>
    </div>
  );
}

export function Segmented<T extends string>({
  value,
  items,
  onChange,
}: {
  value: T;
  items: readonly T[];
  onChange: (value: T) => void;
}) {
  return (
    <div className="segmented">
      {items.map((item) => (
        <button className={item === value ? "active" : ""} type="button" key={item} onClick={() => onChange(item)}>
          {item}
        </button>
      ))}
    </div>
  );
}

export function Disclosure({
  label,
  open,
  onToggle,
  children,
}: {
  label: ReactNode;
  open: boolean;
  onToggle: () => void;
  children?: ReactNode;
}) {
  return (
    <div className="disclosure">
      <button type="button" onClick={onToggle}>
        {open ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
        {label}
      </button>
      {open && <div className="disclosure-children">{children}</div>}
    </div>
  );
}

export function Busy({ label = "Loading" }: { label?: string }) {
  return <div className="busy"><LoaderCircle className="spin" size={15} /><span>{label}</span></div>;
}

export function useStableSelection<T>(
  items: T[],
  getId: (item: T) => number,
): [T | null, (item: T | null) => void] {
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const selected = useMemo(
    () => items.find((item) => getId(item) === selectedId) ?? items[0] ?? null,
    [getId, items, selectedId],
  );
  return [selected, (item) => setSelectedId(item ? getId(item) : null)];
}
