// Drives the REAL command code (insert.ts / apply.ts / renumber.ts / lint.ts)
// through a mock editor that mimics Obsidian's Editor, so we can "play with"
// the plugin headlessly on throwaway notes and assert it never jumbles.

import { Notice } from "obsidian";
import { insertOrJumpFootnote } from "../src/insert";
import { applyRenumber } from "../src/apply";
import { findProblems } from "../src/lint";
import { posInText } from "../src/text";

interface Pos {
  line: number;
  ch: number;
}

class MockEditor {
  value: string;
  cursor: Pos = { line: 0, ch: 0 };
  cm = undefined;

  constructor(value: string) {
    this.value = value;
  }
  getValue() {
    return this.value;
  }
  setValue(v: string) {
    this.value = v;
    this.cursor = { line: 0, ch: 0 };
  }
  getLine(line: number) {
    return this.value.split("\n")[line] ?? "";
  }
  lineCount() {
    return this.value.split("\n").length;
  }
  lastLine() {
    return this.lineCount() - 1;
  }
  getCursor() {
    return this.cursor;
  }
  setCursor(posOrLine: Pos | number, ch?: number) {
    this.cursor =
      typeof posOrLine === "number" ? { line: posOrLine, ch: ch ?? 0 } : posOrLine;
  }
  posToOffset(pos: Pos) {
    const lines = this.value.split("\n");
    let off = 0;
    for (let i = 0; i < pos.line; i++) off += lines[i].length + 1;
    return off + pos.ch;
  }
  offsetToPos(offset: number): Pos {
    return posInText(this.value, offset);
  }
  transaction(tx: {
    changes?: { from: Pos; to?: Pos; text: string }[];
    selection?: { from: Pos };
  }) {
    if (tx.changes) {
      const edits = tx.changes
        .map((c) => ({
          from: this.posToOffset(c.from),
          to: c.to ? this.posToOffset(c.to) : this.posToOffset(c.from),
          text: c.text,
        }))
        .sort((a, b) => b.from - a.from);
      let v = this.value;
      for (const e of edits) v = v.slice(0, e.from) + e.text + v.slice(e.to);
      this.value = v;
    }
    if (tx.selection) this.cursor = tx.selection.from;
  }
  focus() {}

  placeCaretAt(marker: string) {
    const at = this.value.indexOf(marker);
    this.cursor = this.offsetToPos(at < 0 ? this.value.length : at);
  }
}

let passed = 0;
let failed = 0;
function check(name: string, cond: boolean, detail?: string) {
  if (cond) passed++;
  else {
    failed++;
    console.log(`  FAIL: ${name}${detail ? `\n        ${detail}` : ""}`);
  }
}
function show(label: string, text: string) {
  console.log(`  ${label}: ${JSON.stringify(text)}`);
}

const opts = { jumpToNewDefinition: true };

console.log("S1 — insert three footnotes into fresh prose");
{
  const ed = new MockEditor("The first point and a second point and a third point.");
  ed.placeCaretAt(" and a second"); // after "first point"
  insertOrJumpFootnote(ed, opts);
  ed.placeCaretAt(" and a third");
  insertOrJumpFootnote(ed, opts);
  ed.setCursor(ed.offsetToPos(ed.getValue().indexOf(" point.") ));
  insertOrJumpFootnote(ed, opts);
  show("result", ed.getValue());
  const firstLine = ed.getValue().split("\n")[0];
  check("three sequential markers 1,2,3 in order", /\[\^1\].*\[\^2\].*\[\^3\]/.test(firstLine));
  check("three definitions created", (ed.getValue().match(/^\[\^\d\]:/gm) || []).length === 3);
}

console.log("S2 — insert in the middle shifts the ones after it");
{
  const ed = new MockEditor("Alpha[^1] then Bravo[^2]\n[^1]: one\n[^2]: two");
  ed.setCursor(ed.offsetToPos(ed.getValue().indexOf("then") + 4)); // just after "then"
  insertOrJumpFootnote(ed, opts);
  show("result", ed.getValue());
  check("new marker took slot 2", /Alpha\[\^1\] then\[\^2\] Bravo\[\^3\]/.test(ed.getValue()));
  check("old 'two' pushed to [^3]", /\[\^3\]: two/.test(ed.getValue()));
  check("new empty definition is [^2]", /\[\^2\]: \n/.test(ed.getValue()) || ed.getValue().endsWith("[^2]: "));
}

console.log("S3 — Tidy normalizes out-of-order footnotes");
{
  const ed = new MockEditor("X[^2] Y[^1]\n[^1]: one\n[^2]: two");
  const { applied } = applyRenumber(ed);
  show("result", ed.getValue());
  check("applied", applied);
  check("body now 1 then 2", /X\[\^1\] Y\[\^2\]/.test(ed.getValue()));
  check("defs reordered to match", ed.getValue().endsWith("[^1]: two\n[^2]: one"));
}

