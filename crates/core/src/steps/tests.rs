use std::sync::Arc;

use editor_snippets::{Replacements, SnippetEngine, SnippetFile};

use super::install_typing_steps;
use crate::document::{Document, Selection};
use crate::history::EditorState;
use crate::pipeline::{EditRequest, Pipeline, TabStops, follow_stops};
use crate::syntax;

const SNIPPETS: &str = "\
mk      → $●$           text, instant
dm      → $$⏎●⏎$$       text, instant, whole word
@a      → \\alpha        math, instant
//      → \\frac{●}{●}   math, instant
sr      → ^{2}          math, instant
bb      → \\mathbf{●}    block math, instant
sum     → \\sum          math, instant
beg     → \\begin{●1}⏎●2⏎\\end{●1}  math, instant
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
    install_typing_steps(&mut pipeline, Arc::new(engine), Arc::new(table)).expect("slots exist");
    pipeline
}

struct Typist {
    state: EditorState,
    pipeline: Pipeline,
    stops: Option<TabStops>,
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
            stops: None,
            clock: 1,
        }
    }

    fn send(&mut self, request: EditRequest) {
        let text = self.state.doc().slice(0..self.state.doc().len());
        let tree = syntax::parse(&text);
        self.clock += 10;
        let output = self.pipeline.run_input(
            request,
            self.state.doc(),
            self.state.selection(),
            &tree,
            self.clock,
            self.stops.as_ref(),
        );
        for transaction in output.transactions.clone() {
            self.state.apply(transaction).expect("transaction applies");
        }
        self.stops = follow_stops(self.stops.take(), &output);
    }

    fn tab(&mut self) {
        self.send(EditRequest::Tab);
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
fn nothing_is_replaced_or_curled_inside_html_tags() {
    let mut typist = Typist::new("", 0);
    typist.type_text("<span style=\"color:red;\" title='a--b'>x -- \"y\"</span>");
    assert_eq!(
        typist.shown(),
        "<span style=\"color:red;\" title='a--b'>x — “y”</span>|"
    );

    let mut block = Typist::new("", 0);
    block.type_text("<p style=\"text-align: center;\">a -> b</p>");
    assert_eq!(block.shown(), "<p style=\"text-align: center;\">a → b</p>|");
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

#[test]
fn tab_visits_each_stop_then_leaves_the_snippet() {
    let mut typist = Typist::new("", 0);
    typist.type_text("mk//a");
    assert_eq!(typist.shown(), "$\\frac{a|}{}$");
    typist.tab();
    typist.type_text("b");
    assert_eq!(typist.shown(), "$\\frac{a}{b|}$");
    typist.tab();
    assert_eq!(typist.shown(), "$\\frac{a}{b}|$");
    assert!(typist.stops.is_none(), "the last stop ends the snippet");
}

#[test]
fn mirrored_stops_are_typed_in_together() {
    let mut typist = Typist::new("$$\n\n$$", 3);
    typist.type_text("begcases");
    assert_eq!(
        typist.state.doc().to_string(),
        "$$\n\\begin{cases}\n\n\\end{cases}\n$$"
    );
    typist.tab();
    assert_eq!(typist.shown(), "$$\n\\begin{cases}\n|\n\\end{cases}\n$$");
}

#[test]
fn a_slash_after_a_term_makes_a_fraction() {
    let mut typist = Typist::new("$a + x^2$", 8);
    typist.type_text("/");
    assert_eq!(typist.shown(), "$a + \\frac{x^2}{|}$");
    typist.type_text("3");
    typist.tab();
    assert_eq!(typist.shown(), "$a + \\frac{x^2}{3}|$");

    let mut parens = Typist::new("$(a+b)$", 6);
    parens.type_text("/");
    assert_eq!(parens.shown(), "$\\frac{a+b}{|}$");

    let mut exponent = Typist::new("$e^{x}$", 5);
    exponent.type_text("/");
    assert_eq!(exponent.shown(), "$e^{x/|}$");

    let mut text = Typist::new("and/or", 3);
    text.type_text("/");
    assert_eq!(text.shown(), "and/|/or");
}

#[test]
fn tab_and_enter_fill_in_a_matrix() {
    let text = "$$\n\\begin{pmatrix}\na\n\\end{pmatrix}\n$$";
    let mut typist = Typist::new(text, text.find("a\n").unwrap() + 1);
    typist.tab();
    typist.type_text("b");
    typist.send(EditRequest::Newline);
    typist.type_text("c");
    assert_eq!(
        typist.shown(),
        "$$\n\\begin{pmatrix}\na & b \\\\\nc|\n\\end{pmatrix}\n$$"
    );
}

#[test]
fn tab_jumps_past_brackets_and_out_of_math() {
    let mut typist = Typist::new("$(a) + b$ c", 3);
    typist.tab();
    assert_eq!(typist.shown(), "$(a)| + b$ c");

    let mut end = Typist::new("$(a) + b$ c", 8);
    end.tab();
    assert_eq!(end.shown(), "$(a) + b$| c");

    let mut block = Typist::new("$$\nx\n$$", 4);
    block.tab();
    assert_eq!(block.shown(), "$$\nx\n$$\n|");
}

#[test]
fn brackets_around_a_sum_grow_when_it_expands() {
    let mut typist = Typist::new("$(x)$", 2);
    typist.type_text("sum");
    assert_eq!(typist.shown(), "$\\left(\\sum|x\\right)$");
    assert!(typist.state.undo(typist.clock + 10_000));
    assert_eq!(typist.state.doc().to_string(), "$(x)$");
}
