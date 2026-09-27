//! Syntax highlighting for code blocks, written as classed `<span>`s so the
//! website's stylesheet picks the colours (and can change them for dark
//! mode). The classes are the editor's own seven kinds of code, so an
//! article reads like the note did.

use std::sync::LazyLock;

use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxSet};

use super::escape_text;

/// Scope prefixes and the class they give, checked from the innermost scope
/// out; the first match wins and `None` leaves text plain. It matches the
/// editor's table in `preview/code_highlight.rs`.
const SCOPE_CLASSES: [(&str, Option<&str>); 24] = [
    ("comment", Some("hl-comment")),
    ("punctuation.definition.comment", Some("hl-comment")),
    ("string", Some("hl-string")),
    ("punctuation.definition.string", Some("hl-string")),
    ("constant.numeric", Some("hl-number")),
    ("constant.character.escape", Some("hl-constant")),
    ("constant", Some("hl-constant")),
    ("variable.language", Some("hl-constant")),
    ("support.constant", Some("hl-constant")),
    ("keyword.operator", None),
    ("keyword", Some("hl-keyword")),
    ("storage", Some("hl-keyword")),
    ("entity.name.tag", Some("hl-keyword")),
    ("entity.name.function", Some("hl-function")),
    ("support.function", Some("hl-function")),
    ("variable.function", Some("hl-function")),
    ("meta.function-call.identifier", Some("hl-function")),
    ("entity.name.type", Some("hl-type")),
    ("entity.name.class", Some("hl-type")),
    ("entity.name.struct", Some("hl-type")),
    ("entity.other.inherited-class", Some("hl-type")),
    ("support.type", Some("hl-type")),
    ("support.class", Some("hl-type")),
    ("entity.other.attribute-name", Some("hl-constant")),
];

static SCOPES: LazyLock<Vec<(Scope, Option<&'static str>)>> = LazyLock::new(|| {
    SCOPE_CLASSES
        .iter()
        .filter_map(|(prefix, class)| Some((Scope::new(prefix).ok()?, *class)))
        .collect()
});

/// Typst's grammars, which PDF export loads too.
fn syntaxes() -> &'static SyntaxSet {
    &typst::text::RAW_SYNTAXES
}

fn class_of(stack: &ScopeStack) -> Option<&'static str> {
    stack.as_slice().iter().rev().find_map(|scope| {
        SCOPES
            .iter()
            .find(|(prefix, _)| prefix.is_prefix_of(*scope))
            .map(|(_, class)| *class)
    })?
}

/// `source` as escaped HTML, highlighted when `lang` names a known grammar.
pub(crate) fn highlight(source: &str, lang: Option<&str>) -> String {
    let syntax = lang.and_then(|lang| syntaxes().find_syntax_by_token(lang));
    let Some(syntax) = syntax else {
        return escape_text(source);
    };
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut spans = Spans::default();
    for (index, line) in source.split('\n').enumerate() {
        if index > 0 {
            spans.push("\n", None);
        }
        let Ok(operations) = state.parse_line(line, syntaxes()) else {
            spans.push(line, None);
            continue;
        };
        let mut last = 0;
        for (offset, operation) in operations {
            spans.push(&line[last..offset], class_of(&stack));
            last = offset;
            if stack.apply(&operation).is_err() {
                break;
            }
        }
        spans.push(&line[last..], class_of(&stack));
    }
    spans.finish()
}

/// Highlighted text, with neighbouring runs of the same class merged into
/// one span.
#[derive(Default)]
struct Spans {
    out: String,
    run: String,
    class: Option<&'static str>,
}

impl Spans {
    fn push(&mut self, text: &str, class: Option<&'static str>) {
        if text.is_empty() {
            return;
        }
        if class != self.class {
            self.flush();
            self.class = class;
        }
        self.run.push_str(text);
    }

    fn flush(&mut self) {
        if self.run.is_empty() {
            return;
        }
        let text = escape_text(&std::mem::take(&mut self.run));
        match self.class {
            Some(class) => {
                self.out.push_str("<span class=\"");
                self.out.push_str(class);
                self.out.push_str("\">");
                self.out.push_str(&text);
                self.out.push_str("</span>");
            }
            None => self.out.push_str(&text),
        }
    }

    fn finish(mut self) -> String {
        self.flush();
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_known_languages() {
        let html = highlight("fn main() {} // hi", Some("rust"));
        assert!(
            html.contains("<span class=\"hl-keyword\">fn</span>"),
            "{html}"
        );
        assert!(html.contains("<span class=\"hl-comment\">"), "{html}");
    }

    #[test]
    fn escapes_unknown_languages() {
        assert_eq!(highlight("a < b", Some("nope")), "a &lt; b");
        assert_eq!(highlight("a & b", None), "a &amp; b");
    }
}
