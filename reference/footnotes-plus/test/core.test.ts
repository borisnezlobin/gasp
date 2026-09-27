import { readFileSync } from "fs";
import { parseFootnotes } from "../src/footnotes";
import { applyEdits, computeRenumber, mapOffset } from "../src/renumber";
import { planNewFootnote } from "../src/plan-insert";

let passed = 0;
let failed = 0;

function check(name: string, cond: boolean, detail?: string) {
  if (cond) {
    passed++;
  } else {
    failed++;
    console.log(`  FAIL: ${name}${detail ? `\n        ${detail}` : ""}`);
  }
}

function eq(name: string, actual: unknown, expected: unknown) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  check(name, a === e, `expected ${e}\n        actual   ${a}`);
}

// Insert helper: "‸" marks the caret. Returns the resulting text (caret removed)
// and where the caret lands.
function insertAt(withCaret: string, jump: boolean) {
  const cursorOff = withCaret.indexOf("‸");
  const text = withCaret.replace("‸", "");
  return planNewFootnote(text, cursorOff, jump);
}

console.log("parse + masking");
{
  const p = parseFootnotes("a[^1] b[^2]\n[^1]: one\n[^2]: two");
  eq("finds 2 refs", p.refs.map((r) => r.label), ["1", "2"]);
  eq("finds 2 defs", p.defs.map((d) => d.label), ["1", "2"]);

  const masked = parseFootnotes("text[^1]\n```\ncode [^9] here\n```\n[^1]: real");
  eq("ignores refs inside fenced code", masked.refs.map((r) => r.label), ["1"]);

  const inline = parseFootnotes("detectors^[2] and one.^[2]\n[^2]: real");
  eq("detects inline typos", inline.inlineTypos.map((t) => t.label), ["2", "2"]);
  eq("inline typo is not a ref", inline.refs.map((r) => r.label), []);
}

console.log("renumber");
{
  // out of order body -> renumber + reorder definitions to match
  const r = computeRenumber("X[^2] Y[^1]\n[^1]: one\n[^2]: two");
  eq(
    "out-of-order body/defs normalized",
    r.newText,
    "X[^1] Y[^2]\n[^1]: two\n[^2]: one"
  );
  check("reports changed", r.changed);

  // idempotent
  const r2 = computeRenumber(r.newText);
  check("already-ordered is a no-op", !r2.changed);

  // gap after deletion closes up (delete [^2], leaving [^1],[^3])
  const del = computeRenumber("A[^1] B[^3]\n[^1]: one\n[^3]: three");
  eq("gap closes to 1..N", del.newText, "A[^1] B[^2]\n[^1]: one\n[^2]: three");

  // named footnotes are left untouched (mixed block -> relabel in place, no reorder)
  const named = computeRenumber("a[^2] b[^note]\n[^note]: cite\n[^2]: two");
  check("named footnote label preserved", named.newText.includes("[^note]"));
}

console.log("cursor mapping");
{
  // one edit replacing 4 chars ("[^9]") with 5 chars ("[^10]") at offset 5..9
  const edits = [{ from: 5, to: 9, text: "[^10]" }];
  eq("offset before edit unchanged", mapOffset(edits, 3), 3);
  eq("offset after edit shifts by +1", mapOffset(edits, 9), 10);
  eq("offset well after edit shifts by +1", mapOffset(edits, 20), 21);
  eq("offset inside edit clamps to end of replacement", mapOffset(edits, 7), 10);

  // applyEdits round-trips with computeRenumber's newText
  const src = "X[^2] Y[^1]\n[^1]: one\n[^2]: two";
  const r = computeRenumber(src);
  eq("applyEdits matches newText", applyEdits(src, r.edits), r.newText);
}

console.log("insert");
{
  // into plain text with no footnotes yet
  const a = insertAt("Hello world‸", true);
  eq("first footnote is [^1]", a.newText, "Hello world[^1]\n\n[^1]: ");
  check("caret lands after the new definition", a.newText.slice(a.caretOffset) === "");

  // append after existing in-order footnotes
  const b = insertAt("A[^1] B[^2]‸\n[^1]: one\n[^2]: two", true);
  eq(
    "append becomes [^3]",
    b.newText,
    "A[^1] B[^2][^3]\n[^1]: one\n[^2]: two\n[^3]: "
  );

  // insert in the MIDDLE renumbers the following ones
  const c = insertAt("A[^1] ‸B[^2]\n[^1]: one\n[^2]: two", true);
  eq(
    "middle insert shifts following",
    c.newText,
    "A[^1] [^2]B[^3]\n[^1]: one\n[^2]: \n[^3]: two"
  );
  check(
    "caret lands in the newly created (empty) definition [^2]",
    c.newText.slice(c.caretOffset).startsWith("\n[^3]: two")
  );
}

// Optional: run against a real note passed as argv[2].
const realPath = process.argv[2];
if (realPath) {
  console.log(`\nreal note: ${realPath}`);
  const original = readFileSync(realPath, "utf8");
  const before = parseFootnotes(original);
  const r = computeRenumber(original);
  const after = parseFootnotes(r.newText);

  const beforeBodyLabels = before.refs.map((x) => x.label);
  const afterBodyLabels = after.refs.map((x) => x.label);
  console.log(`  body refs before: ${beforeBodyLabels.join(", ")}`);
  console.log(`  body refs after:  ${afterBodyLabels.join(", ")}`);
  console.log(`  def labels after: ${after.defs.map((x) => x.label).join(", ")}`);

  const numericAfter = afterBodyLabels.filter((l) => /^\d+$/.test(l)).map(Number);
  const seen = new Set<number>();
  const inAscendingFirstAppearance = numericAfter.every((n) => {
    if (seen.has(n)) return true;
    const expected = seen.size + 1;
    seen.add(n);
    return n === expected;
  });
  check("real note: body numbered 1..N by appearance", inAscendingFirstAppearance);

  // no definition text lost: every original def body still present after renumber
  const afterBodies = after.defs.map((d) => d.body.trim()).sort();
  const beforeBodies = before.defs.map((d) => d.body.trim()).sort();
  eq("real note: no definition content lost", afterBodies, beforeBodies);

  check("real note: renumber is idempotent", !computeRenumber(r.newText).changed);
}

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
