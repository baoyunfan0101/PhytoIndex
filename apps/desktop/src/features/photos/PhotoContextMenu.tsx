import {
  Database,
  FileInput,
  FilePenLine,
  FolderOpen,
  Info,
  Link,
  Link2,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  renamePhoto,
  renamePhotoFromTaxon,
  revealPhotoInFileManager,
  type Photo,
} from "../../api/photos";
import { errorMessage } from "../../api/common";
import { remapPhoto, type PhotoMappingSummary } from "../../api/mapping";
import { Button, Modal } from "../../shared/ui";
import { MappingBadge } from "../mapping/MappingBadge";

export function PhotoContextMenu({
  photo,
  mapping,
  loading,
  x,
  y,
  onClose,
  onChanged,
  onMappingChanged,
  onOpenDetails,
  onOpenTaxon,
  onOpenMappingEditor,
}: {
  photo: Photo;
  mapping: PhotoMappingSummary | null;
  loading: boolean;
  x: number;
  y: number;
  onClose: () => void;
  onChanged: (photo: Photo) => void;
  onMappingChanged: () => void;
  onOpenDetails: () => void;
  onOpenTaxon: (taxonId: number) => void;
  onOpenMappingEditor: () => void;
}) {
  const [renaming, setRenaming] = useState(false);
  const [newFilename, setNewFilename] = useState(photo.filename);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const menuRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (renaming) return;
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
  }, [onClose, renaming]);

  useEffect(() => {
    if (!renaming || !inputRef.current) return;
    const dot = newFilename.lastIndexOf(".");
    inputRef.current.focus();
    inputRef.current.setSelectionRange(0, dot > 0 ? dot : newFilename.length);
  }, [renaming]);

  const matched = mapping?.status === "matched" && mapping.taxon_id !== null;

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

  return (
    <>
      <div
        className="context-menu"
        ref={menuRef}
        style={{ left: Math.min(x, window.innerWidth - 260), top: Math.min(y, window.innerHeight - 390) }}
        role="menu"
      >
        <div className="context-state">
          <span>Mapping state</span>
          {loading ? <span className="context-loading">Loading</span> : mapping ? <MappingBadge status={mapping.status} /> : <span>Unavailable</span>}
        </div>
        <MenuSeparator />
        <MenuButton icon={Info} label="Photo details" onClick={onOpenDetails} />
        <MenuButton icon={Database} label="Go to taxonomy" disabled={!matched} onClick={() => matched && onOpenTaxon(mapping.taxon_id!)} />
        <MenuSeparator />
        <MenuButton icon={FilePenLine} label="Rename" onClick={() => setRenaming(true)} />
        <MenuButton
          icon={FileInput}
          label="Rename from taxonomy"
          disabled={!matched || Boolean(busy)}
          onClick={() => void run("Renaming", async () => onChanged(await renamePhotoFromTaxon(photo.photo_id)))}
        />
        <MenuSeparator />
        <MenuButton icon={Link} label="Edit mapping" onClick={onOpenMappingEditor} />
        <MenuButton
          icon={Link2}
          label="Remap from filename"
          disabled={Boolean(busy)}
          onClick={() => void run("Remapping", async () => {
            await remapPhoto(photo.photo_id);
            onMappingChanged();
          })}
        />
        <MenuSeparator />
        <MenuButton
          icon={FolderOpen}
          label="Reveal in Finder / Explorer"
          disabled={Boolean(busy)}
          onClick={() => void run("Revealing", () => revealPhotoInFileManager(photo.photo_id))}
        />
        {error && <div className="context-error">{error}</div>}
      </div>

      {renaming && (
        <Modal
          title="Rename photo"
          onClose={() => setRenaming(false)}
          actions={
            <>
              <Button onClick={() => setRenaming(false)}>Cancel</Button>
              <Button
                variant="primary"
                disabled={!newFilename.trim() || Boolean(busy)}
                onClick={() => void run("Renaming", async () => onChanged(await renamePhoto(photo.photo_id, newFilename.trim())))}
              >
                Rename
              </Button>
            </>
          }
        >
          <label className="field-stack">
            <span>Filename</span>
            <input ref={inputRef} value={newFilename} onChange={(event) => setNewFilename(event.target.value)} />
          </label>
          <span className="field-hint">The extension is part of the filename.</span>
          {error && <div className="inline-error">{error}</div>}
        </Modal>
      )}
    </>
  );
}

function MenuButton({
  icon: Icon,
  label,
  disabled,
  onClick,
}: {
  icon: typeof Info;
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

function MenuSeparator() {
  return <div className="context-separator" role="separator" />;
}
