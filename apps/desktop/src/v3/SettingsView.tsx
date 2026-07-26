import { ArrowDown, ArrowUp, Beaker, Save, Settings2 } from "lucide-react";
import { useEffect, useState } from "react";
import {
  errorMessage,
  checkAppUpdate,
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
  runNamingHookTests,
  setMapSettings,
  setNamingHook,
  setNamingHookTestCases,
  setPhotoFilenameFormatSettings,
  setPhotoNameMatchSettings,
  setTaxonomyNameSeparator,
  type MapSettings,
  type NamingHookKind,
  type NamingHookTestCase,
  type NamingHookTestReport,
  type PhotoFilenameFormatSettings,
  type PhotoNameField,
} from "./api";
import { SectionHeader, Segmented, VirtualList } from "./components";

type SettingsSection = "General" | "Naming" | "Map" | "Hooks";

export function SettingsView() {
  const [section, setSection] = useState<SettingsSection>("General");
  return (
    <div className="settings-workbench">
      <aside className="settings-nav">
        {(["General", "Naming", "Map", "Hooks"] as const).map((item) => (
          <button className={section === item ? "active" : ""} type="button" key={item} onClick={() => setSection(item)}>
            {item === "Hooks" ? <Beaker size={14} /> : <Settings2 size={14} />}{item}
          </button>
        ))}
      </aside>
      <main className="settings-content">
        {section === "General" && <GeneralSettings />}
        {section === "Naming" && <NamingSettings />}
        {section === "Map" && <MapSettingsPanel />}
        {section === "Hooks" && <HooksSettings />}
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
    Promise.all([getNamingHookSettings(), getNamingHookTemplates(), getNamingHookTestCases()]).then(([settings, nextTemplates, nextCases]) => {
      setTemplates(nextTemplates);
      setScripts({
        photo_filename: settings.photo_filename ?? nextTemplates.photo_filename,
        synonym_authority: settings.synonym_authority ?? nextTemplates.synonym_authority,
      });
      setCases(nextCases);
    });
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
        <div className="code-editor rhai-editor"><pre aria-hidden="true">{scripts[kind]}</pre><textarea spellCheck={false} value={scripts[kind]} onChange={(event) => setScripts({ ...scripts, [kind]: event.target.value })} /></div>
        <div className="hook-tests">
          <header><strong>Project tests</strong><button type="button" onClick={() => setCases({ ...cases, [kind]: [...cases[kind], { name: "New test", input: "", expected: { kind, output: {} } }] })}>+ Add</button></header>
          <VirtualList
            items={cases[kind]}
            rowHeight={104}
            itemKey={(_, index) => index}
            renderItem={(item, index) => (
              <div className={`hook-test-row${report?.cases[index] && !report.cases[index].passed ? " failed" : ""}`}>
                <input value={item.name} onChange={(event) => changeCase(cases, setCases, kind, index, { ...item, name: event.target.value })} />
                <input value={item.input} placeholder="Raw input" onChange={(event) => changeCase(cases, setCases, kind, index, { ...item, input: event.target.value })} />
                <code>{report?.cases[index] ? JSON.stringify(report.cases[index].actual) : JSON.stringify(item.expected)}</code>
                <button type="button" onClick={() => setCases({ ...cases, [kind]: cases[kind].filter((_, itemIndex) => itemIndex !== index) })}>Delete</button>
              </div>
            )}
          />
        </div>
      </div>
      <div className="editor-message">{report ? `${report.passed} passed, ${report.failed} failed` : message}</div>
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
