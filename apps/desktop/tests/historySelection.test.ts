import assert from "node:assert/strict";
import test from "node:test";
import type { OperationSummary } from "../src/api/operations.ts";
import {
  canExportReplayableInput,
  canRollbackOperations,
  formatAuditJson,
  getReplayableOperations,
  getRollbackOrder,
  getSelectedOperations,
} from "../src/features/operations/historySelection.ts";

function operation(
  operationId: number,
  overrides: Partial<OperationSummary> = {},
): OperationSummary {
  return {
    operation_id: operationId,
    kind: "formatted_update",
    source: "formatted_update",
    applied_at: "2026-08-09 10:30:00",
    total_items: 1,
    succeeded_items: 1,
    failed_items: 0,
    rollbackable: true,
    has_formatted_input: true,
    ...overrides,
  };
}

test("keeps selected operations in the loaded list order", () => {
  const operations = [operation(9), operation(7), operation(4)];
  assert.deepEqual(
    getSelectedOperations(operations, [4, 9]).map((item) => item.operation_id),
    [9, 4],
  );
});

test("orders batch rollback from newest to oldest", () => {
  assert.deepEqual(
    getRollbackOrder([operation(4), operation(12), operation(7)])
      .map((item) => item.operation_id),
    [12, 7, 4],
  );
});

test("requires every selected operation to support an action", () => {
  assert.equal(canRollbackOperations([]), false);
  assert.equal(canRollbackOperations([operation(1), operation(2)]), true);
  assert.equal(canRollbackOperations([operation(1, { rollbackable: false })]), false);
  assert.equal(canExportReplayableInput([operation(1), operation(2)]), true);
  assert.equal(
    canExportReplayableInput([operation(1), operation(2, { has_formatted_input: false })]),
    true,
  );
  assert.equal(canExportReplayableInput([operation(2, { has_formatted_input: false })]), false);
  assert.deepEqual(
    getReplayableOperations([
      operation(1),
      operation(2, { has_formatted_input: false }),
      operation(3),
    ]).map((item) => item.operation_id),
    [1, 3],
  );
});

test("formats audit state as readable indented JSON", () => {
  assert.equal(formatAuditJson(null), "null");
  assert.equal(formatAuditJson({ name: "Canis", nested: { id: 4 } }), [
    "{",
    "  \"name\": \"Canis\",",
    "  \"nested\": {",
    "    \"id\": 4",
    "  }",
    "}",
  ].join("\n"));
});
