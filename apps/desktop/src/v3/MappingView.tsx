import { useCallback, useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  Circle,
  Clock3,
  GitMerge,
  RefreshCw,
  Search,
  Sparkles,
} from "lucide-react";
import {
  getMappingMetadata,
  getPhotoTaxonMatch,
  listPhotosByMappingStatus,
  startPhotoMapping,
  waitForOperation,
  type MappingListStatus,
  type MappingMetadata,
  type PhotoMappingListItem,
  type PhotoTaxonMatch,
} from "./api";
import {
  BusyState,
  EmptyState,
  PanelTitle,
  PhotoPreview,
  Tabs,
  errorMessage,
} from "./components";

type MappingMode =
  | "Overview"
  | "Unmapped"
  | "Unmatched"
  | "Ambiguous"
  | "Matched"
  | "Processing";

const mappingModes = [
  "Overview",
  "Unmapped",
  "Unmatched",
  "Ambiguous",
  "Matched",
  "Processing",
] as const;

const statusByMode: Record<Exclude<MappingMode, "Overview">, MappingListStatus> = {
  Unmapped: "unmapped",
  Unmatched: "unmatched",
  Ambiguous: "ambiguous",
  Matched: "matched",
  Processing: "processing",
};

const emptyMetadata: MappingMetadata = {
  mapped_photo_count: 0,
  unmatched_photo_count: 0,
  ambiguous_photo_count: 0,
  processing_photo_count: 0,
  mapping_taxa_count: 0,
};

