import {
  ArrowDown,
  ArrowUp,
  BugPlay,
  CaseSensitive,
  ChevronDown,
  ChevronRight,
  Database,
  FolderCog,
  Info,
  Library,
  Map,
  Move,
  Pencil,
  Plug,
  RefreshCcw,
  Save,
  SlidersHorizontal,
  Trash2,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  updateGeneralSettings,
  type GeneralSettings as GeneralSettingsValue,
  type WorkspaceSettingsSection,
} from "../../api/general";
import { getAppVersion } from "../../api/updater";
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
import { getMapSettings, setMapSettings, type MapSettings } from "../../api/map";
import { getTaxonomyBaseMetadata, type TaxonomyBaseMetadata } from "../../api/baseImport";
import {
  getNamingHookSettings,
  getNamingHookTemplates,
  getNamingHookTestCases,
  getPhotoFilenameFormatSettings,
  getPhotoNameMatchSettings,
  getTaxonomyNameSeparator,
  runNamingHookTests,
  saveNamingHook,
  setPhotoFilenameFormatSettings,
  setPhotoNameMatchSettings,
  setTaxonomyNameSeparator,
  type NamingHookKind,
  type NamingHookTestCase,
  type NamingHookTestReport,
  type NamingHookTestResult,
  type PhotoFilenameFormatSettings,
  type PhotoNameField,
} from "../../api/settings";
import { selectDatabaseDestination, selectPhotoDirectory, selectSqliteDatabase } from "../../api/dialogs";
import { Button, IconButton, SectionHeader } from "../../shared/ui";
import { CodeEditor } from "../../shared/CodeEditor";
import { ResizablePanels } from "../../shared/ResizablePanels";
import { BaseImportSettings } from "../taxonomy/BaseImportSettings";
import { emitMetadataChange } from "../../shared/metadataChanges";
import {
  defaultPhotoFilenameFormatSettings,
  normalizePhotoFilenameFormatSettings,
  photoFilenameFormatFields,
  photoNamePriorityFields,
  photoNamePriorityLabels,
} from "./namingSettings";

export type SettingsSection = WorkspaceSettingsSection;

const settingsSections: Array<{
  id: SettingsSection;
  icon: LucideIcon;
}> = [
  { id: "General", icon: SlidersHorizontal },
  { id: "Storage", icon: FolderCog },
  { id: "Photo Libraries", icon: Library },
  { id: "Taxonomy Databases", icon: Database },
  { id: "Naming", icon: CaseSensitive },
  { id: "Map", icon: Map },
];

export function SettingsView({
  onBaseReplaced,
  onSectionChange,
  onWorkspaceChanged,
  generalSettings,
  generalSettingsLoadError,
  onGeneralSettingsChange,
  section,
}: {
  generalSettings: GeneralSettingsValue;
  generalSettingsLoadError?: string;
  onGeneralSettingsChange: (settings: GeneralSettingsValue) => void;
  onBaseReplaced?: () => void;
  onSectionChange: (section: SettingsSection) => void;
  onWorkspaceChanged?: (resetPhotoTabs: boolean) => void;
  section: SettingsSection;
}) {
  const hookSection = section === "Filename Parser" || section === "Synonym Splitter";
  const [hooksExpanded, setHooksExpanded] = useState(hookSection);

  useEffect(() => {
    if (hookSection) setHooksExpanded(true);
  }, [hookSection]);

  return (
    <ResizablePanels
      className="settings-workbench"
      initialSize={180}
      minFirst={150}
      minSecond={420}
      separatorLabel="Resize Settings navigation"
      first={(<aside className="settings-nav">
        {settingsSections.map(({ id, icon: Icon }) => (
          <button className={section === id ? "active" : ""} type="button" key={id} onClick={() => onSectionChange(id)}>
            <Icon size={14} />{id}
          </button>
        ))}
        <button className={hookSection ? "active" : ""} type="button" onClick={() => setHooksExpanded((current) => !current)}>
          <Plug size={14} />Hooks
          {hooksExpanded ? <ChevronDown className="settings-nav-chevron" size={13} /> : <ChevronRight className="settings-nav-chevron" size={13} />}
        </button>
        {hooksExpanded && (
          <div className="settings-subnav">
            {(["Filename Parser", "Synonym Splitter"] as const).map((id) => (
              <button className={section === id ? "active" : ""} type="button" key={id} onClick={() => onSectionChange(id)}>
                {id}<span className="settings-language-badge">Rhai</span>
              </button>
            ))}
          </div>
        )}
        <div className="settings-nav-spacer" />
        <button className={section === "About" ? "active" : ""} type="button" onClick={() => onSectionChange("About")}>
          <Info size={14} />About
        </button>
      </aside>)}
      second={(<main className="settings-content">
        {section === "General" && (
          <GeneralSettings
            settings={generalSettings}
            loadError={generalSettingsLoadError}
            onChange={onGeneralSettingsChange}
          />
        )}
        {section === "Storage" && <StorageSettings />}
        {section === "Photo Libraries" && <PhotoLibrariesSettings onChanged={onWorkspaceChanged} />}
        {section === "Taxonomy Databases" && <BaseImportSettings onApplied={onBaseReplaced} />}
        {section === "Naming" && <NamingSettings />}
        {section === "Map" && <MapSettingsPanel />}
        {hookSection && <HooksSettings kind={section === "Filename Parser" ? "photo_filename" : "synonym_authority"} />}
        {section === "About" && <AboutSettings />}
      </main>)}
    />
  );
}

