//! Edge cases beyond the plugin's own tests.

use super::*;

fn edit_result(text: &str, cursor: usize, settings: &FootnoteSettings) -> (String, usize) {
    match insert_or_jump(text, cursor, settings) {
        InsertOrJump::Edit { edits, cursor } => (apply_edits(text, &edits), cursor),
        other => panic!("expected an edit, got {other:?}"),
    }
}

fn jump_target(text: &str, cursor: usize) -> usize {
    match insert_or_jump(text, cursor, &FootnoteSettings::default()) {
        InsertOrJump::Jump { cursor } => cursor,
        other => panic!("expected a jump, got {other:?}"),
    }
}

#[test]
fn settings_default_to_the_plugin_defaults() {
    let settings = FootnoteSettings::default();
    assert!(settings.auto_renumber_on_edit);
    assert!(settings.jump_to_new_definition);
}

#[test]
fn crlf_insert_uses_crlf_and_keeps_definitions_clean() {
    let text = "A[^1] B\r\n\r\n[^1]: one\r\n";
    let cursor = text.find(" B").unwrap_or(0) + 2;
    let (new_text, cursor) = edit_result(text, cursor, &FootnoteSettings::default());
    assert_eq!(new_text, "A[^1] B[^2]\r\n\r\n[^1]: one\r\n[^2]: \r\n");
    assert_eq!(&new_text[cursor..], "\r\n");
}

#[test]
fn crlf_renumber_reorders_with_crlf() {
    let result = compute_renumber("X[^2] Y[^1]\r\n[^1]: one\r\n[^2]: two\r\n");
    assert_eq!(result.new_text, "X[^1] Y[^2]\r\n[^1]: two\r\n[^2]: one\r\n");
    let defs = parse_footnotes(&result.new_text).defs;
    assert_eq!(defs[0].body, " two");
    assert!(find_problems(&result.new_text).is_empty());
}

#[test]
fn crlf_first_footnote_at_end_of_text() {
    let (new_text, _) = edit_result("line one\r\nline two", 18, &FootnoteSettings::default());
    assert_eq!(new_text, "line one\r\nline two[^1]\r\n\r\n[^1]: ");
}

#[test]
fn footnotes_inside_callouts() {
    let text = "> [!note]\n> Callout text[^2] and more[^1].\n>\n> [^1]: first\n> [^2]: second";
    let parsed = parse_footnotes(text);
    assert_eq!(parsed.defs.len(), 2);
    assert_eq!(parsed.defs[0].indent, "> ");
    assert_eq!(parsed.defs[0].body, " first");
    let result = compute_renumber(text);
    assert_eq!(
        result.new_text,
        "> [!note]\n> Callout text[^1] and more[^2].\n>\n> [^1]: second\n> [^2]: first"
    );
}

#[test]
fn insert_after_callout_definitions_keeps_the_prefix() {
    let text = "> Text[^1] here\n>\n> [^1]: first";
    let cursor = text.find(" here").unwrap_or(0) + 5;
    let (new_text, _) = edit_result(text, cursor, &FootnoteSettings::default());
    assert_eq!(new_text, "> Text[^1] here[^2]\n>\n> [^1]: first\n> [^2]: ");
}

#[test]
fn footnotes_inside_lists() {
    let text = "- one[^2]\n- two[^1]\n  - nested[^2]\n\n[^1]: a\n[^2]: b";
    let result = compute_renumber(text);
    assert_eq!(
        result.new_text,
        "- one[^1]\n- two[^2]\n  - nested[^1]\n\n[^1]: b\n[^2]: a"
    );
}

#[test]
fn multi_paragraph_definition_moves_as_one_block() {
    let text = "B[^2] A[^1]\n\n[^1]: first para\n\n    second para of one\n[^2]: two";
    let parsed = parse_footnotes(text);
    assert_eq!(parsed.defs.len(), 2);
    assert_eq!(parsed.defs[0].body, " first para\n\n    second para of one");
    let result = compute_renumber(text);
    assert_eq!(
        result.new_text,
        "B[^1] A[^2]\n\n[^1]: two\n[^2]: first para\n\n    second para of one"
    );
}

