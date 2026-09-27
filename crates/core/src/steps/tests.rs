use editor_snippets::{Replacements, SnippetEngine, SnippetFile};

use super::install_typing_steps;
use crate::document::{Document, Selection};
use crate::history::EditorState;
use crate::pipeline::{EditRequest, Pipeline};
use crate::syntax;

const SNIPPETS: &str = "\
mk      → $●$           text, instant
dm      → $$⏎●⏎$$       text, instant, whole word
@a      → \\alpha        math, instant
//      → \\frac{●}{●}   math, instant
sr      → ^{2}          math, instant
bb      → \\mathbf{●}    block math, instant
";

const REPLACEMENTS: &str = r#"
[[replacement]]
from = "--"
to = "—"
group = "Dashes"
enabled = true
fire = "instant"

[[replacement]]
from = "->"
to = "→"
group = "Arrows"
enabled = true
fire = "instant"
"#;

fn pipeline() -> Pipeline {
    let file = SnippetFile::parse(SNIPPETS).expect("test snippets parse");
    let engine = SnippetEngine::new(file.snippets().cloned().collect()).expect("snippets compile");
    let table = Replacements::from_toml(REPLACEMENTS).expect("replacements parse");
    let mut pipeline = Pipeline::builtin();
    install_typing_steps(&mut pipeline, engine, table).expect("slots exist");
    pipeline
}

struct Typist {
    state: EditorState,
    pipeline: Pipeline,
    clock: u64,
}

impl Typist {
    fn new(text: &str, caret: usize) -> Typist {
        let mut state = EditorState::new(Document::from(text));
        let select = crate::transaction::Transaction::select(
            Selection::cursor(caret),
            crate::transaction::Origin::Other("test".into()),
            0,
        );
        state.apply(select).expect("selection applies");
        Typist {
            state,
            pipeline: pipeline(),
            clock: 1,
        }
    }

    fn send(&mut self, request: EditRequest) {
        let text = self.state.doc().slice(0..self.state.doc().len());
        let tree = syntax::parse(&text);
        self.clock += 10;
        let transactions = self.pipeline.run_steps(
            request,
            self.state.doc(),
            self.state.selection(),
            &tree,
            self.clock,
        );
        for transaction in transactions {
            self.state.apply(transaction).expect("transaction applies");
        }
    }

    fn type_text(&mut self, text: &str) {
        for typed in text.chars() {
            self.send(EditRequest::InsertText(typed.to_string()));
        }
    }

    /// The text with `|` at the caret, or `[` and `]` around a selection.
    fn shown(&self) -> String {
        let mut text = self.state.doc().slice(0..self.state.doc().len());
        let range = self.state.selection().primary();
        if range.is_empty() {
            text.insert(range.from(), '|');
        } else {
            text.insert(range.to(), ']');
            text.insert(range.from(), '[');
        }
        text
    }
}

#[test]
fn text_snippet_opens_inline_math_with_the_caret_inside() {
    let mut typist = Typist::new("", 0);
    typist.type_text("See mk");
    assert_eq!(typist.shown(), "See $|$");
}

#[test]
fn math_snippets_fire_inside_math_only() {
    let mut typist = Typist::new("", 0);
    typist.type_text("@a mk@a");
    assert_eq!(typist.shown(), "@a $\\alpha|$");
}

#[test]
fn tab_stops_are_visited_in_order() {
    let mut typist = Typist::new("", 0);
    typist.type_text("mk//");
    assert_eq!(typist.shown(), "$\\frac{|}{}$");
}

#[test]
fn block_math_snippets_need_a_block() {
    let mut inline = Typist::new("", 0);
    inline.type_text("mkbb");
    assert_eq!(inline.shown(), "$bb|$");

    let mut block = Typist::new("$$\n\n$$", 3);
    block.type_text("bb");
    assert_eq!(block.shown(), "$$\n\\mathbf{|}\n$$");
}

#[test]
fn replacements_fire_in_text_but_not_in_math_or_code() {
    let mut typist = Typist::new("", 0);
    typist.type_text("a -- b -> c");
    assert_eq!(typist.shown(), "a — b → c|");

    let mut math = Typist::new("$x$", 2);
    math.type_text("->");
    assert_eq!(math.shown(), "$x->|$");

    let mut code = Typist::new("`x`", 2);
    code.type_text("--");
    assert_eq!(code.shown(), "`x--|`");
}

#[test]
fn an_expansion_undoes_in_one_step() {
    let mut typist = Typist::new("", 0);
    typist.type_text("x mk");
    let before_undo = typist.shown();
    assert!(typist.state.undo(10_000));
    assert_ne!(typist.shown(), before_undo);
    assert_eq!(typist.state.doc().slice(0..typist.state.doc().len()), "");
}

#[test]
fn undo_right_after_a_replacement_gives_back_what_was_typed() {
    let mut typist = Typist::new("", 0);
    typist.type_text("a --");
    assert_eq!(typist.shown(), "a —|");
    assert!(typist.state.undo(typist.clock));
    assert_eq!(typist.shown(), "a --|");
}

#[test]
fn replacements_shorter_than_what_they_replace_keep_the_caret() {
    let mut typist = Typist::new("", 0);
    typist.type_text("x -> y");
    assert_eq!(typist.shown(), "x → y|");
}
