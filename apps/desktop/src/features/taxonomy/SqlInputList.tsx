import { Database, FilePlus2, Table2, Trash2 } from "lucide-react";
import { selectCsvFile, selectSqliteDatabase } from "../../api/dialogs";
import type { PersistentSqlInput } from "../../api/customSql";
import { IconButton } from "../../shared/ui";

export function SqlInputList({
  inputs,
  busy,
  onAdd,
  onRemove,
}: {
  inputs: PersistentSqlInput[];
  busy: boolean;
  onAdd: (kind: "csv" | "sqlite", alias: string, path: string) => Promise<void>;
  onRemove: (input: PersistentSqlInput) => Promise<void>;
}) {
  async function add(kind: "csv" | "sqlite") {
    const path = kind === "csv" ? await selectCsvFile() : await selectSqliteDatabase();
    if (!path) return;
    const alias = window.prompt("SQL access name", suggestedAlias(path, inputs))?.trim();
    if (!alias) return;
    await onAdd(kind, alias, path);
  }

  return (
    <aside className="sql-sources">
      <header className="sql-input-actions">
        <strong>Data Sources</strong>
        <IconButton aria-label="Add CSV" size="small" disabled={busy} title="Add CSV" onClick={() => void add("csv")}>
          <FilePlus2 size={13} />
        </IconButton>
        <IconButton aria-label="Add SQLite" size="small" disabled={busy} title="Add SQLite" onClick={() => void add("sqlite")}>
          <Database size={13} />
        </IconButton>
      </header>
      {inputs.length === 0 && <span>No data sources</span>}
      {inputs.map((input) => (
        <section className="base-source" key={input.alias}>
          <header>
            <span><b>{input.alias}</b><i>{input.kind.toUpperCase()}</i></span>
            <IconButton
              aria-label={`Remove ${input.alias}`}
              size="small"
              title={`Remove ${input.alias}`}
              disabled={busy}
              onClick={() => void onRemove(input)}
            >
              <Trash2 size={12} />
            </IconButton>
          </header>
          <code title={input.original_path}>{input.original_path}</code>
          <small className={input.available ? "available" : "unavailable"}>
            {input.available ? "Stored copy available" : "Stored copy unavailable"}
          </small>
          {input.schema.objects.map((object) => (
            <details key={`${input.alias}:${object.name}`}>
              <summary><Table2 size={12} />{object.name}</summary>
              {object.columns.map((column) => (
                <span key={column.name}>{column.name}<i>{column.declared_type ?? "untyped"}</i></span>
              ))}
            </details>
          ))}
        </section>
      ))}
    </aside>
  );
}

function suggestedAlias(path: string, existing: PersistentSqlInput[]): string {
  const stem = path.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, "") ?? "source";
  const normalized = stem.replace(/[^A-Za-z0-9_]/g, "_").replace(/^[^A-Za-z_]/, "_$&") || "source";
  let candidate = normalized;
  let suffix = 2;
  while (existing.some((input) => input.alias.toLowerCase() === candidate.toLowerCase())) {
    candidate = `${normalized}_${suffix}`;
    suffix += 1;
  }
  return candidate;
}
