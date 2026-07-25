import { useEffect, useState } from "react";
import {
  CircleHelp,
  Database,
  FolderOpen,
  Keyboard,
  Search,
  TerminalSquare,
} from "lucide-react";
import {
  getMappingMetadata,
  getOperationsStatus,
  getPhotoLibrary,
  getPhotoLibraryCount,
  type MappingMetadata,
  type OperationsStatus,
  type PhotoLibrary,
} from "./api";
import { BusyState, EmptyState, PanelTitle, Tabs, errorMessage } from "./components";

type SettingsMode =
  | "Workspace"
  | "Database"
  | "Shortcuts"
  | "Diagnostics"
  | "About";

const settingsModes = [
  "Workspace",
  "Database",
  "Shortcuts",
  "Diagnostics",
  "About",
] as const;

export function SettingsView({
  onStatus,
}: {
  onStatus: (message: string, busy?: boolean) => void;
}) {
  const [mode, setMode] = useState<SettingsMode>("Workspace");
  const [library, setLibrary] = useState<PhotoLibrary | null>(null);
  const [photoCount, setPhotoCount] = useState(0);
  const [metadata, setMetadata] = useState<MappingMetadata | null>(null);
  const [operations, setOperations] = useState<OperationsStatus>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  useEffect(() => {
    setLoading(true);
    Promise.all([
      getPhotoLibrary(),
      getPhotoLibraryCount(),
      getMappingMetadata(),
      getOperationsStatus(),
    ])
      .then(([nextLibrary, nextPhotoCount, nextMetadata, nextOperations]) => {
        setLibrary(nextLibrary);
        setPhotoCount(nextPhotoCount);
        setMetadata(nextMetadata);
        setOperations(nextOperations);
        onStatus("Settings ready");
      })
      .catch((nextError) => {
        const message = errorMessage(nextError);
        setError(message);
        onStatus(message);
      })
      .finally(() => setLoading(false));
  }, [onStatus]);

  return (
    <section className="module-view">
      <div className="topbar">
        <div className="command-field passive">
          <Search size={14} />
          <span>Settings</span>
        </div>
        <span className="topbar-context">Vividarium</span>
      </div>
      <Tabs items={settingsModes} value={mode} onChange={setMode} />
      <div className="settings-layout">
        <div className="panel settings-panel">
          <PanelTitle>{mode}</PanelTitle>
          {loading ? (
            <BusyState label="Loading settings" />
          ) : error ? (
            <EmptyState title="Unable to load settings" detail={error} />
          ) : (
            <SettingsContent
              mode={mode}
              library={library}
              photoCount={photoCount}
              metadata={metadata}
              operations={operations}
            />
          )}
        </div>
      </div>
    </section>
  );
}

function SettingsContent({
  mode,
  library,
  photoCount,
  metadata,
  operations,
}: {
  mode: SettingsMode;
  library: PhotoLibrary | null;
  photoCount: number;
  metadata: MappingMetadata | null;
  operations: OperationsStatus;
}) {
  if (mode === "Workspace") {
    return (
      <div className="settings-grid">
        <SettingRow
          icon={FolderOpen}
          label="Photo root"
          value={library?.root_path ?? "Not configured"}
        />
        <SettingRow
          icon={Database}
          label="Photos"
          value={String(photoCount)}
        />
      </div>
    );
  }

  if (mode === "Database") {
    return (
      <div className="settings-grid">
        <SettingRow
          icon={Database}
          label="Mapped"
          value={String(metadata?.mapped_photo_count ?? 0)}
        />
        <SettingRow
          icon={Database}
          label="Taxa in use"
          value={String(metadata?.mapping_taxa_count ?? 0)}
        />
      </div>
    );
  }

  if (mode === "Shortcuts") {
    return (
      <div className="settings-grid">
        <SettingRow icon={Keyboard} label="Open" value="Enter" />
        <SettingRow icon={Keyboard} label="Clear" value="Escape" />
      </div>
    );
  }

  if (mode === "Diagnostics") {
    const photoState = operations.photos;
    const mappingState = operations.mapping;
    return (
      <div className="settings-grid">
        <SettingRow
          icon={TerminalSquare}
          label="Photos"
          value={photoState?.running ? photoState.message : "Idle"}
        />
        <SettingRow
          icon={TerminalSquare}
          label="Mapping"
          value={mappingState?.running ? mappingState.message : "Idle"}
        />
      </div>
    );
  }

  return (
    <div className="about-panel">
      <CircleHelp size={24} />
      <strong>Vividarium</strong>
      <span>Photo and taxonomy workbench</span>
      <code>2.1.0</code>
    </div>
  );
}

function SettingRow({
  icon: Icon,
  label,
  value,
}: {
  icon: typeof FolderOpen;
  label: string;
  value: string;
}) {
  return (
    <div className="setting-tile">
      <Icon size={16} />
      <span>{label}</span>
      <strong title={value}>{value}</strong>
    </div>
  );
}
