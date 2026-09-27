//! Ported from the plugin's `test/core.test.ts`, same inputs and outputs.

use super::*;

/// `‸` marks the cursor. Also checks the edits reproduce the new text.
fn insert_at(with_caret: &str, jump: bool) -> PlannedInsert {
    let cursor = with_caret.find('‸').expect("caret marker");
    let text = with_caret.replacen('‸', "", 1);
    let planned = plan_new_footnote(&text, cursor, jump);
    assert_eq!(apply_edits(&text, &planned.edits), planned.new_text);
    planned
}

fn labels_of_refs(parsed: &ParsedFootnotes) -> Vec<&str> {
    parsed.refs.iter().map(|r| r.label.as_str()).collect()
}

#[test]
fn parse_finds_refs_and_defs() {
    let parsed = parse_footnotes("a[^1] b[^2]\n[^1]: one\n[^2]: two");
    assert_eq!(labels_of_refs(&parsed), ["1", "2"]);
    let defs: Vec<&str> = parsed.defs.iter().map(|d| d.label.as_str()).collect();
    assert_eq!(defs, ["1", "2"]);
}

#[test]
fn parse_ignores_refs_inside_fenced_code() {
    let parsed = parse_footnotes("text[^1]\n```\ncode [^9] here\n```\n[^1]: real");
    assert_eq!(labels_of_refs(&parsed), ["1"]);
}

#[test]
fn parse_detects_inline_typos_which_are_not_refs() {
    let parsed = parse_footnotes("detectors^[2] and one.^[2]\n[^2]: real");
    let typos: Vec<&str> = parsed
        .inline_typos
        .iter()
        .map(|t| t.label.as_str())
        .collect();
    assert_eq!(typos, ["2", "2"]);
    assert!(labels_of_refs(&parsed).is_empty());
}

#[test]
fn renumber_normalizes_out_of_order_body_and_defs() {
    let result = compute_renumber("X[^2] Y[^1]\n[^1]: one\n[^2]: two");
    assert_eq!(result.new_text, "X[^1] Y[^2]\n[^1]: two\n[^2]: one");
    assert!(result.changed);
    assert!(!compute_renumber(&result.new_text).changed);
}

#[test]
fn renumber_closes_gap_after_deletion() {
    let result = compute_renumber("A[^1] B[^3]\n[^1]: one\n[^3]: three");
    assert_eq!(result.new_text, "A[^1] B[^2]\n[^1]: one\n[^2]: three");
}

#[test]
fn renumber_keeps_named_footnotes() {
    let result = compute_renumber("a[^2] b[^note]\n[^note]: cite\n[^2]: two");
    assert!(result.new_text.contains("[^note]"));
}

#[test]
fn cursor_mapping_through_one_edit() {
    let edits = [FootnoteEdit::new(5..9, "[^10]")];
    assert_eq!(map_offset(&edits, 3), 3);
    assert_eq!(map_offset(&edits, 9), 10);
    assert_eq!(map_offset(&edits, 20), 21);
    assert_eq!(map_offset(&edits, 7), 10);
}

#[test]
fn apply_edits_matches_renumbered_text() {
    let source = "X[^2] Y[^1]\n[^1]: one\n[^2]: two";
    let result = compute_renumber(source);
    assert_eq!(apply_edits(source, &result.edits), result.new_text);
}

#[test]
fn insert_first_footnote_into_plain_text() {
    let planned = insert_at("Hello world‸", true);
    assert_eq!(planned.new_text, "Hello world[^1]\n\n[^1]: ");
    assert_eq!(&planned.new_text[planned.cursor..], "");
}

#[test]
fn insert_appends_after_in_order_footnotes() {
    let planned = insert_at("A[^1] B[^2]‸\n[^1]: one\n[^2]: two", true);
    assert_eq!(
        planned.new_text,
        "A[^1] B[^2][^3]\n[^1]: one\n[^2]: two\n[^3]: "
    );
}

#[test]
fn insert_in_the_middle_shifts_following_footnotes() {
    let planned = insert_at("A[^1] ‸B[^2]\n[^1]: one\n[^2]: two", true);
    assert_eq!(
        planned.new_text,
        "A[^1] [^2]B[^3]\n[^1]: one\n[^2]: \n[^3]: two"
    );
    assert!(planned.new_text[planned.cursor..].starts_with("\n[^3]: two"));
}

/// The plugin's optional "real note" checks, run on a synthetic note.
#[test]
fn note_invariants_hold_after_renumber() {
    let original = "Intro[^4] and more[^b].\n\n- item[^2]\n- again[^4]\n\n[^2]: second\n[^4]: first\n[^b]: named";
    let before = parse_footnotes(original);
    let result = compute_renumber(original);
    let after = parse_footnotes(&result.new_text);

    let mut seen: Vec<u64> = Vec::new();
    let ascending = labels_of_refs(&after)
        .into_iter()
        .filter(|label| is_numeric(label))
        .all(|label| {
            let number: u64 = label.parse().unwrap_or(0);
            if seen.contains(&number) {
                return true;
            }
            seen.push(number);
            number == seen.len() as u64
        });
    assert!(ascending);

    let bodies = |parsed: &ParsedFootnotes| {
        let mut bodies: Vec<String> = parsed
            .defs
            .iter()
            .map(|d| d.body.trim().to_string())
            .collect();
        bodies.sort();
        bodies
    };
    assert_eq!(bodies(&after), bodies(&before));
    assert!(!compute_renumber(&result.new_text).changed);
}
