/** Just enough Markdown for the one-line-at-a-time demo editor. */

export type BlockKind = "h1" | "h2" | "task" | "bullet" | "quote" | "paragraph";

export type Block = {
  kind: BlockKind;
  /** The marker at the start of the line, such as `## ` or `- [ ] `. */
  marker: string;
  body: string;
  done: boolean;
};

const BLOCK_PATTERNS: [BlockKind, RegExp][] = [
  ["h1", /^# /],
  ["h2", /^## /],
  ["task", /^- \[[ xX]\] /],
  ["bullet", /^- /],
  ["quote", /^> /],
];

export function parseBlock(line: string): Block {
  for (const [kind, pattern] of BLOCK_PATTERNS) {
    const marker = line.match(pattern)?.[0];
    if (marker) {
      return { kind, marker, body: line.slice(marker.length), done: /\[[xX]\]/.test(marker) };
    }
  }
  return { kind: "paragraph", marker: "", body: line, done: false };
}

export function toggleTask(line: string): string {
  const block = parseBlock(line);
  if (block.kind !== "task") return line;
  return `${block.done ? "- [ ] " : "- [x] "}${block.body}`;
}

export type InlineKind = "text" | "strong" | "emphasis" | "highlight" | "code" | "link";

export type Inline = {
  kind: InlineKind;
  open: string;
  text: string;
  close: string;
};

const INLINE_PATTERN = /(\*\*[^*]+\*\*|==[^=]+==|`[^`]+`|\[\[[^\]]+\]\]|\*[^*\s][^*]*\*)/g;

const INLINE_MARKS: [InlineKind, string, string][] = [
  ["strong", "**", "**"],
  ["highlight", "==", "=="],
  ["code", "`", "`"],
  ["link", "[[", "]]"],
  ["emphasis", "*", "*"],
];

function inlineOf(token: string): Inline {
  const [kind, open, close] = INLINE_MARKS.find(
    ([, open, close]) => token.startsWith(open) && token.endsWith(close),
  ) ?? ["text", "", ""];
  return { kind, open, close, text: token.slice(open.length, token.length - close.length) };
}

/** Splitting on a capturing pattern alternates plain text and marked-up
    runs, so odd places hold the runs. */
export function parseInline(text: string): Inline[] {
  return text
    .split(INLINE_PATTERN)
    .map((part, index): Inline => (index % 2 === 1 ? inlineOf(part) : { kind: "text", open: "", close: "", text: part }))
    .filter((inline) => inline.open !== "" || inline.text !== "");
}
