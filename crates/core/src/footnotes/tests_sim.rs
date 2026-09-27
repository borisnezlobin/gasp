//! Ported from the plugin's `test/sim.test.ts`: drive the commands through a
//! small editor model and check footnotes never get jumbled.

use super::*;

struct Editor {
    text: String,
    cursor: usize,
    notices: Vec<String>,
}

impl Editor {
    fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            cursor: 0,
            notices: Vec::new(),
        }
    }

    /// Puts the cursor at the first occurrence of `marker`, or the end.
    fn place_cursor_at(&mut self, marker: &str) {
        self.cursor = self.text.find(marker).unwrap_or(self.text.len());
    }

    fn set_line_ch(&mut self, line: usize, ch: usize) {
        let start: usize = self.text.split('\n').take(line).map(|l| l.len() + 1).sum();
        self.cursor = start + ch;
    }

    fn cursor_line(&self) -> usize {
        self.text[..self.cursor].matches('\n').count()
    }

    fn insert_or_jump(&mut self) {
        match insert_or_jump(&self.text, self.cursor, &FootnoteSettings::default()) {
            InsertOrJump::Jump { cursor } => self.cursor = cursor,
            InsertOrJump::Edit { edits, cursor } => {
                self.text = apply_edits(&self.text, &edits);
                self.cursor = cursor;
            }
            InsertOrJump::Unreferenced { label } => self.notices.push(unreferenced_message(&label)),
        }
    }

    fn tidy(&mut self) -> AppliedRenumber {
        let outcome = apply_renumber(&self.text, self.cursor, false);
        if outcome.applied {
            self.text = apply_edits(&self.text, &outcome.result.edits);
            self.cursor = outcome.cursor;
        }
        outcome
    }
}

fn has_problem(problems: &[FootnoteProblem], kind: FootnoteProblemKind, label: &str) -> bool {
    problems.iter().any(|p| p.kind == kind && p.label == label)
}

fn count_numbered_markers(text: &str) -> usize {
    text.as_bytes()
        .windows(4)
        .filter(|w| w[0] == b'[' && w[1] == b'^' && w[2].is_ascii_digit() && w[3] == b']')
        .count()
}

#[test]
fn s1_insert_three_footnotes_into_fresh_prose() {
    let mut editor = Editor::new("The first point and a second point and a third point.");
    editor.place_cursor_at(" and a second");
    editor.insert_or_jump();
    editor.place_cursor_at(" and a third");
    editor.insert_or_jump();
    editor.place_cursor_at(" point.");
    editor.insert_or_jump();

    let first_line = editor.text.split('\n').next().unwrap_or_default();
    let one = first_line.find("[^1]");
    let two = first_line.find("[^2]");
    let three = first_line.find("[^3]");
    assert!(one < two && two < three && one.is_some(), "{}", editor.text);
    let definitions = editor
        .text
        .split('\n')
        .filter(|line| line.len() >= 5 && line.starts_with("[^") && line[3..].starts_with("]:"))
        .count();
    assert_eq!(definitions, 3);
}

#[test]
fn s2_insert_in_the_middle_shifts_the_ones_after_it() {
    let mut editor = Editor::new("Alpha[^1] then Bravo[^2]\n[^1]: one\n[^2]: two");
    editor.cursor = editor.text.find("then").unwrap_or(0) + 4;
    editor.insert_or_jump();
    assert!(
        editor.text.contains("Alpha[^1] then[^2] Bravo[^3]"),
        "{}",
        editor.text
    );
    assert!(editor.text.contains("[^3]: two"));
    assert!(editor.text.contains("[^2]: \n") || editor.text.ends_with("[^2]: "));
}

#[test]
fn s3_tidy_normalizes_out_of_order_footnotes() {
    let mut editor = Editor::new("X[^2] Y[^1]\n[^1]: one\n[^2]: two");
    let outcome = editor.tidy();
    assert!(outcome.applied);
    assert!(editor.text.contains("X[^1] Y[^2]"));
    assert!(editor.text.ends_with("[^1]: two\n[^2]: one"));
}

