import { call } from "./client";
import { demoSqlInput, type AddSqlInputResult, type PersistentSqlInput, type RemoveSqlInputResult, type SqlStatementMessage } from "./customSql";
import { demoOperation, type OperationState } from "./tasks";

export type BaseImportExecutionResult = {
  statements_executed: number;
  messages: SqlStatementMessage[];
  script_saved: boolean;
  warnings: string[];
};
export type BaseImportIssue = {
  code: string;
  message: string;
  taxon_id: number | null;
  related_taxon_id: number | null;
  table: string | null;
  row_identifier: string | null;
};
export type BaseImportValidationResult = {
  valid: boolean;
  can_apply: boolean;
  taxa_count: number;
  name_counts: Array<{ name_type: string; count: number }>;
  normalization_changes: number;
  total_warning_count: number;
  total_error_count: number;
  warnings: BaseImportIssue[];
  errors: BaseImportIssue[];
};
export type ValidateBaseImportResult = {
  execution: BaseImportExecutionResult;
  validation: BaseImportValidationResult;
  warnings: string[];
  can_apply: boolean;
};
export type TaxonomyBaseMetadata = {
  source_path: string;
  taxa_count: number;
  taxon_names_count: number;
  imported_at: string;
};
export type TaxonomyBaseReplaceResult = { metadata: TaxonomyBaseMetadata; warnings: string[] };

export const getBaseImportSql = () => call<string>("get_base_import_sql", undefined, () => [
  "ATTACH DATABASE 'vividarium_base.db' AS base;",
  "CREATE TABLE base.taxa AS SELECT * FROM biolib.taxa;",
  "CREATE TABLE base.taxon_names AS SELECT * FROM biolib.taxon_names;",
].join("\n"));
export const listBaseImportInputs = () =>
  call<PersistentSqlInput[]>("list_base_import_inputs", undefined, () => []);
export const addBaseImportInput = (kind: "csv" | "sqlite", alias: string, path: string) =>
  call<AddSqlInputResult>("add_base_import_input", { request: { kind, alias, path } }, () => {
    const input = demoSqlInput(kind, alias, path);
    return { input, inputs: [input], warnings: [] };
  });
export const removeBaseImportInput = (alias: string) =>
  call<RemoveSqlInputResult>("remove_base_import_input", { request: { alias } }, () => ({ inputs: [], warnings: [] }));
export const startBaseImportValidation = (sql: string) =>
  call<OperationState>("start_base_import_validation", { request: { sql } }, () => {
    const operation = demoOperation("base_import", "ready_to_apply");
    operation.operation = "validate_base_import";
    operation.result = {
      execution: {
        statements_executed: sql.split(";").filter(Boolean).length,
        messages: [{ statement_index: 1, affected_rows: null, message: "Script completed" }],
        script_saved: true,
        warnings: [],
      },
      validation: {
        valid: true,
        can_apply: true,
        taxa_count: 125000,
        name_counts: [{ name_type: "sci_name", count: 125000 }, { name_type: "synonym", count: 60000 }],
        normalization_changes: 0,
        total_warning_count: 0,
        total_error_count: 0,
        warnings: [],
        errors: [],
      },
      warnings: [],
      can_apply: true,
    } satisfies ValidateBaseImportResult;
    return operation;
  });
export const applyBaseImport = () => call<OperationState>("apply_base_import", undefined, () => {
  const operation = demoOperation("base_import", "Base import applied");
  operation.result = {
    metadata: {
      source_path: "demo-base.db",
      taxa_count: 125000,
      taxon_names_count: 185000,
      imported_at: new Date().toISOString(),
    },
    warnings: [],
  } satisfies TaxonomyBaseReplaceResult;
  return operation;
});
export const replaceTaxonomyBaseDatabase = (sourcePath: string) =>
  call<OperationState>("replace_taxonomy_base_database", { sourcePath }, () => {
    const operation = demoOperation("base_import", "Taxonomy database imported");
    operation.operation = "replace_taxonomy_base_database";
    operation.result = {
      metadata: {
        source_path: sourcePath,
        taxa_count: 125000,
        taxon_names_count: 185000,
        imported_at: new Date().toISOString(),
      },
      warnings: [],
    } satisfies TaxonomyBaseReplaceResult;
    return operation;
  });
export const getTaxonomyBaseMetadata = () =>
  call<TaxonomyBaseMetadata | null>("get_taxonomy_base_metadata", undefined, () => null);
