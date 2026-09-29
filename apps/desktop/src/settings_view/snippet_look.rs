//! What a row on the Snippets page shows, worked out from the snippet or
//! replacement once per layout: the keys typed, what they turn into, and
//! where and when that happens.
//!
//! The row reads "type this, get this". What you get is shown as it will
//! look, not as it's written: math is rendered, and what can't be
//! rendered is shown as its source with each place the cursor stops drawn
//! as a mark and each line break as a break. Parts of the trigger that
//! stand for any letter or digit are shown as an example, such as `x` and
//! `2`, and the same example fills them in the result.

use gasp_snippets::{
    CaptureRef, ExpansionPart, Fire, InputContext, NamedPattern, Replacement, ReplacementFire,
    Scope, Snippet, Trigger, TriggerPart,
};

/// One piece of the keys a trigger is typed with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyPiece {
    /// Characters typed as they are.
    Text(String),
    /// An example of a part that stands for any letter, digit or name.
    Example(String),
    /// A space, drawn as a mark so it can be seen.
    Space,
}

/// What has to be typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerLook {
    Keys(Vec<KeyPiece>),
    /// A raw pattern, which is shown as the code it is.
    Pattern(String),
}

/// One piece of what a snippet gives, where it isn't rendered as math.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResultPiece {
    Text(String),
    /// Text the trigger matched or a placeholder, shown as an example.
    Example(String),
    /// Where the cursor stops to be typed into.
    Slot,
    Break,
}

/// Where a snippet or replacement works.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Anywhere,
    Text,
    Math,
    Code,
}

impl Place {
    /// The place in words, as its tooltip starts.
    pub fn words(self) -> &'static str {
        match self {
            Place::Anywhere => "Works anywhere",
            Place::Text => "Works in text",
            Place::Math => "Works in math",
            Place::Code => "Works in code",
        }
    }
}

/// Everything a row shows for one snippet or replacement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnippetLook {
    pub trigger: TriggerLook,
    /// What the example parts of the trigger stand for, in words.
    pub trigger_note: Option<String>,
    /// The LaTeX to render as the result, when it's math.
    pub math: Option<String>,
    /// The result as pieces: shown when there's no math, or while it
    /// renders, or when it can't.
    pub result: Vec<ResultPiece>,
    /// Whether the pieces are LaTeX source, drawn in the code font.
    pub code: bool,
    pub places: Vec<Place>,
    /// The words each place's tooltip says after the place.
    pub place_note: String,
    pub on_tab: bool,
    pub on_selection: bool,
}

/// Said by the Tab key's tooltip.
pub const TAB_WORDS: &str = "Fires when you press Tab after typing it";
/// Said by the selection mark's tooltip.
pub const SELECTION_WORDS: &str = "Select some text first, then type it to wrap the selection";

impl SnippetLook {
    pub fn of_snippet(snippet: &Snippet) -> SnippetLook {
        let options = &snippet.options;
        let math_scoped = !options.scopes.is_empty() && options.scopes.iter().all(is_math);
        let (trigger, trigger_note) = trigger_look(&snippet.trigger);
        SnippetLook {
            trigger,
            trigger_note,
            math: math_tex(snippet, math_scoped),
            result: result_pieces(snippet),
            code: math_scoped,
            places: snippet_places(&options.scopes),
            place_note: extras(options.whole_word, options.after_space),
            on_tab: options.fire == Fire::OnTab,
            on_selection: options.on_selection,
        }
    }

    pub fn of_replacement(entry: &Replacement) -> SnippetLook {
        let places = match &entry.contexts {
            None => vec![Place::Text],
            Some(contexts) => dedup(contexts.iter().map(|context| context_place(*context))),
        };
        let outside = if entry.contexts.is_none() {
            ", outside code and math"
        } else {
            ""
        };
        let start = if entry.word_start {
            ", at the start of a word"
        } else {
            ""
        };
        let after_space = extras(false, entry.fire == ReplacementFire::AfterSpace);
        let place_note = format!("{outside}{start}{after_space}");
        SnippetLook {
            trigger: TriggerLook::Keys(key_pieces(&entry.from)),
            trigger_note: None,
            math: None,
            result: text_pieces(&entry.to),
            code: false,
            places,
            place_note,
            on_tab: false,
            on_selection: false,
        }
    }

