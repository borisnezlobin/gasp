use super::*;

type Command = fn(&Document, &Selection, u64) -> Transaction;

/// Runs `command` on `marked`, where `|` is the caret and `{`…`}` a
/// selection, and returns the text with the mapped caret or selection
/// marked the same way.
fn run(marked: &str, command: Command) -> String {
    let (text, selection) = unmark(marked);
    let doc = Document::from(text.as_str());
    let transaction = command(&doc, &selection, 0);
    let after = transaction.changes.apply_to_string(&text).unwrap();
    mark(&after, selection.map(&transaction.changes).primary())
}

fn unmark(marked: &str) -> (String, Selection) {
    let at = |mark: char| marked.find(mark);
    let text: String = marked.chars().filter(|ch| !"|{}".contains(*ch)).collect();
    let range = match (at('|'), at('{'), at('}')) {
        (Some(caret), _, _) => SelectionRange::cursor(caret),
        (None, Some(open), Some(close)) => SelectionRange::new(open, close - 1),
        _ => panic!("mark the caret or a selection in {marked:?}"),
    };
    (text, Selection::single(range))
}

fn mark(text: &str, range: SelectionRange) -> String {
    let mut out = text.to_owned();
    if range.is_empty() {
        out.insert(range.head, '|');
    } else {
        out.insert(range.to(), '}');
        out.insert(range.from(), '{');
    }
    out
}

#[test]
fn a_list_item_nests_under_the_one_before_with_its_children() {
    assert_eq!(
        run("- a\n- b|c\n\t- d\n- e", indent),
        "- a\n\t- b|c\n\t\t- d\n- e"
    );
    assert_eq!(run("1. one\n2. t|wo", indent), "1. one\n\t2. t|wo");
}

#[test]
fn the_first_item_has_nothing_to_nest_under() {
    assert_eq!(run("- o|ne\n- two", indent), "- o|ne\n- two");
}

#[test]
fn an_empty_item_nests_as_enter_leaves_it() {
    assert_eq!(run("- one\n- |", indent), "- one\n\t- |");
    assert_eq!(run("- one\n\t- |", outdent), "- one\n- |");
    assert_eq!(run("- one\n\t- |", indent), "- one\n\t\t- |");
}

#[test]
fn a_rule_that_looks_like_a_list_stays_put() {
    assert_eq!(run("text\n\n{- - -}", indent), "text\n\n{- - -}");
}

#[test]
fn outdent_brings_a_nested_item_and_its_children_out() {
    assert_eq!(
        run("- a\n\t- |b\n\t\t- c\n\t- d", outdent),
        "- a\n- |b\n\t- c\n\t- d"
    );
    assert_eq!(run("- a|t the edge", outdent), "- a|t the edge");
}

#[test]
fn lists_indented_with_spaces_stay_with_spaces() {
    assert_eq!(run("- a\n  - b\n  - c|", indent), "- a\n  - b\n    - c|");
    assert_eq!(run("1. a\n2. b|", indent), "1. a\n\t2. b|");
    assert_eq!(run("- a\n  - b|\n    more", outdent), "- a\n- b|\n  more");
}

#[test]
fn every_line_of_a_paragraph_shifts() {
    assert_eq!(run("one\ntw|o\n\nthree", indent), "\tone\n\ttw|o\n\nthree");
    assert_eq!(run("\tone\n\ttw|o\n\nthree", outdent), "one\ntw|o\n\nthree");
    assert_eq!(run("pl|ain", outdent), "pl|ain");
}

#[test]
fn a_selection_moves_every_item_it_touches() {
    assert_eq!(
        run("- a\n- {b\n- c}\n- d", indent),
        "- a\n\t- {b\n\t- c}\n- d"
    );
    assert_eq!(
        run("- a\n\t- {b\n\t- c}\n- d", outdent),
        "- a\n- {b\n- c}\n- d"
    );
}

#[test]
fn a_selection_across_blocks_moves_each() {
    assert_eq!(run("{para\n\n- a\n- b}", indent), "\t{para\n\n- a\n\t- b}");
}

#[test]
fn code_blocks_keep_the_tab() {
    assert_eq!(run("```\nx|y\n```", indent), "```\nx\t|y\n```");
    assert_eq!(run("```\n{a\nb}\n```", indent), "```\n\t{a\n\tb}\n```");
    assert_eq!(run("```\n\ta|\n```", outdent), "```\na|\n```");
    assert_eq!(run("$$\nx|\n$$", indent), "$$\nx\t|\n$$");
}

#[test]
fn quotes_nest_and_come_out() {
    assert_eq!(run("> a\n> b|", indent), "> > a\n> > b|");
    assert_eq!(run("> > a|", outdent), "> a|");
    assert_eq!(run("> a|", outdent), "a|");
    assert_eq!(
        run("> [!note] Title\n> bo|dy", outdent),
        "[!note] Title\nbo|dy"
    );
}

#[test]
fn a_list_in_a_quote_nests_inside_the_quote() {
    assert_eq!(run("> - a\n> - b|", indent), "> - a\n> \t- b|");
    assert_eq!(run("> - a\n> \t- b|", outdent), "> - a\n> - b|");
}

#[test]
fn headings_type_a_tab_at_the_caret_and_selections_leave_them() {
    assert_eq!(run("# Ti|tle", indent), "# Ti\t|tle");
    assert_eq!(run("# {Title}", indent), "# {Title}");
    assert_eq!(run("# Ti|tle", outdent), "# Ti|tle");
}

#[test]
fn the_selection_stays_on_its_text() {
    assert_eq!(run("- a\n- {bc}", indent), "- a\n\t- {bc}");
    assert_eq!(run("- a\n|- b", indent), "- a\n\t|- b");
    assert_eq!(run("- a\n\t|- b", outdent), "- a\n|- b");
}