function GeneralSettings({
  settings,
  loadError = "",
  onChange,
}: {
  settings: GeneralSettingsValue;
  loadError?: string;
  onChange: (settings: GeneralSettingsValue) => void;
}) {
  const [message, setMessage] = useState(loadError);
  const [saving, setSaving] = useState(false);
  const saveSequence = useRef(0);

  useEffect(() => {
    if (loadError) setMessage(loadError);
  }, [loadError]);

  async function change(next: GeneralSettingsValue) {
    const previous = settings;
    const sequence = ++saveSequence.current;
    onChange(next);
    setSaving(true);
    setMessage("");
    try {
      const saved = await updateGeneralSettings(next);
      if (sequence === saveSequence.current) {
        onChange(saved);
        setMessage("Saved");
      }
    } catch (nextError) {
      if (sequence === saveSequence.current) {
        onChange(previous);
        setMessage(errorMessage(nextError));
      }
    } finally {
      if (sequence === saveSequence.current) setSaving(false);
    }
  }

  return (
    <div className="settings-section">
      <SectionHeader title="General" detail="Configure application-wide settings." />
      <section className="settings-group" aria-labelledby="general-appearance-heading">
        <h3 id="general-appearance-heading">Appearance</h3>
        <label className="general-setting-row">
          <span><strong>Theme</strong><small>Choose the application color scheme.</small></span>
          <select
            aria-label="Theme"
            disabled={saving}
            value={settings.theme}
            onChange={(event) => void change({
              ...settings,
              theme: event.target.value as GeneralSettingsValue["theme"],
            })}
          >
            <option value="system">System</option>
            <option value="light">Light</option>
            <option value="dark">Dark</option>
          </select>
        </label>
      </section>
      <section className="settings-group" aria-labelledby="general-startup-heading">
        <h3 id="general-startup-heading">Startup</h3>
        <label className="general-setting-row">
          <span><strong>Restore previously opened tabs</strong><small>Restore valid tabs and the active tab on the next launch.</small></span>
          <input
            aria-label="Restore previously opened tabs"
            disabled={saving}
            type="checkbox"
            checked={settings.restore_tabs}
            onChange={(event) => void change({ ...settings, restore_tabs: event.target.checked })}
          />
        </label>
      </section>
      <section className="settings-group" aria-labelledby="general-search-heading">
        <h3 id="general-search-heading">Search</h3>
        <label className="general-setting-row">
          <span><strong>Recent searches limit</strong><small>Keep between 1 and 50 searches on this computer.</small></span>
          <input
            aria-label="Recent searches limit"
            disabled={saving}
            type="number"
            min={1}
            max={50}
            value={settings.recent_searches_limit}
            onChange={(event) => {
              const value = Number(event.target.value);
              if (Number.isInteger(value) && value >= 1 && value <= 50) {
                void change({ ...settings, recent_searches_limit: value });
              }
            }}
          />
        </label>
      </section>
      {(saving || message) && (
        <div className={saving || message === "Saved" ? "editor-message" : "inline-error"} role={saving || message === "Saved" ? "status" : "alert"}>
          {saving ? "Saving..." : message}
        </div>
      )}
    </div>
  );
}