export function MappingView({
  onStatus,
}: {
  onStatus: (message: string, busy?: boolean) => void;
}) {
  const [mode, setMode] = useState<MappingMode>("Overview");
  const [metadata, setMetadata] = useState<MappingMetadata>(emptyMetadata);
  const [items, setItems] = useState<PhotoMappingListItem[]>([]);
  const [selected, setSelected] = useState<PhotoMappingListItem | null>(null);
  const [match, setMatch] = useState<PhotoTaxonMatch | null>(null);
  const [filter, setFilter] = useState("");
  const [loading, setLoading] = useState(true);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState("");

  const loadMetadata = useCallback(async () => {
    const nextMetadata = await getMappingMetadata();
    setMetadata(nextMetadata);
    return nextMetadata;
  }, []);

  const loadMode = useCallback(async (nextMode: MappingMode) => {
    if (nextMode === "Overview") {
      setItems([]);
      setSelected(null);
      return;
    }
    const page = await listPhotosByMappingStatus(statusByMode[nextMode]);
    setItems(page.items);
    setSelected(page.items[0] ?? null);
  }, []);

  useEffect(() => {
    setLoading(true);
    setError("");
    Promise.all([loadMetadata(), loadMode(mode)])
      .then(([nextMetadata]) => {
        const total =
          nextMetadata.mapped_photo_count +
          nextMetadata.unmatched_photo_count +
          nextMetadata.ambiguous_photo_count +
          nextMetadata.processing_photo_count;
        onStatus(`${total} mapped records`);
      })
      .catch((nextError) => {
        const message = errorMessage(nextError);
        setError(message);
        onStatus(message);
      })
      .finally(() => setLoading(false));
  }, [loadMetadata, loadMode, mode, onStatus]);

  useEffect(() => {
    setMatch(null);
    if (!selected) {
      return;
    }
    getPhotoTaxonMatch(selected.photo.photo_id)
      .then(setMatch)
      .catch(() => setMatch(null));
  }, [selected]);

  const filteredItems = useMemo(() => {
    const query = filter.trim().toLocaleLowerCase();
    if (!query) {
      return items;
    }
    return items.filter((item) =>
      `${item.photo.filename} ${item.photo.relative_path}`
        .toLocaleLowerCase()
        .includes(query),
    );
  }, [filter, items]);

  async function runMapping() {
    setRunning(true);
    setError("");
    onStatus("Running mapping", true);
    try {
      const started = await startPhotoMapping();
      await waitForOperation("mapping", started.operation.task_id, (operation) => {
        onStatus(operation.message, true);
      });
      await Promise.all([loadMetadata(), loadMode(mode)]);
      onStatus("Mapping complete");
    } catch (nextError) {
      const message = errorMessage(nextError);
      setError(message);
      onStatus(message);
    } finally {
      setRunning(false);
    }
  }

  return (
    <section className="module-view">
      <div className="topbar">
        <label className="command-field">
          <Search size={14} />
          <input
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
            placeholder="Filter photos"
            aria-label="Filter mappings"
          />
        </label>
        <button
          className="ghost-button"
          type="button"
          onClick={() => void runMapping()}
          disabled={running}
        >
          <RefreshCw size={13} />
          {running ? "Running" : "Run"}
        </button>
      </div>
      <Tabs items={mappingModes} value={mode} onChange={setMode} />
      <div className="workspace-grid mapping-grid">
        <aside className="panel status-panel">
          <PanelTitle>Status</PanelTitle>
          <div className="status-stack">
            <StatusRow
              icon={Circle}
              label="Unmapped"
              value={null}
              active={mode === "Unmapped"}
              onClick={() => setMode("Unmapped")}
            />
            <StatusRow
              icon={Sparkles}
              label="Unmatched"
              value={metadata.unmatched_photo_count}
              tone="blue"
              active={mode === "Unmatched"}
              onClick={() => setMode("Unmatched")}
            />
            <StatusRow
              icon={AlertTriangle}
              label="Ambiguous"
              value={metadata.ambiguous_photo_count}
              tone="amber"
              active={mode === "Ambiguous"}
              onClick={() => setMode("Ambiguous")}
            />
            <StatusRow
              icon={CheckCircle2}
              label="Matched"
              value={metadata.mapped_photo_count}
              tone="green"
              active={mode === "Matched"}
              onClick={() => setMode("Matched")}
            />
            <StatusRow
              icon={Clock3}
              label="Processing"
              value={metadata.processing_photo_count}
              tone="purple"
              active={mode === "Processing"}
              onClick={() => setMode("Processing")}
            />
          </div>
        </aside>

        <main className="panel mapping-panel">
          <PanelTitle trailing={<span className="counter">{filteredItems.length}</span>}>
            {mode}
          </PanelTitle>
          {loading || running ? (
            <BusyState label={running ? "Mapping photos" : "Loading mappings"} />
          ) : error ? (
            <EmptyState title="Unable to load mappings" detail={error} />
          ) : mode === "Overview" ? (
            <EmptyState
              icon={GitMerge}
              title="Photo to taxon mapping"
              detail={`${metadata.mapping_taxa_count} taxa are currently connected to photos.`}
              action={
                <button className="primary-button" type="button" onClick={() => void runMapping()}>
                  Run mapping
                </button>
              }
            />
          ) : filteredItems.length === 0 ? (
            <EmptyState
              icon={CheckCircle2}
              title={`No ${mode.toLocaleLowerCase()} photos`}
              detail="The current status has no records."
            />
          ) : (
            <div className="mapping-table">
              {filteredItems.map((item) => (
                <button
                  className={`mapping-row${selected?.photo.photo_id === item.photo.photo_id ? " active" : ""}`}
                  key={item.photo.photo_id}
                  type="button"
                  onClick={() => setSelected(item)}
                >
                  <span>{item.photo.filename}</span>
                  <span className={`pill ${toneForStatus(item.mapping?.status)}`}>
                    {item.mapping?.status ?? "unmapped"}
                  </span>
                  <span>{item.photo.relative_path}</span>
                </button>
              ))}
            </div>
          )}
        </main>

        <aside className="panel bridge-panel">
          <PanelTitle>Bridge</PanelTitle>
          <PhotoPreview photo={selected?.photo ?? null} />
          {selected && (
            <div className="candidate-list">
              <span className="section-label">Candidates</span>
              {match?.candidates.length ? (
                match.candidates.slice(0, 5).map((candidate) => (
                  <div className="candidate-row" key={candidate.summary.taxon_id}>
                    <strong>
                      {candidate.accepted_names.scientific ??
                        candidate.accepted_names.english ??
                        `Taxon ${candidate.summary.taxon_id}`}
                    </strong>
                    <span>{candidate.summary.rank}</span>
                  </div>
                ))
              ) : (
                <span className="muted-copy">No candidate taxa</span>
              )}
            </div>
          )}
        </aside>
      </div>
    </section>
  );
}

function StatusRow({
  icon: Icon,
  label,
  value,
  tone = "gray",
  active,
  onClick,
}: {
  icon: typeof Circle;
  label: string;
  value: number | null;
  tone?: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      className={`status-card ${tone}${active ? " active" : ""}`}
      type="button"
      onClick={onClick}
    >
      <Icon size={14} />
      <span>{label}</span>
      <strong>{value ?? "-"}</strong>
    </button>
  );
}

function toneForStatus(status: string | undefined): string {
  if (status === "matched") {
    return "green";
  }
  if (status === "ambiguous") {
    return "amber";
  }
  if (status === "unmatched") {
    return "blue";
  }
  if (status === "processing" || status === "stale") {
    return "purple";
  }
  return "gray";
}
