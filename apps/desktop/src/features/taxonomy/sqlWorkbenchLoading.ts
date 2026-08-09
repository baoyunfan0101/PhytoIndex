export type SqlWorkbenchLoadResult<T> = {
  sql?: string;
  inputs?: T[];
  error: string;
};

export function resolveSqlWorkbenchLoads<T>(
  sqlResult: PromiseSettledResult<string>,
  inputsResult: PromiseSettledResult<T[]>,
): SqlWorkbenchLoadResult<T> {
  const errors: string[] = [];
  const result: SqlWorkbenchLoadResult<T> = { error: "" };
  if (sqlResult.status === "fulfilled") result.sql = sqlResult.value;
  else errors.push(`SQL script: ${loadErrorMessage(sqlResult.reason)}`);
  if (inputsResult.status === "fulfilled") result.inputs = inputsResult.value;
  else errors.push(`Input sources: ${loadErrorMessage(inputsResult.reason)}`);
  result.error = errors.join(" ");
  return result;
}

function loadErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
