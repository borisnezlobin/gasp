// Parsing model shared by every feature: turn raw markdown into the set of
// footnote references and definitions, with absolute offsets so callers can
// edit precisely. Footnote-like text inside code is masked out first so a
// `[^1]` in a code sample is never mistaken for a real footnote.

export interface FootnoteRef {
  label: string;
  start: number; // offset of the '[' in "[^label]"
  end: number; // offset just past the ']'
}

export interface FootnoteDef {
  label: string;
  lineStart: number; // offset of the first character of the definition's first line
  end: number; // offset just past the last character of the definition block (no trailing newline)
  indent: string; // leading whitespace on the first line
  body: string; // everything after "[^label]:" (includes the leading space and any continuation lines)
}

export interface InlineTypo {
  start: number; // offset of the '^' in "^[digits]"
  end: number;
  label: string; // the digits inside
}

export interface ParsedFootnotes {
  refs: FootnoteRef[];
  defs: FootnoteDef[];
  inlineTypos: InlineTypo[];
}

const REF_RE = /\[\^([^\[\]\n]+)\](?!:)/g;
const DEF_HEAD_RE = /^(\s*)\[\^([^\[\]\n]+)\]:/;
const INLINE_TYPO_RE = /\^\[(\d+)\]/g;

export function isNumeric(label: string): boolean {
  return /^\d+$/.test(label);
}

function blank(chars: string[], from: number, to: number): void {
  for (let i = from; i < to; i++) {
    if (chars[i] !== "\n") chars[i] = " ";
  }
}

// Return a string the same length as `text` with all fenced-code and inline-code
// regions replaced by spaces (newlines preserved), so offsets still line up.
export function maskCode(text: string): string {
  const chars = text.split("");
  const lines = text.split("\n");
  let inFence = false;
  let fenceChar = "";
  let offset = 0;
  for (const line of lines) {
    const fence = line.match(/^\s*(```+|~~~+)/);
    if (!inFence && fence) {
      inFence = true;
      fenceChar = fence[1][0];
      blank(chars, offset, offset + line.length);
    } else if (inFence && fence && fence[1][0] === fenceChar) {
      inFence = false;
      blank(chars, offset, offset + line.length);
    } else if (inFence) {
      blank(chars, offset, offset + line.length);
    }
    offset += line.length + 1;
  }
  let masked = chars.join("");
  masked = masked.replace(/`+[^`\n]*`+/g, (m) => " ".repeat(m.length));
  return masked;
}

export function parseFootnotes(text: string): ParsedFootnotes {
  const masked = maskCode(text);

  const refs: FootnoteRef[] = [];
  REF_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = REF_RE.exec(masked)) !== null) {
    refs.push({ label: m[1], start: m.index, end: m.index + m[0].length });
  }

  const inlineTypos: InlineTypo[] = [];
  INLINE_TYPO_RE.lastIndex = 0;
  while ((m = INLINE_TYPO_RE.exec(masked)) !== null) {
    inlineTypos.push({ label: m[1], start: m.index, end: m.index + m[0].length });
  }

  const defs: FootnoteDef[] = [];
  const lines = text.split("\n");
  const maskedLines = masked.split("\n");
  const lineStarts: number[] = [];
  let off = 0;
  for (const line of lines) {
    lineStarts.push(off);
    off += line.length + 1;
  }

  for (let i = 0; i < lines.length; i++) {
    const head = maskedLines[i].match(DEF_HEAD_RE);
    if (!head) continue;
    let j = i + 1;
    while (j < lines.length) {
      const next = maskedLines[j];
      if (next.trim() === "") break;
      if (DEF_HEAD_RE.test(next)) break;
      j++;
    }
    const lineStart = lineStarts[i];
    const end = lineStarts[j - 1] + lines[j - 1].length;
    const block = text.slice(lineStart, end);
    defs.push({
      label: head[2],
      lineStart,
      end,
      indent: head[1],
      body: block.slice(head[0].length),
    });
    i = j - 1;
  }

  return { refs, defs, inlineTypos };
}

export function maxNumericLabel(parsed: ParsedFootnotes): number {
  let max = 0;
  for (const r of parsed.refs) if (isNumeric(r.label)) max = Math.max(max, Number(r.label));
  for (const d of parsed.defs) if (isNumeric(d.label)) max = Math.max(max, Number(d.label));
  return max;
}

export function findDef(defs: FootnoteDef[], label: string): FootnoteDef | undefined {
  return defs.find((d) => d.label === label);
}

export function firstRef(refs: FootnoteRef[], label: string): FootnoteRef | undefined {
  return refs.find((r) => r.label === label);
}