#[test]
fn unindented_paragraph_after_definition_is_not_part_of_it() {
    let parsed = parse_footnotes("a[^1]\n\n[^1]: def\n\nNext paragraph.");
    assert_eq!(parsed.defs[0].body, " def");
}

#[test]
fn unicode_text_before_the_cursor() {
    let text = "Ünïcødé 日本語 ✓";
    let cursor = text.len();
    let (new_text, cursor) = edit_result(text, cursor, &FootnoteSettings::default());
    assert_eq!(new_text, "Ünïcødé 日本語 ✓[^1]\n\n[^1]: ");
    assert_eq!(cursor, new_text.len());

    let middle = "日本[^1] 語 ✓\n\n[^1]: 一";
    let cursor = middle.find(" 語").unwrap_or(0) + " 語".len();
    let settings = FootnoteSettings {
        jump_to_new_definition: false,
        ..FootnoteSettings::default()
    };
    let (new_text, cursor) = edit_result(middle, cursor, &settings);
    assert_eq!(new_text, "日本[^1] 語[^2] ✓\n\n[^1]: 一\n[^2]: ");
    assert_eq!(&new_text[..cursor], "日本[^1] 語[^2]");
}

#[test]
fn ignores_code_spans_fences_and_math() {
    let text = "`[^9]` $x^[2]$ real[^1]\n\n$$\n[^8]\n$$\n```\n[^7]: no\n```\n[^1]: yes";
    let parsed = parse_footnotes(text);
    let refs: Vec<&str> = parsed.refs.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(refs, ["1"]);
    assert_eq!(parsed.defs.len(), 1);
    assert!(parsed.inline_typos.is_empty());
    assert!(find_problems(text).is_empty());
}

#[test]
fn jump_between_reference_and_definition() {
    let text = "See[^a] this.\n\n[^a]: named note";
    let on_ref = text.find("[^a]").unwrap_or(0) + 1;
    let at_def = jump_target(text, on_ref);
    assert_eq!(&text[at_def..], "named note");
    let back = jump_target(text, at_def + 3);
    assert_eq!(&text[..back], "See[^a]");
}

#[test]
fn jump_to_missing_definition_creates_it() {
    let text = "a[^1] b[^x]\n\n[^1]: one";
    let cursor = text.find("[^x]").unwrap_or(0) + 4;
    let (new_text, cursor) = edit_result(text, cursor, &FootnoteSettings::default());
    assert_eq!(new_text, "a[^1] b[^x]\n\n[^1]: one\n[^x]: ");
    assert_eq!(cursor, new_text.len());
}

#[test]
fn insert_without_jump_leaves_cursor_after_marker() {
    let settings = FootnoteSettings {
        jump_to_new_definition: false,
        ..FootnoteSettings::default()
    };
    let (new_text, cursor) = edit_result("Hello world", 5, &settings);
    assert_eq!(new_text, "Hello[^1] world\n\n[^1]: ");
    assert_eq!(&new_text[..cursor], "Hello[^1]");
}

#[test]
fn insert_into_empty_document_and_after_trailing_newline() {
    let (new_text, _) = edit_result("", 0, &FootnoteSettings::default());
    assert_eq!(new_text, "[^1]\n\n[^1]: ");
    let (new_text, _) = edit_result("Para one.\n\nPara two.\n", 9, &FootnoteSettings::default());
    assert_eq!(new_text, "Para one.[^1]\n\nPara two.\n\n[^1]: ");
}

#[test]
fn insert_into_inconsistent_document_keeps_temporary_label() {
    let text = "a[^1] b[^5] c";
    let (new_text, cursor) = edit_result(text, text.len(), &FootnoteSettings::default());
    assert_eq!(new_text, "a[^1] b[^5] c[^6]\n\n[^6]: ");
    assert_eq!(cursor, new_text.len());
}

