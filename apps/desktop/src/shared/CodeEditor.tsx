import { useRef, type ReactNode, type UIEvent } from "react";

type CodeLanguage = "json" | "rhai" | "sql";

const keywordSets: Record<CodeLanguage, Set<string>> = {
  json: new Set(["false", "null", "true"]),
  rhai: new Set([
    "break", "continue", "else", "false", "fn", "for", "if", "in", "let",
    "private", "return", "switch", "throw", "true", "while",
  ]),
  sql: new Set([
    "alter", "and", "as", "begin", "by", "case", "commit", "create", "delete",
    "distinct", "drop", "else", "end", "from", "group", "having", "in", "insert",
    "into", "is", "join", "limit", "not", "null", "on", "or", "order", "rollback",
    "select", "set", "table", "then", "union", "update", "values", "when", "where",
  ]),
};

const rhaiFunctions = new Set([
  "contains", "ends_with", "get", "index_of", "is_ascii_alpha", "is_uppercase",
  "is_whitespace", "len", "normalize_name", "replace", "starts_with",
  "sub_string", "trim",
]);

const tokenPattern =
  /(\/\/[^\n]*|--[^\n]*|\/\*[\s\S]*?\*\/|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|\b\d+(?:\.\d+)?\b|\b[A-Za-z_][A-Za-z0-9_]*\b)/g;

export function CodeEditor({
  language,
  value,
  onChange,
  ariaLabel,
  autoGrow,
}: {
  language: CodeLanguage;
  value: string;
  onChange: (value: string) => void;
  ariaLabel: string;
  autoGrow?: { minRows: number; maxRows: number };
}) {
  const highlightRef = useRef<HTMLPreElement>(null);
  const rowCount = value.split("\n").length;
  const autoHeight = autoGrow
    ? Math.min(autoGrow.maxRows, Math.max(autoGrow.minRows, rowCount)) * 19.8 + 24
    : undefined;

  function syncScroll(event: UIEvent<HTMLTextAreaElement>) {
    if (!highlightRef.current) return;
    highlightRef.current.scrollTop = event.currentTarget.scrollTop;
    highlightRef.current.scrollLeft = event.currentTarget.scrollLeft;
  }

  return (
    <div className={`code-editor language-${language}`} style={autoHeight ? { height: `${autoHeight}px` } : undefined}>
      <pre ref={highlightRef} aria-hidden="true">
        <code>{highlight(value, language)}{"\n"}</code>
      </pre>
      <textarea
        aria-label={ariaLabel}
        spellCheck={false}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        onScroll={syncScroll}
      />
    </div>
  );
}

function highlight(value: string, language: CodeLanguage): ReactNode[] {
  const output: ReactNode[] = [];
  let cursor = 0;
  tokenPattern.lastIndex = 0;
  for (let match = tokenPattern.exec(value); match; match = tokenPattern.exec(value)) {
    if (match.index > cursor) output.push(value.slice(cursor, match.index));
    const token = match[0];
    output.push(
      <span className={`syntax-${classify(token, language)}`} key={`${match.index}:${token}`}>
        {token}
      </span>,
    );
    cursor = match.index + token.length;
  }
  if (cursor < value.length) output.push(value.slice(cursor));
  return output;
}

function classify(token: string, language: CodeLanguage): string {
  if (token.startsWith("//") || token.startsWith("--") || token.startsWith("/*")) {
    return "comment";
  }
  if (token.startsWith("\"") || token.startsWith("'")) return "string";
  if (/^\d/.test(token)) return "number";
  if (keywordSets[language].has(token.toLocaleLowerCase())) return "keyword";
  if (language === "rhai" && rhaiFunctions.has(token)) return "function";
  return "identifier";
}
