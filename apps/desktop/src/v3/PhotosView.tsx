import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ChevronRight,
  Folder,
  FolderOpen,
  History,
  Image,
  Map,
  Play,
  RefreshCw,
  TreeDeciduous,
} from "lucide-react";
import {
  browsePhotoDirectory,
  getPhotoDirectoryCounts,
  getPhotoLibrary,
  listPhotoOperations,
  openPhotoLibrary,
  refreshPhotoDirectory,
  selectPhotoDirectory,
  waitForOperation,
  type DirectoryEntryCounts,
  type Photo,
  type PhotoDirectory,
  type PhotoLibrary,
  type PhotoOperation,
} from "./api";
import {
  BusyState,
  EmptyState,
  PanelTitle,
  PhotoPreview,
  Tabs,
  errorMessage,
  formatBytes,
} from "./components";

type PhotoMode = "Folders" | "Taxa" | "Map" | "History";
type Location = Pick<PhotoDirectory, "directory_id" | "name" | "relative_path">;

const photoModes = ["Folders", "Taxa", "Map", "History"] as const;
const photoModeIcons = {
  Folders: Folder,
  Taxa: TreeDeciduous,
  Map,
  History,
};

export function PhotosView({
  onStatus,
}: {
  onStatus: (message: string, busy?: boolean) => void;
}) {
  const [mode, setMode] = useState<PhotoMode>("Folders");
  const [library, setLibrary] = useState<PhotoLibrary | null>(null);
  const [draftRoot, setDraftRoot] = useState("");
  const [trail, setTrail] = useState<Location[]>([]);
  const [directories, setDirectories] = useState<PhotoDirectory[]>([]);
  const [photos, setPhotos] = useState<Photo[]>([]);
  const [counts, setCounts] = useState<DirectoryEntryCounts | null>(null);
  const [selectedPhoto, setSelectedPhoto] = useState<Photo | null>(null);
  const [operations, setOperations] = useState<PhotoOperation[]>([]);
  const [selectedOperation, setSelectedOperation] = useState<PhotoOperation | null>(null);
  const [loading, setLoading] = useState(true);
  const [progress, setProgress] = useState("");
  const [error, setError] = useState("");

  const currentDirectoryId =
    trail[trail.length - 1]?.directory_id ??
    library?.root_directory_id ??
    null;

  const loadDirectory = useCallback(async (directoryId: number) => {
    setLoading(true);
    setError("");
    try {
      const [page, nextCounts] = await Promise.all([
        browsePhotoDirectory(directoryId),
        getPhotoDirectoryCounts(directoryId),
      ]);
      setDirectories(
        page.items
          .filter((item) => item.kind === "directory")
          .map((item) => item.directory),
      );
      setPhotos(
        page.items
          .filter((item) => item.kind === "photo")
          .map((item) => item.photo),
      );
      setCounts(nextCounts);
      setSelectedPhoto(null);
      onStatus(
        `${nextCounts.directory_count} folders, ${nextCounts.file_count} photos`,
      );
    } catch (nextError) {
      const message = errorMessage(nextError);
      setError(message);
      onStatus(message);
    } finally {
      setLoading(false);
    }
  }, [onStatus]);

  useEffect(() => {
    let active = true;
    getPhotoLibrary()
      .then((nextLibrary) => {
        if (!active) {
          return;
        }
        setLibrary(nextLibrary);
        setDraftRoot(nextLibrary?.root_path ?? "");
        if (nextLibrary) {
          return loadDirectory(nextLibrary.root_directory_id);
        }
        setLoading(false);
        onStatus("Open a photo folder to begin");
      })
      .catch((nextError) => {
        if (!active) {
          return;
        }
        const message = errorMessage(nextError);
        setError(message);
        setLoading(false);
        onStatus(message);
      });
    return () => {
      active = false;
    };
  }, [loadDirectory, onStatus]);

  useEffect(() => {
    if (mode !== "History") {
      return;
    }
    setLoading(true);
    listPhotoOperations()
      .then((page) => {
        setOperations(page.items);
        setSelectedOperation(page.items[0] ?? null);
        onStatus(`${page.items.length} rename operations`);
      })
      .catch((nextError) => {
        const message = errorMessage(nextError);
        setError(message);
        onStatus(message);
      })
      .finally(() => setLoading(false));
  }, [mode, onStatus]);

  async function browseRoot() {
    const selected = await selectPhotoDirectory();
    if (selected) {
      setDraftRoot(selected);
      await openRoot(selected);
    }
  }

  async function openRoot(root = draftRoot) {
    const value = root.trim();
    if (!value) {
      setError("Enter or browse to a photo folder.");
      return;
    }
    setLoading(true);
    setProgress("Opening folder");
    setError("");
    onStatus("Opening photo folder", true);
    try {
      const nextLibrary = await openPhotoLibrary(value);
      setLibrary(nextLibrary);
      setDraftRoot(nextLibrary.root_path);
      setTrail([]);
      await runRefresh(nextLibrary.root_directory_id);
    } catch (nextError) {
      const message = errorMessage(nextError);
      setError(message);
      onStatus(message);
    } finally {
      setProgress("");
      setLoading(false);
    }
  }

  async function refreshCurrent() {
    if (currentDirectoryId === null) {
      return;
    }
    setLoading(true);
    setError("");
    onStatus("Refreshing photos", true);
    try {
      await runRefresh(currentDirectoryId);
    } catch (nextError) {
      const message = errorMessage(nextError);
      setError(message);
      onStatus(message);
    } finally {
      setProgress("");
      setLoading(false);
    }
  }

  async function runRefresh(directoryId: number) {
    const started = await refreshPhotoDirectory(directoryId);
    await waitForOperation("photos", started.operation.task_id, (operation) => {
      const suffix = operation.total
        ? ` ${operation.processed}/${operation.total}`
        : "";
      setProgress(`${operation.message}${suffix}`);
      onStatus(operation.message, true);
    });
    await loadDirectory(directoryId);
  }

  function openDirectory(directory: PhotoDirectory) {
    const nextLocation: Location = {
      directory_id: directory.directory_id,
      name: directory.name,
      relative_path: directory.relative_path,
    };
    setTrail((current) => [...current, nextLocation]);
    void loadDirectory(directory.directory_id);
  }

  function openBreadcrumb(index: number) {
    const nextTrail = index < 0 ? [] : trail.slice(0, index + 1);
    const nextDirectoryId =
      nextTrail[nextTrail.length - 1]?.directory_id ??
      library?.root_directory_id;
    setTrail(nextTrail);
    if (nextDirectoryId !== undefined) {
      void loadDirectory(nextDirectoryId);
    }
  }

  const currentLabel = trail[trail.length - 1]?.name || "Root";
  const visibleCount = directories.length + photos.length;
  const totalCount = (counts?.directory_count ?? 0) + (counts?.file_count ?? 0);

  return (
    <section className="module-view">
      <div className="topbar photos-topbar">
        <form
          className="root-field"
          onSubmit={(event) => {
            event.preventDefault();
            void openRoot();
          }}
        >
          <span>Root</span>
          <input
            value={draftRoot}
            onChange={(event) => setDraftRoot(event.target.value)}
            placeholder="/path/to/photos"
            aria-label="Photo root"
          />
          <button type="submit">
            <Play size={12} />
            Open
          </button>
          <button type="button" onClick={() => void browseRoot()}>
            Browse
          </button>
        </form>
        <button
          className="ghost-button icon-action"
          type="button"
          onClick={() => void refreshCurrent()}
          disabled={!library || loading}
          title="Refresh current folder"
        >
          <RefreshCw size={13} />
          Refresh
        </button>
      </div>
      <Tabs
        items={photoModes}
        value={mode}
        onChange={setMode}
        icons={photoModeIcons}
      />

      {mode === "Folders" && (
        <div className="workspace-grid photos-grid">
          <aside className="panel sidebar-panel">
            <PanelTitle trailing={<span className="counter">{counts?.directory_count ?? 0}</span>}>
              Explorer
            </PanelTitle>
            {!library ? (
              <EmptyState
                icon={FolderOpen}
                title="No root"
                detail="Open a photo folder above."
              />
            ) : (
              <>
                <div className="breadcrumbs">
                  <button type="button" onClick={() => openBreadcrumb(-1)}>
                    {lastPathSegment(library.root_path)}
                  </button>
                  {trail.map((item, index) => (
                    <span key={item.directory_id}>
                      <ChevronRight size={11} />
                      <button type="button" onClick={() => openBreadcrumb(index)}>
                        {item.name}
                      </button>
                    </span>
                  ))}
                </div>
                <div className="tree-list">
                  {trail.length > 0 && (
                    <button className="tree-row" type="button" onClick={() => openBreadcrumb(trail.length - 2)}>
                      <FolderOpen size={14} />
                      <span>..</span>
                    </button>
                  )}
                  {directories.map((directory) => (
                    <button
                      className="tree-row"
                      key={directory.directory_id}
                      type="button"
                      onClick={() => openDirectory(directory)}
                    >
                      <Folder size={14} />
                      <span>{directory.name}</span>
                    </button>
                  ))}
                </div>
              </>
            )}
          </aside>
          <main className="panel list-panel">
            <PanelTitle trailing={<span className="counter">{totalCount}</span>}>
              {currentLabel}
            </PanelTitle>
            {loading ? (
              <BusyState label={progress || "Loading photos"} />
            ) : error ? (
              <EmptyState title="Unable to load photos" detail={error} />
            ) : !library ? (
              <EmptyState
                icon={FolderOpen}
                title="Open a photo folder"
                detail="Type a path or use Browse."
              />
            ) : visibleCount === 0 ? (
              <EmptyState
                icon={Image}
                title="This folder is empty"
                detail="Refresh after adding supported image files."
              />
            ) : (
              <div className="photo-list">
                {photos.map((photo) => (
                  <button
                    className={`photo-row${selectedPhoto?.photo_id === photo.photo_id ? " active" : ""}`}
                    key={photo.photo_id}
                    type="button"
                    onClick={() => setSelectedPhoto(photo)}
                  >
                    <Image size={14} />
                    <div>
                      <strong>{photo.filename}</strong>
                      <span>{formatBytes(photo.file_size)}</span>
                    </div>
                  </button>
                ))}
                {visibleCount < totalCount && (
                  <div className="list-note">Showing the first {visibleCount} entries</div>
                )}
              </div>
            )}
          </main>
          <aside className="panel preview-panel">
            <PanelTitle>Preview</PanelTitle>
            <PhotoPreview photo={selectedPhoto} />
          </aside>
        </div>
      )}

      {mode === "History" && (
        <div className="workspace-grid history-grid">
          <main className="panel list-panel">
            <PanelTitle trailing={<span className="counter">{operations.length}</span>}>
              Renames
            </PanelTitle>
            {loading ? (
              <BusyState label="Loading rename history" />
            ) : operations.length === 0 ? (
              <EmptyState
                icon={History}
                title="No rename history"
                detail="Photo rename operations will appear here."
              />
            ) : (
              <div className="photo-list">
                {operations.map((operation) => (
                  <button
                    className={`photo-row${selectedOperation?.operation_id === operation.operation_id ? " active" : ""}`}
                    key={operation.operation_id}
                    type="button"
                    onClick={() => setSelectedOperation(operation)}
                  >
                    <History size={14} />
                    <div>
                      <strong>{operation.new_filename}</strong>
                      <span>{operation.status}</span>
                    </div>
                  </button>
                ))}
              </div>
            )}
          </main>
          <aside className="panel preview-panel">
            <PanelTitle>Details</PanelTitle>
            {selectedOperation ? (
              <dl className="compact-details standalone">
                <dt>Before</dt>
                <dd>{selectedOperation.old_filename}</dd>
                <dt>After</dt>
                <dd>{selectedOperation.new_filename}</dd>
                <dt>Folder</dt>
                <dd>{selectedOperation.directory_relative_path || "/"}</dd>
                <dt>Status</dt>
                <dd>{selectedOperation.status}</dd>
                <dt>Applied</dt>
                <dd>{selectedOperation.applied_at}</dd>
              </dl>
            ) : (
              <EmptyState title="No operation selected" />
            )}
          </aside>
        </div>
      )}

      {mode === "Taxa" && (
        <div className="single-panel">
          <EmptyState
            icon={TreeDeciduous}
            title="Taxon browser"
            detail="This view will use the mappings created in Mapping."
          />
        </div>
      )}

      {mode === "Map" && (
        <div className="single-panel">
          <EmptyState
            icon={Map}
            title="Map"
            detail="Geotagged photos will appear here after metadata is indexed."
          />
        </div>
      )}
    </section>
  );
}

function lastPathSegment(path: string): string {
  const parts = path.split(/[/\\]/).filter(Boolean);
  return parts[parts.length - 1] || "Root";
}