    /// Each mark's tooltip, in the order the row draws the marks: when
    /// it fires, where it works, and whether it wraps a selection. As
    /// you type is the usual way, so it has no mark.
    pub fn tooltips(&self) -> Vec<String> {
        let mut tips = Vec::new();
        if self.on_tab {
            tips.push(TAB_WORDS.to_string());
        }
        tips.extend(
            self.places
                .iter()
                .map(|place| format!("{}{}.", place.words(), self.place_note)),
        );
        if self.on_selection {
            tips.push(SELECTION_WORDS.to_string());
        }
        tips
    }

    /// Every word the row's tooltips say, for search.
    pub fn tooltip_words(&self) -> String {
        let mut words = self.tooltips();
        words.extend(self.trigger_note.clone());
        words.join(" ")
    }
}

fn is_math(scope: &Scope) -> bool {
    matches!(
        scope,
        Scope::Context(InputContext::Math) | Scope::InlineMath | Scope::BlockMath
    )
}

fn context_place(context: InputContext) -> Place {
    match context {
        InputContext::Math => Place::Math,
        InputContext::Code => Place::Code,
        _ => Place::Text,
    }
}

fn snippet_places(scopes: &[Scope]) -> Vec<Place> {
    if scopes.is_empty() {
        return vec![Place::Anywhere];
    }
    dedup(scopes.iter().map(|scope| match scope {
        Scope::Context(context) => context_place(*context),
        Scope::InlineMath | Scope::BlockMath => Place::Math,
    }))
}

fn dedup(places: impl Iterator<Item = Place>) -> Vec<Place> {
    let mut out: Vec<Place> = Vec::new();
    for place in places {
        if !out.contains(&place) {
            out.push(place);
        }
    }
    out
}

/// The conditions after the place, such as ", as a whole word".
fn extras(whole_word: bool, after_space: bool) -> String {
    let mut out = String::new();
    if whole_word {
        out.push_str(", as a whole word");
    }
    if after_space {
        out.push_str(", when you type a space after it");
    }
    out
}

/// The example a named pattern's `occurrence` (1-based) is shown as.
pub fn example(pattern: NamedPattern, occurrence: usize) -> &'static str {
    let examples: &[&str] = match pattern {
        NamedPattern::Letter => &["x", "y", "z"],
        NamedPattern::Digit => &["2", "3", "4"],
        NamedPattern::Greek => &["alpha", "beta", "gamma"],
        NamedPattern::Symbol => &["infty", "nabla", "partial"],
        NamedPattern::Word => &["word", "other", "more"],
    };
    examples[occurrence.saturating_sub(1).min(examples.len() - 1)]
}

fn pattern_words(pattern: NamedPattern) -> &'static str {
    match pattern {
        NamedPattern::Letter => "any letter",
        NamedPattern::Digit => "any digit",
        NamedPattern::Greek => "any Greek letter’s name",
        NamedPattern::Symbol => "any symbol’s name",
        NamedPattern::Word => "any word",
    }
}

fn trigger_look(trigger: &Trigger) -> (TriggerLook, Option<String>) {
    let parts = match trigger {
        Trigger::Regex(source) => return (TriggerLook::Pattern(source.clone()), None),
        Trigger::Pattern(parts) => parts,
    };
    let mut pieces = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut seen: Vec<NamedPattern> = Vec::new();
    for part in parts {
        match part {
            TriggerPart::Text(text) => push_keys(&mut pieces, text),
            TriggerPart::Named(pattern) => {
                seen.push(*pattern);
                let occurrence = seen.iter().filter(|p| *p == pattern).count();
                let shown = example(*pattern, occurrence);
                notes.push(format!("{shown} stands for {}", pattern_words(*pattern)));
                pieces.push(KeyPiece::Example(shown.to_string()));
            }
        }
    }
    let note = (!notes.is_empty()).then(|| capitalise_first(&notes.join(", ")));
    (TriggerLook::Keys(pieces), note)
}

fn capitalise_first(text: &str) -> String {
    super::snippets_page::capitalised(text)
}

