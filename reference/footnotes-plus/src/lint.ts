// Footnote problem detection, shared by the editor highlighter and the
// renumber engine. "Structural" problems (a reference and definition that
// don't match up, or a doubly-defined footnote) block renumbering so we never
// rearrange an inconsistent document; the rest are advisory.

import { ParsedFootnotes, parseFootnotes } from "./footnotes";

export type FootnoteProblemKind =
  | "dangling-ref"
  | "orphan-def"
  | "inline-typo"
  | "duplicate-def"
  | "empty-def";

export interface FootnoteProblem {
  kind: FootnoteProblemKind;
  from: number;
  to: number;
  label: string;
  message: string;
}

const BLOCKING: ReadonlySet<FootnoteProblemKind> = new Set([
  "dangling-ref",
  "orphan-def",
  "duplicate-def",
]);

export function classifyProblems(parsed: ParsedFootnotes): FootnoteProblem[] {
  const { refs, defs, inlineTypos } = parsed;
  const refLabels = new Set(refs.map((r) => r.label));
  const defCounts = new Map<string, number>();
  for (const d of defs) defCounts.set(d.label, (defCounts.get(d.label) ?? 0) + 1);

  const problems: FootnoteProblem[] = [];

  for (const r of refs) {
    if (!defCounts.has(r.label)) {
      problems.push({
        kind: "dangling-ref",
        from: r.start,
        to: r.end,
        label: r.label,
        message: `Footnote [^${r.label}] has no definition.`,
      });
    }
  }

  for (const d of defs) {
    const from = d.lineStart + d.indent.length;
    const to = from + `[^${d.label}]:`.length;
    if (!refLabels.has(d.label)) {
      problems.push({
        kind: "orphan-def",
        from,
        to,
        label: d.label,
        message: `Footnote [^${d.label}] is defined but never used.`,
      });
    }
    if ((defCounts.get(d.label) ?? 0) > 1) {
      problems.push({
        kind: "duplicate-def",
        from,
        to,
        label: d.label,
        message: `Footnote [^${d.label}] is defined more than once.`,
      });
    }
    if (d.body.trim() === "") {
      problems.push({
        kind: "empty-def",
        from,
        to,
        label: d.label,
        message: `Footnote [^${d.label}] has no text yet.`,
      });
    }
  }

  for (const t of inlineTypos) {
    problems.push({
      kind: "inline-typo",
      from: t.start,
      to: t.end,
      label: t.label,
      message: `"^[${t.label}]" is an inline footnote containing "${t.label}". Did you mean [^${t.label}]?`,
    });
  }

  return problems;
}

export function findProblems(text: string): FootnoteProblem[] {
  return classifyProblems(parseFootnotes(text));
}

export function hasBlockingProblems(problems: FootnoteProblem[]): boolean {
  return problems.some((p) => BLOCKING.has(p.kind));
}

export function blockingLabels(problems: FootnoteProblem[]): string[] {
  const labels = new Set<string>();
  for (const p of problems) if (BLOCKING.has(p.kind)) labels.add(`[^${p.label}]`);
  return [...labels];
}
