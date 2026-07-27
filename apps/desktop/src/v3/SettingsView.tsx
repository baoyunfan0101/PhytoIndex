import {
  ArrowDown,
  ArrowUp,
  Beaker,
  CaseSensitive,
  DatabaseBackup,
  FileUp,
  MapPinned,
  Save,
  SlidersHorizontal,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useState } from "react";
import {
  errorMessage,
  checkAppUpdate,
  getTaxonomyBaseMetadata,
  getAppVersion,
  getMapSettings,
  getNamingHookSettings,
  getNamingHookTemplates,
  getNamingHookTestCases,
  getPhotoFilenameFormatSettings,
  getPhotoLibrary,
  getPhotoLibraryCount,
  getPhotoNameMatchSettings,
  getTaxonomyNameSeparator,
  installAppUpdate,
  replaceTaxonomyBaseDatabase,
  runNamingHookTests,
  selectTaxonomyBaseDatabase,
  setMapSettings,
  setNamingHook,
  setNamingHookTestCases,
  setPhotoFilenameFormatSettings,
  setPhotoNameMatchSettings,
  setTaxonomyNameSeparator,
  waitForOperation,
  type MapSettings,
  type NamingHookKind,
  type NamingHookTestCase,
  type NamingHookTestReport,
  type NamingHookTestResult,
  type PhotoFilenameFormatSettings,
  type PhotoNameField,
  type TaxonomyBaseMetadata,
} from "./api";
import { Modal, SectionHeader, Segmented, VirtualList } from "./components";
import { CodeEditor } from "./CodeEditor";
import { emitMetadataChange } from "./metadataChanges";

type SettingsSection = "General" | "Naming" | "Map" | "Base Database" | "Hooks";

const settingsSections: Array<{
  id: SettingsSection;
  icon: LucideIcon;
}> = [
  { id: "General", icon: SlidersHorizontal },
  { id: "Naming", icon: CaseSensitive },
  { id: "Map", icon: MapPinned },
  { id: "Base Database", icon: DatabaseBackup },
  { id: "Hooks", icon: Beaker },
];

export function SettingsView({ onBaseReplaced }: { onBaseReplaced?: () => void }) {
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
        {section === "Naming" && <NamingSettings />}
        {section === "Map" && <MapSettingsPanel />}
        {section === "Base Database" && <BaseDatabaseSettings onReplaced={onBaseReplaced} />}
        {section === "Hooks" && <HooksSettings />}
      </main>
    </div>
  );
}

function BaseDatabaseSettings({ onReplaced }: { onReplaced?: () => void }) {
  const [metadata, setMetadata] = useState<TaxonomyBaseMetadata | null>(null);
  const [sourcePath, setSourcePath] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");

  useEffect(() => {
    getTaxonomyBaseMetadata()
      .then(setMetadata)
      .catch((nextError) => setMessage(errorMessage(nextError)));
  }, []);

  async function chooseDatabase() {
    const selected = await selectTaxonomyBaseDatabase();
    if (selected) setSourcePath(selected);
  }

  async function replaceDatabase() {
    setBusy(true);
    setMessage("Replacing base database");
    try {
      const started = await replaceTaxonomyBaseDatabase(sourcePath);
      const completed = await waitForOperation("mapping", started.operation.task_id, (operation) => {
        setMessage(operation.message);
      });
      if (completed.error) throw new Error(completed.error);
      setMetadata(await getTaxonomyBaseMetadata());
      setMessage("Base database replaced and photo mapping completed.");
      setConfirming(false);
      onReplaced?.();
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="settings-section">
      <SectionHeader title="Base Database" detail="Trusted external taxonomy dataset metadata and replacement" />
      <Setting label="Source" value={metadata?.source_path ?? "Not imported"} />
      <Setting label="Taxa" value={metadata ? String(metadata.taxa_count) : "-"} />
      <Setting label="Taxon names" value={metadata ? String(metadata.taxon_names_count) : "-"} />
      <Setting label="Imported at" value={metadata?.imported_at ?? "-"} />
      <div className="base-database-action">
        <div>
          <strong>Replacement database</strong>
          <span title={sourcePath}>{sourcePath || "Select a .db, .sqlite, or .sqlite3 file"}</span>
        </div>
        <button className="secondary-button" type="button" disabled={busy} onClick={() => void chooseDatabase()}>
          <FileUp size={13} />Select file
        </button>
        <button className="primary-button" type="button" disabled={!sourcePath || busy} onClick={() => setConfirming(true)}>
          Replace base database
        </button>
      </div>
      <div className="editor-message">{message}</div>
      {confirming && (
        <Modal
          title="Replace base database"
          onClose={() => !busy && setConfirming(false)}
          actions={
            <>
              <button className="secondary-button" type="button" disabled={busy} onClick={() => setConfirming(false)}>Cancel</button>
              <button className="primary-button" type="button" disabled={busy} onClick={() => void replaceDatabase()}>
                {busy ? "Replacing" : "Replace and remap"}
              </button>
            </>
          }
        >
          <p>This clears the current taxonomy and its update history, imports the selected base database, and remaps all photos.</p>
          <p className="field-hint">{sourcePath}</p>
        </Modal>
      )}
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
    Promise.all([getPhotoLibrary(), getPhotoLibraryCount(), getAppVersion()]).then(([library, photoCount, appVersion]) => {
      setRoot(library?.root_path ?? "Not configured");
      setCount(photoCount);
      setVersion(appVersion);
    });
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
  const [templates, setTemplates] = useState<Record<NamingHookKind, string>>({ photo_filename: "", synonym_authority: "" });
  const [cases, setCases] = useState<Record<NamingHookKind, NamingHookTestCase[]>>({ photo_filename: [], synonym_authority: [] });
  const [report, setReport] = useState<NamingHookTestReport | null>(null);
  const [message, setMessage] = useState("");

  useEffect(() => {
    let active = true;
    getNamingHookTemplates().then((nextTemplates) => {
      if (!active) return;
      setTemplates(nextTemplates);
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

  async function save() {
    try {
      await Promise.all([setNamingHook(kind, scripts[kind]), setNamingHookTestCases(kind, cases[kind])]);
      setMessage("Hook and project tests saved.");
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    }
  }

  async function run() {
    try {
      setReport(await runNamingHookTests(kind, scripts[kind]));
    } catch (nextError) {
      setMessage(errorMessage(nextError));
    }
  }

  return (
    <div className="hooks-settings">
      <SectionHeader title="Rhai hooks" detail="Default implementations and user overrides share the same execution path" actions={
        <><button className="secondary-button" type="button" onClick={() => setScripts({ ...scripts, [kind]: templates[kind] })}>Reset template</button><button className="secondary-button" type="button" onClick={() => void run()}><Beaker size={13} />Run tests</button><button className="primary-button" type="button" onClick={() => void save()}><Save size={13} />Save</button></>
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
      <div className="editor-message">{report ? `${report.passed} passed, ${report.failed} failed` : message}</div>
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
