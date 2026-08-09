import assert from "node:assert/strict";
import test from "node:test";
import { EditorState } from "@codemirror/state";
import {
  externalValueTransaction,
  externalValueUpdate,
  languageExtension,
} from "../src/shared/codeEditorSupport.ts";

test("creates editor states for every supported language", () => {
  for (const language of ["sql", "json", "rhai"] as const) {
    const state = EditorState.create({
      doc: "line one\nline two",
      extensions: [languageExtension(language)],
    });
    assert.equal(state.doc.toString(), "line one\nline two");
  }
});

test("external value synchronization skips equal values", () => {
  const state = EditorState.create({ doc: "SELECT 1;" });
  assert.equal(externalValueTransaction(state, "SELECT 1;"), null);
});

test("external value synchronization replaces multiline and empty values without feedback", () => {
  const initial = EditorState.create({ doc: "old" });
  const multiline = initial.update(externalValueTransaction(initial, "one\ntwo")!);
  assert.equal(multiline.state.doc.toString(), "one\ntwo");
  assert.equal(multiline.annotation(externalValueUpdate), true);

  const emptySpec = externalValueTransaction(multiline.state, "");
  const empty = multiline.state.update(emptySpec!);
  assert.equal(empty.state.doc.toString(), "");
  assert.equal(empty.annotation(externalValueUpdate), true);
});

test("CodeMirror read-only state exposes its protected state", () => {
  const state = EditorState.create({
    doc: "unchanged",
    extensions: [EditorState.readOnly.of(true)],
  });
  assert.equal(state.facet(EditorState.readOnly), true);
});