/// Typed text as keys, with spaces as their own mark.
pub fn key_pieces(text: &str) -> Vec<KeyPiece> {
    let mut pieces = Vec::new();
    push_keys(&mut pieces, text);
    pieces
}

fn push_keys(pieces: &mut Vec<KeyPiece>, text: &str) {
    for c in text.chars() {
        if c == ' ' {
            pieces.push(KeyPiece::Space);
            continue;
        }
        match pieces.last_mut() {
            Some(KeyPiece::Text(run)) => run.push(c),
            _ => pieces.push(KeyPiece::Text(c.to_string())),
        }
    }
}

/// Plain text as pieces, breaking at each new line.
pub fn text_pieces(text: &str) -> Vec<ResultPiece> {
    let mut pieces = Vec::new();
    push_text(&mut pieces, text);
    pieces
}

fn push_text(pieces: &mut Vec<ResultPiece>, text: &str) {
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            pieces.push(ResultPiece::Break);
        }
        if line.is_empty() {
            continue;
        }
        match pieces.last_mut() {
            Some(ResultPiece::Text(run)) => run.push_str(line),
            _ => pieces.push(ResultPiece::Text(line.to_string())),
        }
    }
}

/// The parts that show: the expansion without the stops at its end,
/// which only say where the cursor goes once it's filled in.
fn shown_parts(parts: &[ExpansionPart]) -> &[ExpansionPart] {
    let end = parts
        .iter()
        .rposition(|part| match part {
            ExpansionPart::Stop(stop) => stop.placeholder.is_some(),
            ExpansionPart::Text(text) => !text.trim().is_empty(),
            _ => true,
        })
        .map_or(0, |last| last + 1);
    &parts[..end]
}

fn capture_example(capture: &CaptureRef) -> Option<&'static str> {
    match capture {
        CaptureRef::Named {
            pattern,
            occurrence,
        } => Some(example(*pattern, *occurrence)),
        CaptureRef::Group(_) => None,
    }
}

fn result_pieces(snippet: &Snippet) -> Vec<ResultPiece> {
    let mut pieces = Vec::new();
    for part in shown_parts(&snippet.expansion.parts) {
        match part {
            ExpansionPart::Text(text) => push_text(&mut pieces, text),
            ExpansionPart::Stop(stop) => pieces.push(match &stop.placeholder {
                Some(placeholder) => ResultPiece::Example(placeholder.clone()),
                None => ResultPiece::Slot,
            }),
            ExpansionPart::Capture(capture) => pieces.push(match capture_example(capture) {
                Some(example) => ResultPiece::Example(example.to_string()),
                None => ResultPiece::Slot,
            }),
            ExpansionPart::Selection => pieces.push(ResultPiece::Slot),
        }
    }
    pieces
}

/// The LaTeX a snippet's result renders as: all of it when the snippet
/// works in math, or what's between the dollars of text that opens and
/// closes math. Each place to type into is an empty box, as the live
/// preview draws an empty argument. `None` when there's nothing to render
/// or the result depends on a raw pattern's groups.
fn math_tex(snippet: &Snippet, math_scoped: bool) -> Option<String> {
    let mut tex = String::new();
    for part in shown_parts(&snippet.expansion.parts) {
        match part {
            ExpansionPart::Text(text) => tex.push_str(text),
            ExpansionPart::Stop(stop) => match &stop.placeholder {
                Some(placeholder) => tex.push_str(placeholder),
                None => push_slot(&mut tex),
            },
            ExpansionPart::Capture(capture) => tex.push_str(capture_example(capture)?),
            ExpansionPart::Selection => push_slot(&mut tex),
        }
    }
    let tex = if math_scoped {
        tex.trim().to_string()
    } else {
        let inner = tex.trim().strip_prefix('$')?.strip_suffix('$')?;
        let inner = inner.strip_prefix('$').unwrap_or(inner);
        let inner = inner.strip_suffix('$').unwrap_or(inner);
        inner.trim().to_string()
    };
    let visible = tex.replace(SLOT_TEX, "").replace(SLOT_CHAR, "");
    (!visible.trim().is_empty()).then_some(tex)
}

/// An empty box, where the cursor stops to be typed into.
const SLOT_TEX: &str = "\\square";
/// The same box inside text, such as `\text{…}`, which takes no commands.
const SLOT_CHAR: &str = "□";