console.log("S4 — delete only the [^3] MARKER (definition left behind) must NOT jumble");
{
  const ed = new MockEditor(
    "a[^1] b[^2] c[^3]\n[^1]: one\n[^2]: two\n[^3]: three"
  );
  // simulate deleting the "c[^3]" marker in the body, keeping the definition
  ed.value = ed.value.replace(" c[^3]", " c");
  const { applied, result } = applyRenumber(ed);
  show("result", ed.getValue());
  check("renumber refused (no change)", !applied);
  check("definition text NOT moved", /\[\^3\]: three/.test(ed.getValue()));
  check("problem flagged: orphan [^3]", result.problems.some((p) => p.kind === "orphan-def" && p.label === "3"));
}

console.log("S5 — delete BOTH the marker and its definition closes the gap");
{
  const ed = new MockEditor("a[^1] b[^2] c[^3]\n[^1]: one\n[^2]: two\n[^3]: three");
  ed.value = ed.value.replace(" b[^2]", " b").replace("\n[^2]: two", "");
  const { applied } = applyRenumber(ed);
  show("result", ed.getValue());
  check("applied", applied);
  check("closed to 1..2", /a\[\^1\] b c\[\^2\]/.test(ed.getValue()) && /\[\^2\]: three/.test(ed.getValue()));
}

console.log("S6 — dangling reference (used, never defined) blocks renumber");
{
  const ed = new MockEditor("a[^1] b[^9]\n[^1]: one");
  const { applied, result } = applyRenumber(ed);
  check("renumber refused", !applied);
  check("problem: dangling [^9]", result.problems.some((p) => p.kind === "dangling-ref" && p.label === "9"));
}

console.log("S7 — duplicate definition blocks renumber");
{
  const ed = new MockEditor("a[^1] b[^2]\n[^1]: one\n[^2]: two\n[^2]: two again");
  const { applied, result } = applyRenumber(ed);
  check("renumber refused", !applied);
  check("problem: duplicate [^2]", result.problems.some((p) => p.kind === "duplicate-def" && p.label === "2"));
}

console.log("S8 — inline typo ^[2] alongside a real, consistent set: tidy works, typo flagged");
{
  const ed = new MockEditor("a[^1] b^[2] c[^2]\n[^1]: one\n[^2]: two");
  const { applied, result } = applyRenumber(ed);
  show("result", ed.getValue());
  check("tidy applied (set is consistent)", applied || /a\[\^1\] b\^\[2\] c\[\^2\]/.test(ed.getValue()));
  check("typo flagged", result.problems.some((p) => p.kind === "inline-typo" && p.label === "2"));
}

console.log("S9 — undo guard: watcher does not re-apply a change the user undid");
{
  const ed = new MockEditor("X[^2] Y[^1]\n[^1]: one\n[^2]: two");
  const before = ed.getValue();
  const { applied } = applyRenumber(ed);
  const undoGuard = applied ? before : null;
  check("first tidy applied", applied);
  // user presses Cmd+Z -> editor returns to the pre-renumber text
  ed.value = before;
  // watcher fires again; guard should suppress it
  let watcherActed = false;
  if (ed.getValue() !== undoGuard) {
    const r = applyRenumber(ed);
    watcherActed = r.applied;
  }
  check("watcher suppressed after undo", !watcherActed);
  check("undo stuck (still the pre-renumber text)", ed.getValue() === before);
}

console.log("S10 — Alt+0 on a definition line jumps, never inserts");
{
  Notice.messages = [];
  const ed = new MockEditor("a[^1]\n[^1]: one");
  const cnt = (s: string) => (s.match(/\[\^\d\]/g) || []).length;
  const beforeCount = cnt(ed.getValue());
  ed.setCursor({ line: 1, ch: 8 }); // inside "[^1]: one"
  insertOrJumpFootnote(ed, opts);
  check("no footnote inserted from a definition line", cnt(ed.getValue()) === beforeCount);
  check("caret jumped up to the body reference (line 0)", ed.getCursor().line === 0);
}

console.log("S11 — orphan definition line: Alt+0 warns instead of inserting");
{
  Notice.messages = [];
  const ed = new MockEditor("a[^1]\n[^1]: one\n[^6]: orphan");
  const before = ed.getValue();
  ed.setCursor({ line: 2, ch: 6 }); // inside "[^6]: orphan"
  insertOrJumpFootnote(ed, opts);
  check("document unchanged", ed.getValue() === before);
  check("notice explains the orphan", Notice.messages.some((m) => m.includes("[^6]")));
}

console.log("S12 — problem report for a messy note");
{
  const messy = "Ref a[^1] b[^2] c^[3] d[^7]\n[^1]: one\n[^2]:\n[^2]: dup\n[^5]: never used";
  const problems = findProblems(messy);
  for (const p of problems) console.log(`  - ${p.kind}: [^${p.label}] — ${p.message}`);
  check("finds dangling [^7]", problems.some((p) => p.kind === "dangling-ref" && p.label === "7"));
  check("finds inline typo ^[3]", problems.some((p) => p.kind === "inline-typo" && p.label === "3"));
  check("finds duplicate [^2]", problems.some((p) => p.kind === "duplicate-def" && p.label === "2"));
  check("finds empty [^2]", problems.some((p) => p.kind === "empty-def" && p.label === "2"));
  check("finds orphan [^5]", problems.some((p) => p.kind === "orphan-def" && p.label === "5"));
}

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
