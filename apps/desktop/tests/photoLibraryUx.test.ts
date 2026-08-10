import assert from "node:assert/strict";
import test from "node:test";
import type { OperationState } from "../src/api/tasks.ts";
import { photoCacheIdentity } from "../src/api/photoCacheIdentity.ts";
import { observedSuccessfulCompletion } from "../src/app/operationTransitions.ts";
import { shouldSwitchPhotoLibrary } from "../src/features/settings/photoLibraryUx.ts";
import { describePhotoOperation, photoOperationProgress, photoRenameSummaryFromOperation } from "../src/features/photos/photoOperation.ts";

function operation(overrides: Partial<OperationState> = {}): OperationState {
  return {
    module: "photos",
    task_id: "task-1",
    operation: "initial_index",
    running: true,
    started_at: "2026-08-09 22:00:00",
    finished_at: null,
    message: "Indexing Photo Library",
    processed: 0,
    total: null,
    progress: null,
    result: null,
    error: null,
    ...overrides,
  };
}

test("a Photo Library card switches only when it is inactive and the settings view is idle", () => {
  assert.equal(shouldSwitchPhotoLibrary(false, true, false), true);
  assert.equal(shouldSwitchPhotoLibrary(true, true, false), false);
  assert.equal(shouldSwitchPhotoLibrary(false, false, false), false);
  assert.equal(shouldSwitchPhotoLibrary(false, true, true), false);
});

test("photo operation progress presents known totals", () => {
  const value = operation({
    progress: {
      stage: "Indexing photos",
      current: 1200,
      total: 5000,
      statement_index: null,
      statement_total: null,
    },
  });
  assert.equal(describePhotoOperation(value), "Indexing photos: 1,200 / 5,000");
  assert.deepEqual(photoOperationProgress(value), { value: 1200, max: 5000 });
});

test("photo operation progress supports phase-only updates", () => {
  const value = operation();
  assert.equal(describePhotoOperation(value), "Indexing Photo Library");
  assert.equal(photoOperationProgress(value), null);
});

test("photo media cache identity includes the active Photo Library", () => {
  const photo = {
    photo_id: 7,
    directory_id: 2,
    relative_path: "flowers/example.jpg",
    filename: "example.jpg",
    file_size: 4096,
    modified_at_ns: 123456,
    thumbnail_path: null,
  };
  assert.equal(photoCacheIdentity(photo, "library-a"), "library-a:123456:4096");
  assert.notEqual(
    photoCacheIdentity(photo, "library-a"),
    photoCacheIdentity(photo, "library-b"),
  );
});

test("a photo operation completed entirely between polls still invalidates photo views", () => {
  const idle = operation({ task_id: null, operation: null, running: false });
  const completed = operation({
    running: false,
    finished_at: "2026-08-09 22:00:01",
    message: "completed",
  });
  assert.equal(observedSuccessfulCompletion(idle, completed), true);
  assert.equal(observedSuccessfulCompletion(completed, completed), false);
  assert.equal(observedSuccessfulCompletion(idle, { ...completed, error: "failed" }), false);
});

test("reads the completed bulk rename result from the photo operation", () => {
  const result = {
    operation_id: 12,
    total: 3,
    applied: 2,
    no_change: 1,
    failed: 0,
  };
  assert.deepEqual(
    photoRenameSummaryFromOperation(operation({ running: false, result })),
    result,
  );
  assert.throws(
    () => photoRenameSummaryFromOperation(operation({ running: false, error: "rename failed" })),
    /rename failed/,
  );
});