#[test]
fn insert_edits_are_small() {
    let text = "Long intro. A[^1] B[^2]\n\n[^1]: one\n[^2]: two";
    let cursor = text.find(" B").unwrap_or(0) + 1;
    let InsertOrJump::Edit { edits, .. } =
        insert_or_jump(text, cursor, &FootnoteSettings::default())
    else {
        panic!("expected an edit");
    };
    assert!(
        edits
            .iter()
            .all(|e| e.range.start >= "Long intro. A[^1]".len())
    );
}

#[test]
fn mixed_named_and_numbered_definitions_are_relabelled_in_place() {
    let result = compute_renumber("a[^2] b[^note]\n[^note]: cite\n[^2]: two");
    assert_eq!(result.new_text, "a[^1] b[^note]\n[^note]: cite\n[^1]: two");
    assert!(result.reordered_block.is_none());
}

#[test]
fn fix_inline_typos_converts_and_renumbers() {
    let text = "a^[2] b[^1]\n\n[^1]: one\n[^2]: two";
    let fix = fix_inline_typos(text, text.len());
    let Some(fix) = fix else {
        panic!("expected typos");
    };
    assert_eq!(fix.fixed, 1);
    let new_text = apply_edits(text, &fix.edits);
    assert_eq!(new_text, "a[^1] b[^2]\n\n[^1]: two\n[^2]: one");
    assert_eq!(fix.cursor, new_text.len());
    assert_eq!(fix_typos_message(fix.fixed), "Fixed 1 inline footnote.");
    assert!(fix_inline_typos("no typos `^[1]`", 0).is_none());
    assert_eq!(fix_typos_message(0), "No inline footnote typos found.");
}

#[test]
fn highlight_keeps_the_most_important_problem_per_span() {
    let text = "a[^1]\n[^1]: one\n[^2]:\n[^2]: again";
    let problems = highlight_problems(text);
    let kinds: Vec<FootnoteProblemKind> = problems.iter().map(|p| p.kind).collect();
    assert_eq!(
        kinds,
        [
            FootnoteProblemKind::Duplicate,
            FootnoteProblemKind::Duplicate
        ]
    );
    assert_eq!(&text[problems[0].range.clone()], "[^2]:");
}

#[test]
fn auto_renumber_waits_while_cursor_is_in_the_definition_block() {
    let text = "X[^2] Y[^1]\n[^1]: one\n[^2]: two";
    let settings = FootnoteSettings::default();
    let mut auto = AutoRenumber::new();
    assert!(auto.on_idle(text, text.len(), &settings).is_none());
    let disabled = FootnoteSettings {
        auto_renumber_on_edit: false,
        ..settings
    };
    assert!(auto.on_idle(text, 0, &disabled).is_none());
    let Some((edits, cursor)) = auto.on_idle(text, 3, &settings) else {
        panic!("expected a renumber");
    };
    let new_text = apply_edits(text, &edits);
    assert_eq!(&new_text[..cursor], "X[^1]");
}

#[test]
fn tidy_messages() {
    let blocked = compute_renumber("a[^1] b[^9]\n[^1]: one");
    assert!(tidy_message(&blocked, false).contains("[^9]"));
    let typo = compute_renumber("a[^1] b^[3]\n[^1]: one");
    assert_eq!(
        tidy_message(&typo, false),
        "Footnotes already in order. Also found 1 inline typo. Run \"Convert inline footnote typos\"."
    );
    let applied = apply_renumber("X[^2] Y[^1]\n[^1]: one\n[^2]: two", 0, false);
    assert_eq!(
        tidy_message(&applied.result, applied.applied),
        "Footnotes renumbered."
    );
}

#[test]
fn problem_ranges_point_at_the_syntax() {
    let text = "x[^3]\n  [^4]:  ";
    let problems = find_problems(text);
    let spans: Vec<(&str, &str)> = problems
        .iter()
        .map(|p| (p.kind.id(), &text[p.range.clone()]))
        .collect();
    assert_eq!(
        spans,
        [
            ("dangling-ref", "[^3]"),
            ("orphan-def", "[^4]:"),
            ("empty-def", "[^4]:")
        ]
    );
}
