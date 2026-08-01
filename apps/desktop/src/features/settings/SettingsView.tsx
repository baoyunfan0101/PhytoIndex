import {
  ArrowDown,
  ArrowUp,
  Beaker,
  CaseSensitive,
  Database,
  FolderCog,
  Info,
  Library,
  MapPinned,
  Move,
  Pencil,
  RefreshCcw,
  Save,
  SlidersHorizontal,
  Trash2,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import {
  checkAppUpdate,
  getAppVersion,
  installAppUpdate,
} from "../../api/updater";
import { errorMessage } from "../../api/common";
import {
  getDatabaseLocations,
  listPhotoLibraries,
  photoLibraryAvailabilityLabel,
  rebindPhotoLibraryDatabase,
  rebindPhotoLibraryRoot,
  registerPhotoLibrary,
  relocatePhotoLibraryDatabase,
  relocateTaxonomyDatabase,
  removePhotoLibrary,
  renamePhotoLibrary,
  setDefaultPhotoLibraryDatabaseDirectory,
  setDefaultTaxonomyDatabaseDirectory,
  switchPhotoLibrary,
  type DatabaseLocations,
  type PhotoLibraryWorkspace,
} from "../../api/storage";
import { getPhotoLibrary, getPhotoLibraryCount } from "../../api/photos";
import { getMapSettings, setMapSettings, type MapSettings } from "../../api/map";
import {
  getNamingHookSettings,
  getNamingHookTemplates,
  getNamingHookTestCases,
  getPhotoFilenameFormatSettings,
  getPhotoNameMatchSettings,
  getTaxonomyNameSeparator,
  setPhotoFilenameFormatSettings,
  setPhotoNameMatchSettings,
  setTaxonomyNameSeparator,
  testAndSaveNamingHook,
  type NamingHookKind,
  type NamingHookTestCase,
  type NamingHookTestReport,
  type NamingHookTestResult,
  type PhotoFilenameFormatSettings,
  type PhotoNameField,
} from "../../api/settings";
import { selectDatabaseDestination, selectPhotoDirectory, selectSqliteDatabase } from "../../api/dialogs";
import { SectionHeader, Segmented, VirtualList } from "../../shared/ui";
import { CodeEditor } from "../../shared/CodeEditor";
import { BaseImportSettings } from "../taxonomy/BaseImportSettings";
import { emitMetadataChange } from "../../shared/metadataChanges";

type SettingsSection =
  | "General"
  | "Storage"
  | "Photo Libraries"
  | "Naming"
  | "Map"
  | "Hooks"
  | "Base Import"
  | "About";

const settingsSections: Array<{
  id: SettingsSection;
  icon: LucideIcon;
}> = [
  { id: "General", icon: SlidersHorizontal },
  { id: "Storage", icon: Database },
  { id: "Photo Libraries", icon: Library },
  { id: "Naming", icon: CaseSensitive },
  { id: "Map", icon: MapPinned },
  { id: "Hooks", icon: Beaker },
  { id: "Base Import", icon: FolderCog },
  { id: "About", icon: Info },
];

export function SettingsView({
  onBaseReplaced,
  onWorkspaceChanged,
}: {
  onBaseReplaced?: () => void;
  onWorkspaceChanged?: (resetPhotoTabs: boolean) => void;
}) {
  const [section, setSection] = useState<SettingsSection>("General");
  return (
    <div className="settings-workbench">
      <aside className="settings-nav">
        {settingsSections.map(({ id, icon: Icon }) => (
          <button className={section === id ? "active" : ""} type="button" key={id} onClick={() => setSection(id)}>
            <Icon size={14} />{id}
          </button>
        ))}
      </aside>
      <main className="settings-content">
        {section === "General" && <GeneralSettings />}
        {section === "Storage" && <StorageSettings />}
        {section === "Photo Libraries" && <PhotoLibrariesSettings onChanged={onWorkspaceChanged} />}
        {section === "Naming" && <NamingSettings />}
        {section === "Map" && <MapSettingsPanel />}
        {section === "Hooks" && <HooksSettings />}
        {section === "Base Import" && <BaseImportSettings onApplied={onBaseReplaced} />}
        {section === "About" && <AboutSettings />}
      </main>
    </div>
  );
}

function GeneralSettings() {
  const [root, setRoot] = useState("Loading");
  const [count, setCount] = useState(0);
  const [version, setVersion] = useState("3.0.0");
  const [updateMessage, setUpdateMessage] = useState("");
  const [updateAvailable, setUpdateAvailable] = useState(false);
  useEffect(() => {
    void getAppVersion().then(setVersion);
    void getPhotoLibrary()
      .then((library) => setRoot(library?.root_path ?? "Not configured"))
      .catch(() => setRoot("Active library unavailable"));
    void getPhotoLibraryCount().then(setCount).catch(() => setCount(0));
  }, []);

  async function checkUpdate() {
    try {
      setUpdateMessage("Checking GitHub Releases");
      const update = await checkAppUpdate();
      setUpdateAvailable(Boolean(update));
      setUpdateMessage(update ? `Version ${update.version} is available` : "Vividarium is up to date");
    } catch (nextError) {
      setUpdateMessage(errorMessage(nextError));
    }
  }

  async function installUpdate() {
    try {
      await installAppUpdate((event) => {
        if (event.event === "progress") setUpdateMessage(`Downloaded ${event.data.downloaded} bytes`);
        if (event.event === "finished") setUpdateMessage("Installing and restarting");
      });
    } catch (nextError) {
      setUpdateMessage(errorMessage(nextError));
    }
  }
  return (
    <div className="settings-section">
      <SectionHeader title="General" detail="Workspace metadata" />
      <Setting label="Product" value="Vividarium" />
      <Setting label="Software version" value={version} />
      <Setting label="Photo root" value={root} />
      <Setting label="Indexed photos" value={String(count)} />
      <Setting label="Database schema" value="2" />
      <div className="update-row">
        <div><strong>Software update</strong><span>{updateMessage || "Updates are delivered from GitHub Releases."}</span></div>
        {updateAvailable ? <button className="primary-button" type="button" onClick={() => void installUpdate()}>Install and restart</button> : <button className="secondary-button" type="button" onClick={() => void checkUpdate()}>Check for updates</button>}
      </div>
    </div>
  );
}

function StorageSettings() {
  const [locations, setLocations] = useState<DatabaseLocations | null>(null);
  const [message, setMessage] = useState("");
  useEffect(() => {
    void getDatabaseLocations().then(setLocations).catch((nextError) => setMessage(errorMessage(nextError)));
  }, []);

  async function changeTaxonomyDatabase() {
    const destination = await selectDatabaseDestination(locations?.taxonomy_database);
    if (!destination) return;
    try {
      setLocations(await relocateTaxonomyDatabase(destination));
      setMessage("Taxonomy database relocated.");
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    }
  }

  async function changeDefault(kind: "taxonomy" | "photo") {
    const directory = await selectPhotoDirectory();
    if (!directory) return;
    try {
      setLocations(kind === "taxonomy"
        ? await setDefaultTaxonomyDatabaseDirectory(directory)
        : await setDefaultPhotoLibraryDatabaseDirectory(directory));
      setMessage("Default storage directory updated.");
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    }
  }

  return (
    <div className="settings-section">
      <SectionHeader title="Storage" detail="Independent metadata, taxonomy, and photo library database locations" />
      <StoragePath label="Metadata database" value={locations?.metadata_database ?? "Loading"} />
      <StoragePath
        label="Taxonomy database"
        value={locations?.taxonomy_database ?? "Loading"}
        action={<button type="button" onClick={() => void changeTaxonomyDatabase()}><Move size={13} />Relocate</button>}
      />
      <StoragePath
        label="Default taxonomy directory"
        value={locations?.default_taxonomy_directory ?? "Loading"}
        action={<button type="button" onClick={() => void changeDefault("taxonomy")}><FolderCog size={13} />Change</button>}
      />
      <StoragePath
        label="Default photo library DB directory"
        value={locations?.default_photo_library_directory ?? "Loading"}
        action={<button type="button" onClick={() => void changeDefault("photo")}><FolderCog size={13} />Change</button>}
      />
      <div className="editor-message">{message}</div>
    </div>
  );
}

function PhotoLibrariesSettings({ onChanged }: { onChanged?: (resetPhotoTabs: boolean) => void }) {
  const [libraries, setLibraries] = useState<PhotoLibraryWorkspace[]>([]);
  const [busy, setBusy] = useState("");
  const [message, setMessage] = useState("");

  async function load() {
    try {
      setLibraries(await listPhotoLibraries());
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    }
  }

  useEffect(() => { void load(); }, []);

  async function createLibrary() {
    const rootPath = await selectPhotoDirectory();
    if (!rootPath) return;
    const databasePath = await selectDatabaseDestination();
    if (!databasePath) return;
    const displayName = window.prompt("Photo Library name", rootPath.split(/[\\/]/).pop() ?? "Photo Library");
    if (displayName === null) return;
    await mutate(
      "Creating library",
      () => registerPhotoLibrary(rootPath, databasePath, displayName),
      true,
    );
  }

  async function mutate(
    label: string,
    action: () => Promise<unknown>,
    resetPhotoTabs = false,
  ) {
    setBusy(label);
    setMessage("");
    try {
      await action();
      await load();
      onChanged?.(resetPhotoTabs);
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  return (
    <div className="settings-section">
      <SectionHeader
        title="Photo Libraries"
        detail="One registered library represents one real photo root and one independent database"
        actions={(
          <>
            <button className="secondary-button" type="button" onClick={() => void load()}><RefreshCcw size={13} />Refresh</button>
            <button className="primary-button" type="button" disabled={Boolean(busy)} onClick={() => void createLibrary()}>Create or register</button>
          </>
        )}
      />
      <div className="library-settings-list">
        {libraries.map((library) => (
          <article className={`library-settings-row${library.active ? " active" : ""}`} key={library.library_uuid}>
            <div className="library-heading">
              <strong>{library.display_name}</strong>
              <span className={library.root_available && library.database_available ? "available" : "unavailable"}>
                {photoLibraryAvailabilityLabel(library)}
              </span>
              {library.active && <b>Active</b>}
            </div>
            <code>{library.root_path}</code>
            <code>{library.db_path}</code>
            <small>Last opened: {library.last_opened_at}</small>
            <div className="library-actions">
              <button type="button" disabled={library.active || !library.root_available || !library.database_available} onClick={() => void mutate("Switching library", () => switchPhotoLibrary(library.library_uuid), true)}>Open</button>
              <button type="button" onClick={() => {
                const name = window.prompt("Photo Library name", library.display_name)?.trim();
                if (name) void mutate("Renaming library", () => renamePhotoLibrary(library.library_uuid, name));
              }}><Pencil size={12} />Rename</button>
              <button type="button" onClick={() => void selectPhotoDirectory().then((path) => {
                if (path) return mutate("Rebinding root", () => rebindPhotoLibraryRoot(library.library_uuid, path), library.active);
              })}>Rebind root</button>
              <button type="button" onClick={() => void selectSqliteDatabase().then((path) => {
                if (path) return mutate("Rebinding database", () => rebindPhotoLibraryDatabase(library.library_uuid, path), library.active);
              })}>Rebind DB</button>
              <button type="button" disabled={!library.database_available} onClick={() => void selectDatabaseDestination(library.db_path).then((path) => {
                if (path) return mutate("Relocating database", () => relocatePhotoLibraryDatabase(library.library_uuid, path), library.active);
              })}><Move size={12} />Relocate DB</button>
              <button type="button" disabled={library.active} onClick={() => void mutate("Removing registration", () => removePhotoLibrary(library.library_uuid))}><Trash2 size={12} />Remove</button>
            </div>
          </article>
        ))}
      </div>
      {(busy || message) && <div className={message ? "inline-error" : "editor-message"}>{message || busy}</div>}
    </div>
  );
}

function AboutSettings() {
  const [version, setVersion] = useState("3.0.0");
  useEffect(() => { void getAppVersion().then(setVersion); }, []);
  return (
    <div className="settings-section">
      <SectionHeader title="About" detail="Vividarium desktop application" />
      <Setting label="Product" value="Vividarium" />
      <Setting label="Software version" value={version} />
      <Setting label="Database schema" value="2" />
    </div>
  );
}

function StoragePath({
  label,
  value,
  action,
}: {
  label: string;
  value: string;
  action?: ReactNode;
}) {
  return <div className="storage-path"><span>{label}</span><code title={value}>{value}</code>{action}</div>;
}

function NamingSettings() {
  const allFields: PhotoNameField[] = ["species_sci", "species_zh", "genus_sci", "genus_zh", "family_sci", "family_zh"];
  const [priority, setPriority] = useState<PhotoNameField[]>(allFields);
  const [format, setFormat] = useState<PhotoFilenameFormatSettings | null>(null);
  const [separator, setSeparator] = useState(";");
  const [message, setMessage] = useState("");
  useEffect(() => {
    Promise.all([getPhotoNameMatchSettings(), getPhotoFilenameFormatSettings(), getTaxonomyNameSeparator()])
      .then(([match, nextFormat, nextSeparator]) => {
        setPriority(match.priority);
        setFormat(nextFormat);
        setSeparator(nextSeparator);
      });
  }, []);

  function move(index: number, direction: -1 | 1) {
    const target = index + direction;
    if (target < 0 || target >= priority.length) return;
    const next = [...priority];
    [next[index], next[target]] = [next[target], next[index]];
    setPriority(next);
  }

  async function save() {
    if (!format) return;
    try {
      await Promise.all([
        setPhotoNameMatchSettings({ priority }),
        setPhotoFilenameFormatSettings(format),
        setTaxonomyNameSeparator(separator),
      ]);
      emitMetadataChange({ key: "taxonomy_name_separator", value: separator });
      setMessage("Naming metadata saved. Photos have been queued for remapping.");
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    }
  }

  return (
    <div className="settings-section">
      <SectionHeader title="Naming" detail="Mapping priority, filename output, and formatted input" actions={<button className="primary-button" type="button" onClick={() => void save()}><Save size={13} />Save</button>} />
      <h3>Six-field mapping priority</h3>
      <div className="priority-list">
        {priority.map((field, index) => (
          <div key={field}><b>{index + 1}</b><span>{field}</span><button type="button" onClick={() => move(index, -1)}><ArrowUp size={13} /></button><button type="button" onClick={() => move(index, 1)}><ArrowDown size={13} /></button></div>
        ))}
      </div>
      <h3>Photo filename fields</h3>
      {format && <div className="checkbox-grid">{allFields.map((field) => (
        <label key={field}><input type="checkbox" checked={format[field]} onChange={(event) => setFormat({ ...format, [field]: event.target.checked })} />{field}</label>
      ))}</div>}
      <label className="field-stack"><span>Multiple-name separator</span><input value={separator} maxLength={1} onChange={(event) => setSeparator(event.target.value)} /></label>
      <div className="editor-message">{message}</div>
    </div>
  );
}

function MapSettingsPanel() {
  const [settings, setSettings] = useState<MapSettings>({ provider: "osm", tianditu_token: null });
  const [message, setMessage] = useState("");
  useEffect(() => { void getMapSettings().then(setSettings); }, []);
  async function save() {
    try {
      setSettings(await setMapSettings(settings));
      setMessage("Map metadata saved.");
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    }
  }
  return (
    <div className="settings-section">
      <SectionHeader title="Map" detail="Tile source metadata" actions={<button className="primary-button" type="button" onClick={() => void save()}><Save size={13} />Save</button>} />
      <label className="field-stack"><span>Tile provider</span><select value={settings.provider} onChange={(event) => setSettings({ ...settings, provider: event.target.value as MapSettings["provider"] })}><option value="osm">OpenStreetMap</option><option value="tianditu">Tianditu</option></select></label>
      <label className="field-stack"><span>Tianditu token</span><input value={settings.tianditu_token ?? ""} onChange={(event) => setSettings({ ...settings, tianditu_token: event.target.value || null })} /></label>
      <div className="editor-message">{message}</div>
    </div>
  );
}

function HooksSettings() {
  const [kind, setKind] = useState<NamingHookKind>("photo_filename");
  const [scripts, setScripts] = useState<Record<NamingHookKind, string>>({ photo_filename: "", synonym_authority: "" });
  const [cases, setCases] = useState<Record<NamingHookKind, NamingHookTestCase[]>>({ photo_filename: [], synonym_authority: [] });
  const [report, setReport] = useState<NamingHookTestReport | null>(null);
  const [message, setMessage] = useState("");

  useEffect(() => {
    let active = true;
    getNamingHookTemplates().then((nextTemplates) => {
      if (!active) return;
      setScripts(nextTemplates);
      return getNamingHookSettings().then((settings) => {
        if (!active) return;
        setScripts({
          photo_filename: settings.photo_filename?.trim() ? settings.photo_filename : nextTemplates.photo_filename,
          synonym_authority: settings.synonym_authority?.trim() ? settings.synonym_authority : nextTemplates.synonym_authority,
        });
      });
    }).catch((nextError) => {
      if (active) setMessage(errorMessage(nextError));
    });
    getNamingHookTestCases().then((nextCases) => {
      if (active) setCases(nextCases);
    }).catch((nextError) => {
      if (active) setMessage(errorMessage(nextError));
    });
    return () => {
      active = false;
    };
  }, []);

  async function run() {
    try {
      const next = await testAndSaveNamingHook(kind, scripts[kind], cases[kind]);
      setReport(next);
      if (next.failed === 0) {
        setMessage("All tests passed. Hook and project tests saved.");
      } else {
        setMessage("Hook was not saved because tests failed.");
      }
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    }
  }

  return (
    <div className="hooks-settings">
      <SectionHeader title="Rhai hooks" detail="Default implementations and user overrides share the same execution path" actions={
        <button className="primary-button" type="button" onClick={() => void run()}><Beaker size={13} />Test and save</button>
      } />
      <Segmented value={kind} items={["photo_filename", "synonym_authority"] as const} onChange={(next) => {
        setKind(next);
        setReport(null);
      }} />
      <div className="hook-columns">
        <CodeEditor
          language="rhai"
          ariaLabel={`${kind} Rhai source`}
          value={scripts[kind]}
          onChange={(value) => setScripts({ ...scripts, [kind]: value })}
        />
        <div className="hook-tests">
          <header><strong>Project tests</strong><button type="button" onClick={() => setCases({ ...cases, [kind]: [...cases[kind], { name: "New test", input: "", expected: { kind, output: {} } }] })}>+ Add</button></header>
          <VirtualList
            items={cases[kind]}
            rowHeight={report ? 252 : 228}
            itemKey={(_, index) => index}
            renderItem={(item, index) => (
              <div className={`hook-test-row${report?.cases[index] && !report.cases[index].passed ? " failed" : ""}`}>
                <input value={item.name} onChange={(event) => changeCase(cases, setCases, kind, index, { ...item, name: event.target.value })} />
                <input value={item.input} placeholder="Raw input" onChange={(event) => changeCase(cases, setCases, kind, index, { ...item, input: event.target.value })} />
                <button type="button" onClick={() => setCases({ ...cases, [kind]: cases[kind].filter((_, itemIndex) => itemIndex !== index) })}>Delete</button>
                <ExpectedEditor
                  value={item.expected}
                  testName={item.name}
                  onChange={(expected) => changeCase(cases, setCases, kind, index, { ...item, expected })}
                />
                {report?.cases[index] && <span className="hook-test-actual">Actual: {JSON.stringify(report.cases[index].actual)}</span>}
              </div>
            )}
          />
        </div>
      </div>
      <div className="editor-message">{report ? `${report.passed} passed, ${report.failed} failed. ${message}` : message}</div>
    </div>
  );
}

function ExpectedEditor({
  value,
  testName,
  onChange,
}: {
  value: NamingHookTestResult;
  testName: string;
  onChange: (value: NamingHookTestResult) => void;
}) {
  const serialized = JSON.stringify(value, null, 2);
  const [draft, setDraft] = useState(serialized);
  const [error, setError] = useState("");

  useEffect(() => setDraft(serialized), [serialized]);

  function update(next: string) {
    setDraft(next);
    try {
      const parsed = JSON.parse(next) as NamingHookTestResult;
      if (!parsed || typeof parsed !== "object" || !("kind" in parsed) || !("output" in parsed)) {
        throw new Error("Expected a tagged hook result");
      }
      setError("");
      onChange(parsed);
    } catch {
      setError("Invalid expected JSON");
    }
  }

  return (
    <div className={`hook-expected${error ? " invalid" : ""}`} title={error}>
      <CodeEditor language="json" ariaLabel={`${testName} expected output`} value={draft} onChange={update} />
    </div>
  );
}

function changeCase(
  cases: Record<NamingHookKind, NamingHookTestCase[]>,
  setCases: (value: Record<NamingHookKind, NamingHookTestCase[]>) => void,
  kind: NamingHookKind,
  index: number,
  value: NamingHookTestCase,
) {
  setCases({ ...cases, [kind]: cases[kind].map((item, itemIndex) => itemIndex === index ? value : item) });
}

function Setting({ label, value }: { label: string; value: string }) {
  return <div className="setting-row"><span>{label}</span><strong>{value}</strong></div>;
}
