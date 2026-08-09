import type { SqlSourceSchema } from "../../api/customSql";

export type SqlSourceGroup = "inputs" | "tables";

export function toggleSqlSourceGroup(
  current: SqlSourceGroup | null,
  selected: SqlSourceGroup,
): SqlSourceGroup | null {
  return current === selected ? null : selected;
}

export function internalDatabaseSchemas(databaseSchemas: SqlSourceSchema[]): SqlSourceSchema[] {
  return databaseSchemas;
}
