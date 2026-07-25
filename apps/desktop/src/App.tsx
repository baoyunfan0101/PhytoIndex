import { useCallback, useState } from "react";
import {
  ArrowRightLeft,
  Database,
  FileImage,
  Settings,
} from "lucide-react";
import appIconUrl from "../src-tauri/icons/icon.png";
import { MappingView } from "./v3/MappingView";
import { PhotosView } from "./v3/PhotosView";
import { SettingsView } from "./v3/SettingsView";
import { TaxonomyView } from "./v3/TaxonomyView";
import type { IconComponent } from "./v3/components";

type Module = "photos" | "mapping" | "taxonomy" | "settings";

const modules: Array<{
  id: Exclude<Module, "settings">;
  label: string;
  icon: IconComponent;
}> = [
  { id: "photos", label: "Photos", icon: FileImage },
  { id: "mapping", label: "Mapping", icon: ArrowRightLeft },
  { id: "taxonomy", label: "Taxonomy", icon: Database },
];

const settingsModule = {
  id: "settings" as const,
  label: "Settings",
  icon: Settings,
};

export function App() {
  const [module, setModule] = useState<Module>("photos");
  const [status, setStatus] = useState({
    message: "Ready",
    busy: false,
  });

  const handleStatus = useCallback((message: string, busy = false) => {
    setStatus({ message, busy });
  }, []);

  function switchModule(nextModule: Module) {
    setModule(nextModule);
    setStatus({ message: "Ready", busy: false });
  }

  return (
    <div className="desktop-shell">
      <aside className="activity-bar">
        <div className="brand-button" title="Vividarium">
          <img className="mark" src={appIconUrl} alt="Vividarium" />
        </div>
        <div className="activity-group">
          {modules.map(({ id, label, icon }) => (
            <ActivityButton
              key={id}
              active={module === id}
              icon={icon}
              label={label}
              onClick={() => switchModule(id)}
            />
          ))}
        </div>
        <div className="activity-spacer" />
        <ActivityButton
          active={module === settingsModule.id}
          icon={settingsModule.icon}
          label={settingsModule.label}
          onClick={() => switchModule(settingsModule.id)}
        />
      </aside>
      <div className="desktop-main">
        <div className="desktop-content">
          {module === "photos" && <PhotosView onStatus={handleStatus} />}
          {module === "mapping" && <MappingView onStatus={handleStatus} />}
          {module === "taxonomy" && <TaxonomyView onStatus={handleStatus} />}
          {module === "settings" && <SettingsView onStatus={handleStatus} />}
        </div>
        <footer className="status-bar">
          <span className={status.busy ? "status-dot busy" : "status-dot"} />
          <span>{status.message}</span>
          <span className="status-module">{module}</span>
        </footer>
      </div>
    </div>
  );
}

function ActivityButton({
  active,
  icon: Icon,
  label,
  onClick,
}: {
  active: boolean;
  icon: IconComponent;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      className={`activity-button${active ? " active" : ""}`}
      type="button"
      onClick={onClick}
      aria-label={label}
      title={label}
    >
      <Icon size={18} strokeWidth={1.8} />
      <span>{label}</span>
    </button>
  );
}