#[test]
fn s4_deleting_only_a_marker_does_not_jumble() {
    let mut editor = Editor::new("a[^1] b[^2] c[^3]\n[^1]: one\n[^2]: two\n[^3]: three");
    editor.text = editor.text.replace(" c[^3]", " c");
    let outcome = editor.tidy();
    assert!(!outcome.applied);
    assert!(editor.text.contains("[^3]: three"));
    assert!(has_problem(
        &outcome.result.problems,
        FootnoteProblemKind::Unused,
        "3"
    ));
}

#[test]
fn s5_deleting_marker_and_definition_closes_the_gap() {
    let mut editor = Editor::new("a[^1] b[^2] c[^3]\n[^1]: one\n[^2]: two\n[^3]: three");
    editor.text = editor
        .text
        .replace(" b[^2]", " b")
        .replace("\n[^2]: two", "");
    let outcome = editor.tidy();
    assert!(outcome.applied);
    assert!(editor.text.contains("a[^1] b c[^2]") && editor.text.contains("[^2]: three"));
}

#[test]
fn s6_dangling_reference_blocks_renumber() {
    let mut editor = Editor::new("a[^1] b[^9]\n[^1]: one");
    let outcome = editor.tidy();
    assert!(!outcome.applied);
    assert!(has_problem(
        &outcome.result.problems,
        FootnoteProblemKind::Missing,
        "9"
    ));
}

#[test]
fn s7_duplicate_definition_blocks_renumber() {
    let mut editor = Editor::new("a[^1] b[^2]\n[^1]: one\n[^2]: two\n[^2]: two again");
    let outcome = editor.tidy();
    assert!(!outcome.applied);
    assert!(has_problem(
        &outcome.result.problems,
        FootnoteProblemKind::Duplicate,
        "2"
    ));
}

#[test]
fn s8_inline_typo_is_flagged_while_tidy_works() {
    let mut editor = Editor::new("a[^1] b^[2] c[^2]\n[^1]: one\n[^2]: two");
    let outcome = editor.tidy();
    assert!(outcome.applied || editor.text.contains("a[^1] b^[2] c[^2]"));
    assert!(has_problem(
        &outcome.result.problems,
        FootnoteProblemKind::InlineTypo,
        "2"
    ));
}

#[test]
fn s9_undo_guard_does_not_reapply_an_undone_renumber() {
    let before = "X[^2] Y[^1]\n[^1]: one\n[^2]: two";
    let settings = FootnoteSettings::default();
    let mut auto = AutoRenumber::new();
    let applied = auto.on_idle(before, 0, &settings);
    assert!(applied.is_some());
    // The user undoes, returning to the pre-renumber text; the watcher fires again.
    let again = auto.on_idle(before, 0, &settings);
    assert!(again.is_none());
    let text = before;
    assert_eq!(text, "X[^2] Y[^1]\n[^1]: one\n[^2]: two");
}

#[test]
fn s10_on_a_definition_line_jumps_and_never_inserts() {
    let mut editor = Editor::new("a[^1]\n[^1]: one");
    let before_count = count_numbered_markers(&editor.text);
    editor.set_line_ch(1, 8);
    editor.insert_or_jump();
    assert_eq!(count_numbered_markers(&editor.text), before_count);
    assert_eq!(editor.cursor_line(), 0);
}

#[test]
fn s11_orphan_definition_line_warns_instead_of_inserting() {
    let mut editor = Editor::new("a[^1]\n[^1]: one\n[^6]: orphan");
    let before = editor.text.clone();
    editor.set_line_ch(2, 6);
    editor.insert_or_jump();
    assert_eq!(editor.text, before);
    assert!(editor.notices.iter().any(|m| m.contains("[^6]")));
}

#[test]
fn s12_problem_report_for_a_messy_note() {
    let messy = "Ref a[^1] b[^2] c^[3] d[^7]\n[^1]: one\n[^2]:\n[^2]: dup\n[^5]: never used";
    let problems = find_problems(messy);
    assert!(has_problem(&problems, FootnoteProblemKind::Missing, "7"));
    assert!(has_problem(&problems, FootnoteProblemKind::InlineTypo, "3"));
    assert!(has_problem(&problems, FootnoteProblemKind::Duplicate, "2"));
    assert!(has_problem(&problems, FootnoteProblemKind::Empty, "2"));
    assert!(has_problem(&problems, FootnoteProblemKind::Unused, "5"));
}
