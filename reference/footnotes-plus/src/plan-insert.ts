// Pure text-planning for insertion, with no dependency on the Obsidian editor
// so it can be unit-tested directly. The strategy: drop a uniquely-numbered
// marker at the caret plus an empty definition, run the renumber engine, then
// locate where the new footnote ended up.

import { FootnoteDef, isNumeric, maxNumericLabel, parseFootnotes } from "./footnotes";
import { computeRenumber } from "./renumber";

export interface PlannedInsert {
  newText: string;
  caretOffset: number;
}

export function planNewFootnote(
  text: string,
  cursorOff: number,
  jumpToNewDefinition: boolean
): PlannedInsert {
  const parsed = parseFootnotes(text);
  const temp = String(maxNumericLabel(parsed) + 1);

  // The new footnote's number equals its rank by first appearance in the body.
  const firstAppearance = new Map<string, number>();
  for (const r of parsed.refs) {
    if (isNumeric(r.label) && !firstAppearance.has(r.label)) {
      firstAppearance.set(r.label, r.start);
    }
  }
  let distinctBefore = 0;
  for (const start of firstAppearance.values()) if (start < cursorOff) distinctBefore++;
  const targetRank = distinctBefore + 1;

  const hasDefs = parsed.defs.length > 0;
  const defPos = hasDefs ? parsed.defs[parsed.defs.length - 1].end : text.length;
  const markerText = `[^${temp}]`;
  const defText = hasDefs ? `\n[^${temp}]: ` : `\n\n[^${temp}]: `;

  // Apply right-to-left so earlier offsets stay valid. When the marker and the
  // definition want the same offset (first footnote, caret at end of doc), the
  // definition must be inserted first so the marker ends up before it.
  const inserts = [
    { pos: cursorOff, s: markerText, order: 0 },
    { pos: defPos, s: defText, order: 1 },
  ].sort((a, b) => b.pos - a.pos || b.order - a.order);
  let interim = text;
  for (const ins of inserts) interim = interim.slice(0, ins.pos) + ins.s + interim.slice(ins.pos);

  const result = computeRenumber(interim);
  const newText = result.newText;
  // If the document was inconsistent, renumber bails and the marker keeps its
  // temporary (max+1) label; otherwise it becomes its position rank.
  const finalLabel = result.changed ? String(targetRank) : temp;

  let caretOffset: number;
  if (jumpToNewDefinition) {
    const m = new RegExp(`^\\[\\^${finalLabel}\\]: ?`, "m").exec(newText);
    caretOffset = m ? m.index + m[0].length : cursorOff + markerText.length;
  } else {
    const m = new RegExp(`\\[\\^${finalLabel}\\](?!:)`).exec(newText);
    caretOffset = m ? m.index + m[0].length : cursorOff + markerText.length;
  }
  return { newText, caretOffset };
}

export function planCreateMissingDefinition(
  text: string,
  defs: FootnoteDef[],
  label: string
): string {
  const hasDefs = defs.length > 0;
  const pos = hasDefs ? defs[defs.length - 1].end : text.length;
  const insert = hasDefs ? `\n[^${label}]: ` : `\n\n[^${label}]: `;
  return text.slice(0, pos) + insert + text.slice(pos);
}
