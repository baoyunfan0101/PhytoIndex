import type { PersistentSqlInput } from "../../api/customSql";

const reservedAliases = new Set([
  "main",
  "temp",
  "sql_import",
  "metadata",
  "taxonomy",
  "active_photo_library",
]);

export function suggestedSqlInputAlias(path: string, inputs: PersistentSqlInput[]): string {
  const stem = path.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, "") ?? "source";
  const normalized = stem.replace(/[^A-Za-z0-9_]/g, "_").replace(/^[^A-Za-z_]/, "_$&") || "source";
  let candidate = normalized;
  let suffix = 2;
  while (aliasUnavailable(candidate, inputs)) {
    candidate = `${normalized}_${suffix}`;
    suffix += 1;
  }
  return candidate;
}

export function sqlInputAliasError(alias: string, inputs: PersistentSqlInput[]): string {
  const trimmed = alias.trim();
  if (!trimmed) return "SQL access name is required.";
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(trimmed)) {
    return "Use letters, numbers, and underscores, starting with a letter or underscore.";
  }
  if (reservedAliases.has(trimmed.toLowerCase())) return "This SQL access name is reserved.";
  if (inputs.some((input) => input.alias.toLowerCase() === trimmed.toLowerCase())) {
    return "This SQL access name is already in use.";
  }
  return "";
}

function aliasUnavailable(alias: string, inputs: PersistentSqlInput[]): boolean {
  return reservedAliases.has(alias.toLowerCase())
    || inputs.some((input) => input.alias.toLowerCase() === alias.toLowerCase());
}
