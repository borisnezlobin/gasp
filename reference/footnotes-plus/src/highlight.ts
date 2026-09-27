// Live editor decoration that marks footnote problems (dangling references,
// orphaned definitions, inline-syntax typos, duplicate/empty definitions) so
// the user can see mismatches instead of the plugin silently rearranging text.

import { RangeSetBuilder } from "@codemirror/state";
import { Decoration, DecorationSet, EditorView, ViewPlugin, ViewUpdate } from "@codemirror/view";
import { FootnoteProblemKind, findProblems } from "./lint";

const CLASS: Record<FootnoteProblemKind, string> = {
  "dangling-ref": "footnote-problem footnote-problem-dangling",
  "orphan-def": "footnote-problem footnote-problem-orphan",
  "inline-typo": "footnote-problem footnote-problem-typo",
  "duplicate-def": "footnote-problem footnote-problem-duplicate",
  "empty-def": "footnote-problem footnote-problem-empty",
};

// When two problems land on the same span (e.g. a definition that is both
// orphaned and duplicated), show only the most important one.
const PRIORITY: Record<FootnoteProblemKind, number> = {
  "dangling-ref": 0,
  "duplicate-def": 1,
  "orphan-def": 2,
  "empty-def": 3,
  "inline-typo": 4,
};

function buildDecorations(view: EditorView): DecorationSet {
  const problems = findProblems(view.state.doc.toString())
    .filter((p) => p.to > p.from)
    .sort((a, b) => a.from - b.from || a.to - b.to || PRIORITY[a.kind] - PRIORITY[b.kind]);

  const builder = new RangeSetBuilder<Decoration>();
  let lastSpan = "";
  for (const p of problems) {
    const span = `${p.from}-${p.to}`;
    if (span === lastSpan) continue;
    lastSpan = span;
    builder.add(
      p.from,
      p.to,
      Decoration.mark({ class: CLASS[p.kind], attributes: { title: p.message } })
    );
  }
  return builder.finish();
}

export const footnoteHighlighter = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;

    constructor(view: EditorView) {
      this.decorations = buildDecorations(view);
    }

    update(update: ViewUpdate) {
      if (update.docChanged || update.viewportChanged) {
        this.decorations = buildDecorations(update.view);
      }
    }
  },
  { decorations: (plugin) => plugin.decorations }
);