function StorageSettings() {
  const [locations, setLocations] = useState<DatabaseLocations | null>(null);
  const [baseMetadata, setBaseMetadata] = useState<TaxonomyBaseMetadata | null>(null);
  const [message, setMessage] = useState("");
  useEffect(() => {
    void Promise.all([getDatabaseLocations(), getTaxonomyBaseMetadata()])
      .then(([nextLocations, nextMetadata]) => {
        setLocations(nextLocations);
        setBaseMetadata(nextMetadata);
      })
      .catch((nextError) => setMessage(errorMessage(nextError)));
  }, []);

  async function changeDefaultPhotoLibraryDirectory() {
    const directory = await selectPhotoDirectory();
    if (!directory) return;
    try {
      setLocations(await setDefaultPhotoLibraryDatabaseDirectory(directory));
      setMessage("Default storage directory updated.");
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    }
  }

  async function relocate() {
    const destination = await selectDatabaseDestination(locations?.taxonomy_database);
    if (!destination) return;
    try {
      setLocations(await relocateTaxonomyDatabase(destination));
      setMessage("Taxonomy Database relocated.");
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    }
  }

  async function changeDefaultDirectory() {
    const directory = await selectPhotoDirectory();
    if (!directory) return;
    try {
      setLocations(await setDefaultTaxonomyDatabaseDirectory(directory));
      setMessage("Default Taxonomy Database directory updated.");
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    }
  }

  return (
    <div className="settings-section">
      <SectionHeader title="Storage" detail="View database locations, default directories, and taxonomy metadata." />
      <StoragePath label="Metadata database" value={locations?.metadata_database ?? "Loading"} />
      <StoragePath
        label="Taxonomy database"
        value={locations?.taxonomy_database ?? "Loading"}
        action={<Button onClick={() => void relocate()}><Move size={13} />Relocate</Button>}
      />
      <StoragePath
        label="Photo Library database default directory"
        value={locations?.default_photo_library_directory ?? "Loading"}
        action={<Button onClick={() => void changeDefaultPhotoLibraryDirectory()}><FolderCog size={13} />Change</Button>}
      />
      <StoragePath
        label="Taxonomy database default directory"
        value={locations?.default_taxonomy_directory ?? "Loading"}
        action={<Button onClick={() => void changeDefaultDirectory()}><FolderCog size={13} />Change</Button>}
      />
      <h3>Taxonomy metadata</h3>
      <StoragePath label="Current source" value={baseMetadata?.source_path ?? "Not imported"} />
      <Setting label="Taxa" value={baseMetadata ? String(baseMetadata.taxa_count) : "-"} />
      <Setting label="Names" value={baseMetadata ? String(baseMetadata.taxon_names_count) : "-"} />
      <Setting label="Imported" value={baseMetadata?.imported_at ?? "-"} />
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
        detail="Open, register, and manage photo libraries."
        actions={(
          <>
            <Button onClick={() => void load()}><RefreshCcw size={13} />Refresh</Button>
            <Button variant="primary" disabled={Boolean(busy)} onClick={() => void createLibrary()}>Create or register</Button>
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
              <Button size="small" disabled={library.active || !library.root_available || !library.database_available} onClick={() => void mutate("Switching library", () => switchPhotoLibrary(library.library_uuid), true)}>Open</Button>
              <Button size="small" onClick={() => {
                const name = window.prompt("Photo Library name", library.display_name)?.trim();
                if (name) void mutate("Renaming library", () => renamePhotoLibrary(library.library_uuid, name));
              }}><Pencil size={12} />Rename</Button>
              <Button size="small" onClick={() => void selectPhotoDirectory().then((path) => {
                if (path) return mutate("Rebinding root", () => rebindPhotoLibraryRoot(library.library_uuid, path), library.active);
              })}>Rebind root</Button>
              <Button size="small" onClick={() => void selectSqliteDatabase().then((path) => {
                if (path) return mutate("Rebinding database", () => rebindPhotoLibraryDatabase(library.library_uuid, path), library.active);
              })}>Rebind DB</Button>
              <Button size="small" disabled={!library.database_available} onClick={() => void selectDatabaseDestination(library.db_path).then((path) => {
                if (path) return mutate("Relocating database", () => relocatePhotoLibraryDatabase(library.library_uuid, path), library.active);
              })}><Move size={12} />Relocate DB</Button>
              <Button size="small" disabled={library.active} onClick={() => void mutate("Removing registration", () => removePhotoLibrary(library.library_uuid))}><Trash2 size={12} />Remove</Button>
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
      <SectionHeader title="About" detail="View application, version, author, and project information." />
      <div className="about-settings">
        <strong>Vividarium</strong>
        <Setting label="Version" value={version} />
        <Setting label="Author" value="Yunfan Bao" />
        <div className="setting-row">
          <span>GitHub</span>
          <a href="https://github.com/baoyunfan0101/Vividarium" target="_blank" rel="noreferrer">github.com/baoyunfan0101/Vividarium</a>
        </div>
      </div>
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
  const [priority, setPriority] = useState<PhotoNameField[]>([...photoNamePriorityFields]);
  const [format, setFormat] = useState<PhotoFilenameFormatSettings>(defaultPhotoFilenameFormatSettings);
  const [separator, setSeparator] = useState(";");
  const [loadError, setLoadError] = useState("");
  const [message, setMessage] = useState("");
  useEffect(() => {
    let active = true;
    void Promise.allSettled([
      getPhotoNameMatchSettings(),
      getPhotoFilenameFormatSettings(),
      getTaxonomyNameSeparator(),
    ]).then(([matchResult, formatResult, separatorResult]) => {
      if (!active) return;
      const errors: string[] = [];
      if (matchResult.status === "fulfilled") setPriority(matchResult.value.priority);
      else errors.push(`Mapping priority: ${errorMessage(matchResult.reason)}`);
      if (formatResult.status === "fulfilled") {
        setFormat(normalizePhotoFilenameFormatSettings(formatResult.value));
      } else {
        errors.push(`Photo filename format: ${errorMessage(formatResult.reason)}. Showing defaults.`);
      }
      if (separatorResult.status === "fulfilled") setSeparator(separatorResult.value);
      else errors.push(`Multiple-name separator: ${errorMessage(separatorResult.reason)}`);
      setLoadError(errors.join(" "));
    });
    return () => { active = false; };
  }, []);

  function move(index: number, direction: -1 | 1) {
    const target = index + direction;
    if (target < 0 || target >= priority.length) return;
    const next = [...priority];
    [next[index], next[target]] = [next[target], next[index]];
    setPriority(next);
  }

  async function save() {
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
      <SectionHeader title="Naming" detail="Configure taxonomy matching and mapped-photo filename generation." actions={<Button variant="primary" onClick={() => void save()}><Save size={13} />Save</Button>} />
      <h3>Mapping name priority</h3>
      <div className="priority-list">
        {priority.map((field, index) => (
          <div key={field}>
            <b>{index + 1}</b><span>{photoNamePriorityLabels[field]}</span>
            <IconButton
              aria-label={`Move ${photoNamePriorityLabels[field]} up`}
              disabled={index === 0}
              onClick={() => move(index, -1)}
            ><ArrowUp size={13} /></IconButton>
            <IconButton
              aria-label={`Move ${photoNamePriorityLabels[field]} down`}
              disabled={index === priority.length - 1}
              onClick={() => move(index, 1)}
            ><ArrowDown size={13} /></IconButton>
          </div>
        ))}
      </div>
      <h3>Photo filename format</h3>
      <div className="checkbox-grid">{photoFilenameFormatFields.map(({ field, label }) => (
        <label key={field}><input type="checkbox" checked={format[field]} onChange={(event) => setFormat({ ...format, [field]: event.target.checked })} />{label}</label>
      ))}</div>
      <label className="field-stack"><span>Multiple-name separator</span><input value={separator} maxLength={1} onChange={(event) => setSeparator(event.target.value)} /></label>
      {loadError && <div className="editor-message error-message" role="alert">{loadError}</div>}
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
      <SectionHeader title="Map" detail="Configure the map tile provider and provider credentials." actions={<Button variant="primary" onClick={() => void save()}><Save size={13} />Save</Button>} />
      <label className="field-stack"><span>Tile provider</span><select value={settings.provider} onChange={(event) => setSettings({ ...settings, provider: event.target.value as MapSettings["provider"] })}><option value="osm">OpenStreetMap</option><option value="tianditu">Tianditu</option></select></label>
      <label className="field-stack"><span>Token</span><input disabled={settings.provider !== "tianditu"} value={settings.tianditu_token ?? ""} onChange={(event) => setSettings({ ...settings, tianditu_token: event.target.value || null })} /></label>
      <div className="editor-message">{message}</div>
    </div>
  );
}

function HooksSettings({ kind }: { kind: NamingHookKind }) {
  const [scripts, setScripts] = useState<Record<NamingHookKind, string>>({ photo_filename: "", synonym_authority: "" });
  const [cases, setCases] = useState<Record<NamingHookKind, NamingHookTestCase[]>>({ photo_filename: [], synonym_authority: [] });
  const [report, setReport] = useState<NamingHookTestReport | null>(null);
  const [testedSnapshot, setTestedSnapshot] = useState<Record<NamingHookKind, string | null>>({ photo_filename: null, synonym_authority: null });
  const [busy, setBusy] = useState("");
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

  useEffect(() => {
    setReport(null);
    setMessage("");
  }, [kind]);

  const currentSnapshot = JSON.stringify({ script: scripts[kind], cases: cases[kind] });

  function invalidate(nextScripts = scripts, nextCases = cases) {
    setScripts(nextScripts);
    setCases(nextCases);
    setReport(null);
    setTestedSnapshot((current) => ({ ...current, [kind]: null }));
  }

  async function run() {
    setBusy("Testing Hook");
    try {
      const snapshot = currentSnapshot;
      const next = await runNamingHookTests(kind, scripts[kind], cases[kind]);
      setReport(next);
      if (next.failed === 0) {
        setTestedSnapshot((current) => ({ ...current, [kind]: snapshot }));
        setMessage("All tests passed. The current Hook can be saved.");
      } else {
        setTestedSnapshot((current) => ({ ...current, [kind]: null }));
        setMessage("Tests failed. Review the actual output before saving.");
      }
    } catch (nextError) {
      setTestedSnapshot((current) => ({ ...current, [kind]: null }));
      setMessage(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  async function save() {
    setBusy("Saving Hook");
    try {
      await saveNamingHook(kind, scripts[kind], cases[kind]);
      setMessage("Hook and project tests saved.");
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    } finally {
      setBusy("");
    }
  }

  const title = kind === "photo_filename" ? "Filename Parser" : "Synonym Splitter";
  const detail = kind === "photo_filename"
    ? "Edit and test the hook that extracts taxonomy information from filenames."
    : "Edit and test the hook that separates names from authority information.";

  return (
    <div className="hooks-settings">
      <SectionHeader title={title} detail={detail} actions={(
        <>
          <Button disabled={Boolean(busy) || !scripts[kind].trim()} onClick={() => void run()}><BugPlay size={13} />Test</Button>
          <Button variant="primary" disabled={Boolean(busy) || testedSnapshot[kind] !== currentSnapshot} onClick={() => void save()}><Save size={13} />Save</Button>
        </>
      )} />
      <ResizablePanels
        className="hook-columns"
        initialRatio={{ horizontal: 0.55, vertical: 0.52 }}
        minFirst={{ horizontal: 320, vertical: 180 }}
        minSecond={{ horizontal: 300, vertical: 180 }}
        responsiveBreakpoint={800}
        separatorLabel="Resize Hook editor and Project tests"
        first={(<CodeEditor
            language="rhai"
            ariaLabel={`${kind} Rhai source`}
            value={scripts[kind]}
            onChange={(value) => invalidate({ ...scripts, [kind]: value })}
          />)}
        second={(<div className="hook-tests">
          <header><strong>Project tests</strong><Button variant="ghost" size="small" onClick={() => invalidate(scripts, { ...cases, [kind]: [...cases[kind], { input: "", expected: { kind, output: {} } }] })}>+ Add</Button></header>
          <div className="hook-test-list">
            {cases[kind].map((item, index) => (
              <div className={`hook-test-row${report?.cases[index] && !report.cases[index].passed ? " failed" : ""}`} key={index}>
                <header>
                  <strong>Test {index + 1}</strong>
                  {report?.cases[index] && <span className={report.cases[index].passed ? "passed" : "failed"}>{report.cases[index].passed ? "Passed" : "Failed"}</span>}
                  <IconButton size="small" aria-label={`Delete Test ${index + 1}`} title={`Delete Test ${index + 1}`} onClick={() => invalidate(scripts, { ...cases, [kind]: cases[kind].filter((_, itemIndex) => itemIndex !== index) })}><Trash2 size={12} /></IconButton>
                </header>
                <label className="hook-raw-field"><span>Raw input</span><input className="hook-raw-input" value={item.input} onChange={(event) => invalidate(scripts, changeCase(cases, kind, index, { ...item, input: event.target.value }))} /></label>
                <label><span>Expected output</span>
                  <ExpectedEditor
                    value={item.expected}
                    testNumber={index + 1}
                    onDraftChange={() => invalidate()}
                    onChange={(expected) => invalidate(scripts, changeCase(cases, kind, index, { ...item, expected }))}
                  />
                </label>
                {report?.cases[index] && (
                  <div className="hook-test-actual">
                    <span>Actual output</span>
                    <CodeEditor
                      autoGrow
                      language="json"
                      ariaLabel={`Test ${index + 1} actual output`}
                      minHeight={72}
                      maxHeight={160}
                      onChange={() => undefined}
                      readOnly
                      value={JSON.stringify(report.cases[index].actual, null, 2)}
                    />
                    {report.cases[index].error && <p>{report.cases[index].error}</p>}
                  </div>
                )}
              </div>
            ))}
          </div>
        </div>)}
      />
      <div className="editor-message">{busy || (report ? `${report.passed} passed, ${report.failed} failed. ${message}` : message)}</div>
    </div>
  );
}

function ExpectedEditor({
  value,
  testNumber,
  onDraftChange,
  onChange,
}: {
  value: NamingHookTestResult;
  testNumber: number;
  onDraftChange: () => void;
  onChange: (value: NamingHookTestResult) => void;
}) {
  const serialized = JSON.stringify(value, null, 2);
  const [draft, setDraft] = useState(serialized);
  const [error, setError] = useState("");

  useEffect(() => setDraft(serialized), [serialized]);

  function update(next: string) {
    setDraft(next);
    onDraftChange();
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
      <CodeEditor
        autoGrow
        language="json"
        ariaLabel={`Test ${testNumber} expected output`}
        minHeight={72}
        maxHeight={160}
        value={draft}
        onChange={update}
      />
    </div>
  );
}

function changeCase(
  cases: Record<NamingHookKind, NamingHookTestCase[]>,
  kind: NamingHookKind,
  index: number,
  value: NamingHookTestCase,
): Record<NamingHookKind, NamingHookTestCase[]> {
  return { ...cases, [kind]: cases[kind].map((item, itemIndex) => itemIndex === index ? value : item) };
}

function Setting({ label, value }: { label: string; value: string }) {
  return <div className="setting-row"><span>{label}</span><strong>{value}</strong></div>;
}
