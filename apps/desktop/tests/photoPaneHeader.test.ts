import assert from "node:assert/strict";
import test from "node:test";
import { photoPaneHeaderLines } from "../src/features/photos/photoFormatting.ts";

test("uses filename and file metadata for every photo pane header", () => {
  const lines = photoPaneHeaderLines({
    filename: "Panthera_leo_002.jpg",
    file_size: 13_002_342,
    modified_at_ns: Date.UTC(2026, 7, 22, 14, 30) * 1_000_000,
  });

  assert.equal(lines.filename, "Panthera_leo_002.jpg");
  assert.match(lines.summary, /^12\.4 MB \u00b7 /);
  assert.doesNotMatch(lines.summary, /Panthera|Felidae|Taxon/);
});
