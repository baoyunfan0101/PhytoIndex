import {
  ChevronDown,
  ChevronRight,
  CircleAlert,
  LoaderCircle,
  X,
  type LucideIcon,
} from "lucide-react";
import {
  useEffect,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type CSSProperties,
  type KeyboardEvent,
  type ReactNode,
  type UIEvent,
} from "react";
import { useViewState } from "./viewState";

export type IconComponent = LucideIcon;

export type ButtonVariant = "primary" | "secondary" | "ghost";
export type ButtonSize = "default" | "small";

export function Button({
  className = "",
  size = "default",
  type = "button",
  variant = "secondary",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  size?: ButtonSize;
  variant?: ButtonVariant;
}) {
  const classes = [
    "button",
    `button-${variant}`,
    size === "small" ? "button-small" : "",
    className,
  ].filter(Boolean).join(" ");
  return <button {...props} className={classes} type={type} />;
}

export function IconButton({
  "aria-label": ariaLabel,
  className = "",
  size = "default",
  type = "button",
  variant = "ghost",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  "aria-label": string;
  size?: ButtonSize;
  variant?: ButtonVariant;
}) {
  const classes = ["icon-button", size === "small" ? "icon-button-small" : "", className]
    .filter(Boolean)
    .join(" ");
  return (
    <Button
      {...props}
      aria-label={ariaLabel}
      className={classes}
      size={size}
      type={type}
      variant={variant}
    />
  );
}

export function VirtualList<T>({
  items,
  rowHeight = 42,
  className = "",
  overscan = 8,
  itemKey,
  renderItem,
  onNearEnd,
  onTypeSelect,
  stateKey,
}: {
  items: T[];
  rowHeight?: number;
  className?: string;
  overscan?: number;
  itemKey: (item: T, index: number) => string | number;
  renderItem: (item: T, index: number) => ReactNode;
  onNearEnd?: () => void;
  onTypeSelect?: (query: string) => void;
  stateKey?: string;
}) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const typeBuffer = useRef("");
  const typeTimer = useRef<number | null>(null);
  const [height, setHeight] = useState(0);
  const [scrollTop, setScrollTop] = useViewState(
    stateKey ? `${stateKey}.scroll-top` : null,
    0,
  );

  useEffect(() => {
    const element = viewportRef.current;
    if (!element) return;
    element.scrollTop = scrollTop;
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
  stateKey,
}: {
  items: T[];
  minColumnWidth?: number;
  rowHeight?: number;
  itemKey: (item: T) => string | number;
  renderItem: (item: T, index: number) => ReactNode;
  onNearEnd?: () => void;
  stateKey?: string;
}) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const [scrollTop, setScrollTop] = useViewState(
    stateKey ? `${stateKey}.scroll-top` : null,
    0,
  );

  useEffect(() => {
    const element = viewportRef.current;
    if (!element) return;
    element.scrollTop = scrollTop;
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
          <IconButton onClick={onClose} aria-label="Close"><X size={15} /></IconButton>
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
        <Button className={item === value ? "active" : ""} variant="ghost" key={item} onClick={() => onChange(item)}>
          {item}
        </Button>
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
      <Button variant="ghost" onClick={onToggle}>
        {open ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
        {label}
      </Button>
      {open && <div className="disclosure-children">{children}</div>}
    </div>
  );
}

export function Busy({ label = "Loading" }: { label?: string }) {
  return <div className="busy"><LoaderCircle className="spin" size={15} /><span>{label}</span></div>;
}
