// The renumbering invariant: numeric footnotes are numbered 1..N by order of
// first appearance in the body, with the definition block listed in that same
// order. computeRenumber is pure — it returns the edits needed to reach that
// state.
//
// Crucially, it REFUSES to renumber when the document has structural problems
// (a reference with no definition, a definition nothing cites, a duplicate
// definition). Rearranging an inconsistent document is how footnotes get
// jumbled, so instead we leave it alone and let the editor highlighter show
// the user what to fix.

import { isNumeric, parseFootnotes, FootnoteDef } from "./footnotes";
import { classifyProblems, FootnoteProblem, hasBlockingProblems } from "./lint";

export interface Edit {
  from: number;
  to: number;
  text: string;
}

export interface RenumberResult {
  changed: boolean;
  edits: Edit[];
  newText: string;
  reorderedBlock: { start: number; end: number } | null;
  problems: FootnoteProblem[];
}

export function applyEdits(text: string, edits: Edit[]): string {
  const sorted = [...edits].sort((a, b) => a.from - b.from);
  let out = "";
  let cursor = 0;
  for (const e of sorted) {
    out += text.slice(cursor, e.from) + e.text;
    cursor = e.to;
  }
  out += text.slice(cursor);
  return out;
}

export function mapOffset(edits: Edit[], offset: number): number {
  const sorted = [...edits].sort((a, b) => a.from - b.from);
  let delta = 0;
  for (const e of sorted) {
    if (e.to <= offset) {
      delta += e.text.length - (e.to - e.from);
    } else if (e.from < offset && offset < e.to) {
      return e.from + delta + e.text.length;
    } else if (e.from >= offset) {
      break;
    }
  }
  return offset + delta;
}

function defsAreContiguous(text: string, defs: FootnoteDef[]): boolean {
  for (let i = 0; i + 1 < defs.length; i++) {
    if (text.slice(defs[i].end, defs[i + 1].lineStart).trim() !== "") return false;
  }
  return true;
}

export function computeRenumber(text: string): RenumberResult {
  const parsed = parseFootnotes(text);
  const problems = classifyProblems(parsed);
  const unchanged = (): RenumberResult => ({
    changed: false,
    edits: [],
    newText: text,
    reorderedBlock: null,
    problems,
  });

  if (hasBlockingProblems(problems)) return unchanged();

  const numRefs = parsed.refs.filter((r) => isNumeric(r.label));
  const numDefs = parsed.defs.filter((d) => isNumeric(d.label));

  // With no blocking problems, every numeric definition is referenced; order by
  // first body appearance.
  const order: string[] = [];
  const seen = new Set<string>();
  for (const r of numRefs) {
    if (!seen.has(r.label)) {
      seen.add(r.label);
      order.push(r.label);
    }
  }
  const map = new Map<string, string>();
  order.forEach((old, i) => map.set(old, String(i + 1)));

  const edits: Edit[] = [];
  for (const r of numRefs) {
    const nw = map.get(r.label);
    if (nw && nw !== r.label) edits.push({ from: r.start, to: r.end, text: `[^${nw}]` });
  }

  let reorderedBlock: { start: number; end: number } | null = null;
  const canReorder =
    parsed.defs.length > 0 &&
    parsed.defs.every((d) => isNumeric(d.label)) &&
    defsAreContiguous(text, parsed.defs);

  if (canReorder) {
    const start = parsed.defs[0].lineStart;
    const end = parsed.defs[parsed.defs.length - 1].end;
    const sorted = [...parsed.defs].sort(
      (a, b) => Number(map.get(a.label)) - Number(map.get(b.label))
    );
    const rebuilt = sorted
      .map((d) => `${d.indent}[^${map.get(d.label)}]:${d.body}`)
      .join("\n");
    if (rebuilt !== text.slice(start, end)) edits.push({ from: start, to: end, text: rebuilt });
    reorderedBlock = { start, end };
  } else {
    for (const d of numDefs) {
      const nw = map.get(d.label);
      if (nw && nw !== d.label) {
        const headStart = d.lineStart + d.indent.length;
        edits.push({
          from: headStart,
          to: headStart + `[^${d.label}]`.length,
          text: `[^${nw}]`,
        });
      }
    }
  }

  const newText = applyEdits(text, edits);
  return { changed: newText !== text, edits, newText, reorderedBlock, problems };
}
