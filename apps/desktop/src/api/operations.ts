import { call } from "./client";
import type { Page } from "./common";
import { getTaxonomyTemplate } from "./taxonomy";

export type OperationSummary = {
  operation_id: number;
  kind: string;
  source: string;
  applied_at: string;
  total_items: number;
  succeeded_items: number;
  failed_items: number;
  rollbackable: boolean;
  has_formatted_input: boolean;
};
export type OperationAuditRow = {
  operation_id: number;
  sequence: number;
  entity_type: string;
  entity_id: string | null;
  action: string;
  before_json: unknown;
  after_json: unknown;
  succeeded: boolean;
  message: string;
};

function demoSummaries(domain: "photo" | "taxonomy"): OperationSummary[] {
  return [1, 2, 3].map((operationId) => ({
    operation_id: operationId,
    kind: domain === "photo" ? "rename" : operationId === 3 ? "custom_sql" : "formatted_update",
    source: domain === "photo" ? "manual_rename" : operationId === 3 ? "custom_sql" : "formatted_update",
    applied_at: `2026-07-${20 + operationId} 10:30:00`,
    total_items: operationId + 1,
    succeeded_items: operationId + 1,
    failed_items: 0,
    rollbackable: true,
    has_formatted_input: domain === "taxonomy" && operationId !== 3,
  }));
}

export const listPhotoOperationSummaries = (cursor: string | null = null, limit = 80) =>
  call<Page<OperationSummary>>("list_photo_operations", { cursor, limit }, () => ({
    items: demoSummaries("photo"), next_cursor: null,
  }));
export const listPhotoOperationAudit = (operationId: number, cursor: string | null = null, limit = 80) =>
  call<Page<OperationAuditRow>>("list_photo_operation_audit", { operationId, cursor, limit }, () => ({
    items: [{
      operation_id: operationId,
      sequence: 1,
      entity_type: "photo",
      entity_id: "1",
      action: "rename",
      before_json: { directory_relative_path: "Mammalia", filename: "before.jpg" },
      after_json: { directory_relative_path: "Mammalia", filename: "after.jpg" },
      succeeded: true,
      message: "Renamed",
    }],
    next_cursor: null,
  }));
export const rollbackPhotoOperation = (operationId: number) =>
  call<void>("rollback_photo_operation", { operationId }, () => undefined);
export const exportPhotoOperationAudit = (operationId: number, destinationPath: string) =>
  call<void>("export_photo_operation_audit", { operationId, destinationPath }, () => undefined);
export const exportAllPhotoOperationAudit = (destinationPath: string) =>
  call<void>("export_all_photo_operation_audit", { destinationPath }, () => undefined);

export const listTaxonomyOperationSummaries = (cursor: string | null = null, limit = 80) =>
  call<Page<OperationSummary>>("list_taxonomy_operations", { cursor, limit }, () => ({
    items: demoSummaries("taxonomy"), next_cursor: null,
  }));
export const listTaxonomyOperationAudit = (operationId: number, cursor: string | null = null, limit = 80) =>
  call<Page<OperationAuditRow>>("list_taxonomy_operation_audit", { operationId, cursor, limit }, () => ({
    items: [{
      operation_id: operationId,
      sequence: 1,
      entity_type: "taxon_name",
      entity_id: "10",
      action: "update",
      before_json: { name: "Before" },
      after_json: { name: "After" },
      succeeded: true,
      message: "Updated",
    }],
    next_cursor: null,
  }));
export const rollbackTaxonomyOperation = (operationId: number) =>
  call<void>("rollback_taxonomy_operation", { operationId }, () => undefined);
export const exportTaxonomyOperationAudit = (operationId: number, destinationPath: string) =>
  call<void>("export_taxonomy_operation_audit", { operationId, destinationPath }, () => undefined);
export const exportAllTaxonomyOperationAudit = (destinationPath: string) =>
  call<void>("export_all_taxonomy_operation_audit", { destinationPath }, () => undefined);
export const exportTaxonomyOperationInput = (operationId: number) =>
  call<string>("export_taxonomy_operation_input", { operationId }, getTaxonomyTemplate);
export const exportAllReplayableTaxonomyInputs = () =>
  call<string>("export_all_replayable_taxonomy_inputs", undefined, getTaxonomyTemplate);