fn push_slot(tex: &mut String) {
    if in_text_argument(tex) {
        tex.push_str(SLOT_CHAR);
    } else {
        tex.push_str(SLOT_TEX);
        tex.push(' ');
    }
}

/// Whether the end of `tex` is inside the argument of a text command,
/// such as `\text{`.
fn in_text_argument(tex: &str) -> bool {
    let mut open: Vec<usize> = Vec::new();
    for (index, c) in tex.char_indices() {
        match c {
            '{' => open.push(index),
            '}' => {
                open.pop();
            }
            _ => {}
        }
    }
    open.last().is_some_and(|&at| {
        let before = &tex[..at];
        let name_start = before.rfind('\\').map_or(before.len(), |slash| slash + 1);
        let name = &before[name_start..];
        name.starts_with("text") && name.chars().all(|c| c.is_ascii_alphabetic())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gasp_snippets::{Replacement, parse_snippet};

    fn look(line: &str) -> SnippetLook {
        SnippetLook::of_snippet(&parse_snippet(line).unwrap())
    }

    #[test]
    fn math_results_render_with_boxes_for_the_stops() {
        assert_eq!(
            look("reals → \\mathbb{R}  math, instant").math.as_deref(),
            Some("\\mathbb{R}")
        );
        assert_eq!(
            look("// → \\frac{●}{●}●  math, instant").math.as_deref(),
            Some("\\frac{\\square }{\\square }")
        );
    }

    #[test]
    fn examples_fill_in_named_patterns() {
        let look = look("{letter}{digit} → {letter}_{{digit}}  math, instant");
        assert_eq!(look.math.as_deref(), Some("x_{2}"));
        assert_eq!(
            look.trigger,
            TriggerLook::Keys(vec![
                KeyPiece::Example("x".into()),
                KeyPiece::Example("2".into())
            ])
        );
        assert_eq!(
            look.trigger_note.as_deref(),
            Some("X stands for any letter, 2 stands for any digit")
        );
    }

    #[test]
    fn an_empty_math_opener_shows_its_source_with_a_slot() {
        let look = look("mk → $●$  text, instant");
        assert_eq!(look.math, None);
        assert!(!look.code);
        assert_eq!(
            look.result,
            [
                ResultPiece::Text("$".into()),
                ResultPiece::Slot,
                ResultPiece::Text("$".into())
            ]
        );
        let look = super::SnippetLook::of_snippet(&parse_snippet("dm → $$⏎●⏎$$  text").unwrap());
        assert_eq!(
            look.result,
            [
                ResultPiece::Text("$$".into()),
                ResultPiece::Break,
                ResultPiece::Slot,
                ResultPiece::Break,
                ResultPiece::Text("$$".into())
            ]
        );
        assert!(look.on_tab);
    }

    #[test]
    fn a_stop_in_text_is_a_box_character() {
        assert_eq!(
            look("text → \\text{●}●  math, instant").math.as_deref(),
            Some("\\text{□}")
        );
    }

    #[test]
    fn dollars_around_math_render_it() {
        let look = look("al → $\\alpha$  text, instant");
        assert_eq!(look.math.as_deref(), Some("\\alpha"));
    }

    #[test]
    fn raw_patterns_keep_to_their_source() {
        let look = look("regex:a(b) → \\{group1}  math, instant");
        assert_eq!(look.math, None);
        assert_eq!(look.trigger, TriggerLook::Pattern("a(b)".into()));
        assert!(look.result.contains(&ResultPiece::Slot));
    }

    #[test]
    fn tooltips_say_where_and_when() {
        let look = look("in → \\in␣  math, whole word, after space");
        assert_eq!(look.places, [Place::Math]);
        let words = look.tooltip_words();
        assert!(words.contains("Works in math, as a whole word, when you type a space after it"));
        assert!(words.contains("Tab"));
        let dash = SnippetLook::of_replacement(&Replacement::new("--", "—", "dashes"));
        assert_eq!(dash.result, [ResultPiece::Text("—".into())]);
        assert_eq!(dash.places, [Place::Text]);
        assert!(dash.tooltip_words().contains("outside code and math"));
    }
}
