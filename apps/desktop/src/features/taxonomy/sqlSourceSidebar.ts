import type { PersistentSqlInput, SqlSourceSchema } from "../../api/customSql";

export type SqlSourceGroup = "inputs" | "tables";

export function toggleSqlSourceGroup(
  current: SqlSourceGroup | null,
  selected: SqlSourceGroup,
): SqlSourceGroup | null {
  return current === selected ? null : selected;
}

export function accessibleSqlSchemas(
  inputs: PersistentSqlInput[],
  databaseSchemas: SqlSourceSchema[],
): SqlSourceSchema[] {
  return [
    ...databaseSchemas,
    ...inputs.filter((input) => input.available).map((input) => input.schema),
  ];
}
