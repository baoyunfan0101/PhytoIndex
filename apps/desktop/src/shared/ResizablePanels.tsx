import {
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
} from "react";
import { clampPanelSize } from "./panelSizing";

export type PanelDirection = "horizontal" | "vertical";

type AxisValue = number | Partial<Record<PanelDirection, number>>;

export function ResizablePanels({
  className = "",
  direction = "horizontal",
  first,
  initialRatio,
  initialSize,
  minFirst = 120,
  minSecond = 120,
  responsiveBreakpoint,
  second,
  separatorLabel,
}: {
  className?: string;
  direction?: PanelDirection;
  first: ReactNode;
  initialRatio?: AxisValue;
  initialSize?: AxisValue;
  minFirst?: AxisValue;
  minSecond?: AxisValue;
  responsiveBreakpoint?: number;
  second: ReactNode;
  separatorLabel: string;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ pointerId: number; start: number; size: number } | null>(null);
  const [containerSize, setContainerSize] = useState({ width: 0, height: 0 });
  const [dragging, setDragging] = useState(false);
  const [sizes, setSizes] = useState<Partial<Record<PanelDirection, number>>>({});
  const resolvedDirection = responsiveBreakpoint !== undefined && containerSize.width > 0
    ? containerSize.width < responsiveBreakpoint ? "vertical" : "horizontal"
    : direction;
  const availableSize = resolvedDirection === "horizontal" ? containerSize.width : containerSize.height;
  const minimumFirst = axisValue(minFirst, resolvedDirection, 120);
  const minimumSecond = axisValue(minSecond, resolvedDirection, 120);
  const configuredSize = axisValue(initialSize, resolvedDirection, Number.NaN);
  const requestedSize = sizes[resolvedDirection]
    ?? (Number.isFinite(configuredSize) ? configuredSize : availableSize * axisValue(initialRatio, resolvedDirection, 0.5));
  const firstSize = clampPanelSize(requestedSize, availableSize, minimumFirst, minimumSecond);

  useEffect(() => {
    const element = rootRef.current;
    if (!element) return;
    const update = () => setContainerSize({ width: element.clientWidth, height: element.clientHeight });
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  useEffect(() => () => {
    document.body.style.removeProperty("cursor");
    document.body.style.removeProperty("user-select");
  }, []);

  const style = (resolvedDirection === "horizontal"
    ? { gridTemplateColumns: `${firstSize}px 7px minmax(0, 1fr)` }
    : { gridTemplateRows: `${firstSize}px 7px minmax(0, 1fr)` }) as CSSProperties;

  function updateSize(next: number) {
    setSizes((current) => ({
      ...current,
      [resolvedDirection]: clampPanelSize(next, availableSize, minimumFirst, minimumSecond),
    }));
  }

  function beginDrag(event: PointerEvent<HTMLDivElement>) {
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    dragRef.current = {
      pointerId: event.pointerId,
      start: resolvedDirection === "horizontal" ? event.clientX : event.clientY,
      size: firstSize,
    };
    setDragging(true);
    document.body.style.setProperty("cursor", resolvedDirection === "horizontal" ? "col-resize" : "row-resize");
    document.body.style.setProperty("user-select", "none");
  }

  function moveDrag(event: PointerEvent<HTMLDivElement>) {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    const position = resolvedDirection === "horizontal" ? event.clientX : event.clientY;
    updateSize(drag.size + position - drag.start);
  }

  function endDrag(event: PointerEvent<HTMLDivElement>) {
    if (dragRef.current?.pointerId !== event.pointerId) return;
    dragRef.current = null;
    setDragging(false);
    document.body.style.removeProperty("cursor");
    document.body.style.removeProperty("user-select");
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  }

  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const backward = resolvedDirection === "horizontal" ? "ArrowLeft" : "ArrowUp";
    const forward = resolvedDirection === "horizontal" ? "ArrowRight" : "ArrowDown";
    if (event.key === backward || event.key === forward) {
      event.preventDefault();
      updateSize(firstSize + (event.key === forward ? 12 : -12));
    } else if (event.key === "Home") {
      event.preventDefault();
      updateSize(minimumFirst);
    } else if (event.key === "End") {
      event.preventDefault();
      updateSize(availableSize - minimumSecond - 7);
    }
  }

  return (
    <div
      className={`resizable-panels resizable-panels-${resolvedDirection}${dragging ? " dragging" : ""}${className ? ` ${className}` : ""}`}
      ref={rootRef}
      style={style}
    >
      <div className="resizable-panel resizable-panel-first">{first}</div>
      <div
        aria-label={separatorLabel}
        aria-orientation={resolvedDirection}
        aria-valuemax={Math.max(minimumFirst, availableSize - minimumSecond - 7)}
        aria-valuemin={minimumFirst}
        aria-valuenow={Math.round(firstSize)}
        className="resizable-separator"
        onKeyDown={handleKeyDown}
        onPointerCancel={endDrag}
        onPointerDown={beginDrag}
        onPointerMove={moveDrag}
        onPointerUp={endDrag}
        role="separator"
        tabIndex={0}
        title={`${separatorLabel}. Drag or use arrow keys to resize.`}
      />
      <div className="resizable-panel resizable-panel-second">{second}</div>
    </div>
  );
}

function axisValue(value: AxisValue | undefined, direction: PanelDirection, fallback: number): number {
  if (typeof value === "number") return value;
  return value?.[direction] ?? fallback;
}
