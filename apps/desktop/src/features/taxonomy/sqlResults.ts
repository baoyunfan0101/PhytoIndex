import type { CustomSqlExecutionResult } from "../../api/customSql";

type SqlExportPreview = Pick<CustomSqlExecutionResult, "messages" | "result_sets">;

export function canExportFullQuery(result: SqlExportPreview): boolean {
  return result.result_sets.some((resultSet) => {
    if (!resultSet.truncated) return false;
    const message = result.messages.find(
      (candidate) => candidate.statement_index === resultSet.statement_index,
    );
    return message?.affected_rows === null;
  });
}
