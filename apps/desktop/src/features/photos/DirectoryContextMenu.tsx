import { ArrowRightLeft, FolderOpen, RefreshCw, type LucideIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  openPhotoDirectoryInFileManager,
  renamePhotosInDirectoryFromTaxa,
  type PhotoDirectory,
  type PhotoRenameOperationResult,
} from "../../api/photos";
import { errorMessage } from "../../api/common";

export function DirectoryContextMenu({
  directory,
  x,
  y,
  onClose,
  onRefresh,
  onRenamed,
  onStatus,
}: {
  directory: PhotoDirectory;
  x: number;
  y: number;
  onClose: () => void;
  onRefresh: (directory: PhotoDirectory) => Promise<void>;
  onRenamed: (result: PhotoRenameOperationResult) => Promise<void> | void;
  onStatus: (message: string, busy?: boolean) => void;
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

  async function run(label: string, action: () => Promise<void>) {
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

  async function renameDirectoryPhotos() {
    onStatus("Renaming directory photos", true);
    const result = await renamePhotosInDirectoryFromTaxa(directory.directory_id, true);
    onStatus(summarizeRenameResult(result));
    await onRenamed(result);
  }

  return (
    <div
      className="context-menu"
      ref={menuRef}
      role="menu"
      style={{ left: Math.min(x, window.innerWidth - 270), top: Math.min(y, window.innerHeight - 160) }}
    >
      <MenuButton
        icon={RefreshCw}
        label="Refresh"
        disabled={Boolean(busy)}
        onClick={() => void run("Refreshing", () => onRefresh(directory))}
      />
      <MenuButton
        icon={ArrowRightLeft}
        label="Rename from taxonomy recursively"
        disabled={Boolean(busy)}
        onClick={() => void run("Renaming", renameDirectoryPhotos)}
      />
      <div className="context-separator" role="separator" />
      <MenuButton
        icon={FolderOpen}
        label="Open in Finder / Explorer"
        disabled={Boolean(busy)}
        onClick={() => void run("Opening", () => openPhotoDirectoryInFileManager(directory.directory_id))}
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

function summarizeRenameResult(result: PhotoRenameOperationResult) {
  if (result.rows.length === 0) return "No matched photos to rename";
  const applied = result.rows.filter((row) => row.status === "applied").length;
  const noChange = result.rows.filter((row) => row.status === "no_change").length;
  const failed = result.rows.filter((row) => row.status === "failed").length;
  return [
    applied > 0 ? formatCount(applied, "renamed") : "",
    noChange > 0 ? formatCount(noChange, "unchanged") : "",
    failed > 0 ? formatCount(failed, "failed") : "",
  ].filter(Boolean).join(", ");
}

function formatCount(count: number, label: string) {
  return `${count} ${count === 1 ? "photo" : "photos"} ${label}`;
}
