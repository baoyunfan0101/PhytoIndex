import type { ReactNode } from "react";
import { Camera, LoaderCircle, type LucideIcon } from "lucide-react";
import { photoUrl, type Photo } from "./api";

export type IconComponent = LucideIcon;

export function Tabs<T extends string>({
  items,
  value,
  onChange,
  icons,
}: {
  items: readonly T[];
  value: T;
  onChange: (value: T) => void;
  icons?: Record<string, IconComponent>;
}) {
  return (
    <div className={icons ? "mode-tabs" : "flat-tabs"}>
      {items.map((item) => {
        const Icon = icons?.[item];
        return (
          <button
            className={`${icons ? "mode-tab" : "tab-button"}${value === item ? " active" : ""}`}
            key={item}
            type="button"
            onClick={() => onChange(item)}
          >
            {Icon && <Icon size={14} />}
            <span>{item}</span>
          </button>
        );
      })}
    </div>
  );
}

export function PanelTitle({
  children,
  trailing,
}: {
  children: ReactNode;
  trailing?: ReactNode;
}) {
  return (
    <div className="panel-title">
      <span>{children}</span>
      {trailing}
    </div>
  );
}

export function EmptyState({
  icon: Icon = Camera,
  title,
  detail,
  action,
}: {
  icon?: IconComponent;
  title: string;
  detail?: string;
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

export function BusyState({ label }: { label: string }) {
  return (
    <div className="busy-state">
      <LoaderCircle size={15} />
      <span>{label}</span>
    </div>
  );
}

export function PhotoPreview({ photo }: { photo: Photo | null }) {
  if (!photo) {
    return <EmptyState title="No photo selected" detail="Select a photo to inspect it." />;
  }
  return (
    <div className="real-photo-preview">
      <div className="real-photo-stage">
        <img src={photoUrl(photo, true)} alt={photo.filename} />
      </div>
      <dl className="compact-details">
        <dt>Name</dt>
        <dd>{photo.filename}</dd>
        <dt>Path</dt>
        <dd>{photo.relative_path}</dd>
        <dt>Size</dt>
        <dd>{formatBytes(photo.file_size)}</dd>
      </dl>
    </div>
  );
}

export function formatBytes(value: number): string {
  if (value < 1024) {
    return `${value} B`;
  }
  if (value < 1024 * 1024) {
    return `${(value / 1024).toFixed(1)} KB`;
  }
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
