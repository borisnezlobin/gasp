// Apply the renumber invariant to a live editor as one undoable transaction,
// preserving the caret. Shared by the Tidy command and the edit-watcher.

import { Editor } from "obsidian";
import { computeRenumber, mapOffset, RenumberResult } from "./renumber";
import { posInText } from "./text";

export interface ApplyOutcome {
  result: RenumberResult;
  applied: boolean;
}

export function applyRenumber(editor: Editor, guardCaretInBlock = false): ApplyOutcome {
  const result = computeRenumber(editor.getValue());
  if (!result.changed) return { result, applied: false };

  const caretOff = editor.posToOffset(editor.getCursor());
  if (guardCaretInBlock && result.reorderedBlock) {
    const { start, end } = result.reorderedBlock;
    // Don't rearrange the definition block out from under an in-progress edit.
    if (caretOff >= start && caretOff <= end) return { result, applied: false };
  }

  const changes = [...result.edits]
    .sort((a, b) => a.from - b.from)
    .map((e) => ({
      from: editor.offsetToPos(e.from),
      to: editor.offsetToPos(e.to),
      text: e.text,
    }));
  const caret = posInText(result.newText, mapOffset(result.edits, caretOff));
  editor.transaction({ changes, selection: { from: caret } });
  return { result, applied: true };
}
