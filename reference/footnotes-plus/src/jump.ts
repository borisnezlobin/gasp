// Editor-side navigation between a footnote reference and its definition.
// Reading view / exported PDF already get Obsidian's native "↩︎" backrefs; this
// is what makes the same round-trip work while editing.

import { Editor } from "obsidian";
import { EditorView } from "@codemirror/view";
import { FootnoteDef, FootnoteRef, findDef, firstRef } from "./footnotes";

export function moveCaretTo(editor: Editor, offset: number): void {
  const pos = editor.offsetToPos(offset);
  editor.setCursor(pos);
  const cm = (editor as unknown as { cm?: EditorView }).cm;
  if (cm) {
    try {
      cm.dispatch({ effects: EditorView.scrollIntoView(offset, { y: "center" }) });
    } catch {
      // scrollIntoView can throw if the offset is momentarily out of range;
      // the setCursor above already lands the caret.
    }
  }
  editor.focus();
}

export function jumpToDefinition(editor: Editor, defs: FootnoteDef[], label: string): boolean {
  const def = findDef(defs, label);
  if (!def) return false;
  const headLen = def.indent.length + `[^${label}]:`.length;
  const leadingSpace = def.body.startsWith(" ") ? 1 : 0;
  moveCaretTo(editor, def.lineStart + headLen + leadingSpace);
  return true;
}

export function jumpToReference(editor: Editor, refs: FootnoteRef[], label: string): boolean {
  const ref = firstRef(refs, label);
  if (!ref) return false;
  moveCaretTo(editor, ref.end);
  return true;
}
