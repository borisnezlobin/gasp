// The Alt+0 action. One key, context-aware:
//   - caret on a definition line   -> jump up to the first reference (never insert here)
//   - caret on a reference marker  -> jump down to its definition
//   - otherwise                    -> insert a new footnote here
//
// Text planning lives in ./plan-insert (pure, unit-tested); this file wires the
// plan to the live editor using a transaction so the insert stays undoable.

import { Editor, Notice } from "obsidian";
import { parseFootnotes } from "./footnotes";
import { jumpToDefinition, jumpToReference, moveCaretTo } from "./jump";
import { planCreateMissingDefinition, planNewFootnote } from "./plan-insert";
import { posInText } from "./text";

const DEF_LINE_RE = /^\s*\[\^([^\[\]\n]+)\]:/;

export interface InsertOptions {
  jumpToNewDefinition: boolean;
}

export function insertOrJumpFootnote(editor: Editor, opts: InsertOptions): void {
  const text = editor.getValue();
  const cursor = editor.getCursor();
  const cursorOff = editor.posToOffset(cursor);
  const parsed = parseFootnotes(text);

  // On a definition line we always navigate, never insert — inserting here is
  // what produced footnotes-inside-footnotes.
  const defMatch = editor.getLine(cursor.line).match(DEF_LINE_RE);
  if (defMatch) {
    if (!jumpToReference(editor, parsed.refs, defMatch[1])) {
      new Notice(`Footnote [^${defMatch[1]}] isn't referenced in the text.`);
    }
    return;
  }

  const onRef = parsed.refs.find((r) => cursorOff >= r.start && cursorOff <= r.end);
  if (onRef) {
    if (jumpToDefinition(editor, parsed.defs, onRef.label)) return;
    const created = planCreateMissingDefinition(text, parsed.defs, onRef.label);
    replaceDoc(editor, created);
    jumpToDefinition(editor, parseFootnotes(created).defs, onRef.label);
    return;
  }

  const planned = planNewFootnote(text, cursorOff, opts.jumpToNewDefinition);
  replaceDoc(editor, planned.newText);
  moveCaretTo(editor, planned.caretOffset);
}

// Replace the whole document in a single transaction so it remains one undo
// step (unlike editor.setValue, which discards undo history).
function replaceDoc(editor: Editor, newText: string): void {
  const oldText = editor.getValue();
  editor.transaction({
    changes: [{ from: { line: 0, ch: 0 }, to: posInText(oldText, oldText.length), text: newText }],
  });
}
