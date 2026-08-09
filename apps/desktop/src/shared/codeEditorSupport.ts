import { json } from "@codemirror/lang-json";
import { SQLite, sql } from "@codemirror/lang-sql";
import { StreamLanguage, type StreamParser } from "@codemirror/language";
import { Annotation, EditorState, type Extension, type TransactionSpec } from "@codemirror/state";
import { tags } from "@lezer/highlight";

export type CodeLanguage = "json" | "rhai" | "sql";

export const externalValueUpdate = Annotation.define<boolean>();

export function languageExtension(language: CodeLanguage): Extension {
  if (language === "sql") return sql({ dialect: SQLite });
  if (language === "json") return json();
  if (language === "rhai") return rhaiLanguage;
  throw new Error(`Unsupported CodeEditor language: ${String(language)}`);
}

export function externalValueTransaction(
  state: EditorState,
  value: string,
): TransactionSpec | null {
  if (state.doc.toString() === value) return null;
  return {
    annotations: externalValueUpdate.of(true),
    changes: { from: 0, to: state.doc.length, insert: value },
  };
}

type RhaiState = {
  blockComment: boolean;
  expectFunction: boolean;
};

const rhaiKeywords = new Set([
  "as", "break", "const", "continue", "do", "else", "export", "fn", "for",
  "if", "import", "in", "let", "loop", "private", "return", "switch", "throw",
  "try", "while",
]);

const rhaiParser: StreamParser<RhaiState> = {
  name: "rhai",
  startState: () => ({ blockComment: false, expectFunction: false }),
  tokenTable: {
    bracket: tags.bracket,
    functionName: tags.function(tags.variableName),
    operator: tags.operator,
  },
  token(stream, state) {
    if (state.blockComment) {
      if (stream.skipTo("*/")) {
        stream.match("*/");
        state.blockComment = false;
      } else {
        stream.skipToEnd();
      }
      return "blockComment";
    }
    if (stream.eatSpace()) return null;
    if (stream.match("//")) {
      stream.skipToEnd();
      return "lineComment";
    }
    if (stream.match("/*")) {
      state.blockComment = true;
      return "blockComment";
    }
    const quote = stream.peek();
    if (quote === "\"" || quote === "'") {
      stream.next();
      let escaped = false;
      for (let character = stream.next(); character !== undefined; character = stream.next()) {
        if (character === quote && !escaped) break;
        escaped = character === "\\" && !escaped;
        if (character !== "\\") escaped = false;
      }
      return "string";
    }
    if (stream.match(/^(?:0x[\da-fA-F]+|0b[01]+|\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)/)) {
      return "number";
    }
    if (stream.match(/^[A-Za-z_][A-Za-z0-9_]*/)) {
      const word = stream.current();
      if (rhaiKeywords.has(word)) {
        state.expectFunction = word === "fn";
        return "keyword";
      }
      if (word === "true" || word === "false") return "bool";
      if (state.expectFunction) {
        state.expectFunction = false;
        return "functionName";
      }
      return /^\s*\(/.test(stream.string.slice(stream.pos)) ? "functionName" : "variableName";
    }
    if (stream.match(/^[()[\]{}]/)) return "bracket";
    if (stream.match(/^(?:\+\+|--|==|!=|<=|>=|=>|&&|\|\||\?\?|\.\.|[+\-*\/%=<>!&|^?:.])/)) {
      return "operator";
    }
    state.expectFunction = false;
    stream.next();
    return null;
  },
};

const rhaiLanguage = StreamLanguage.define(rhaiParser);
