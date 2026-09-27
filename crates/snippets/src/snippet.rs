//! The data model of one snippet: a trigger, an expansion and plain-word options.

use crate::context::InputContext;
use crate::pattern::NamedPattern;

/// Marks a tab stop in the readable format.
pub const STOP_GLYPH: char = '●';
/// Stands for a space where a bare space would be ambiguous.
pub const SPACE_GLYPH: char = '␣';
/// Stands for a line break.
pub const NEWLINE_GLYPH: char = '⏎';
/// Stands for a tab character.
pub const TAB_GLYPH: char = '⇥';
/// Separates the trigger from the expansion.
pub const ARROW: char = '→';
/// Starts a raw-regex trigger.
pub const REGEX_PREFIX: &str = "regex:";

/// One snippet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snippet {
    pub trigger: Trigger,
    pub expansion: Expansion,
    pub options: Options,
}

/// What has to be typed for a snippet to fire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// Literal text and named patterns, such as `{letter}{digit}`.
    Pattern(Vec<TriggerPart>),
    /// A raw regex, matched against the end of the text before the cursor. Named
    /// patterns such as `{greek}` inside it are expanded; capture groups are referred
    /// to as `{group1}`, `{group2}` and so on.
    Regex(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerPart {
    Text(String),
    Named(NamedPattern),
}

impl Trigger {
    /// How many times a named pattern occurs in the trigger.
    pub fn pattern_count(&self, pattern: NamedPattern) -> usize {
        match self {
            Trigger::Pattern(parts) => parts
                .iter()
                .filter(|part| **part == TriggerPart::Named(pattern))
                .count(),
            Trigger::Regex(_) => 0,
        }
    }

    /// The trigger's text when it is a single literal, such as `(` for a selection snippet.
    pub fn literal(&self) -> Option<&str> {
        match self {
            Trigger::Pattern(parts) => match parts.as_slice() {
                [TriggerPart::Text(text)] => Some(text),
                _ => None,
            },
            Trigger::Regex(_) => None,
        }
    }
}

/// What a snippet inserts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Expansion {
    pub parts: Vec<ExpansionPart>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExpansionPart {
    Text(String),
    Stop(StopMark),
    Capture(CaptureRef),
    /// The selected text, for snippets that run `on selection`.
    Selection,
}

/// A tab stop in an expansion.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StopMark {
    /// `None` for `●`, which numbers stops in order. `Some(0)` is the final stop.
    pub number: Option<u32>,
    /// Text that is inserted and selected when the stop is reached.
    pub placeholder: Option<String>,
}

/// A reference in the expansion to text the trigger matched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureRef {
    /// The nth (1-based) occurrence of a named pattern in the trigger.
    Named {
        pattern: NamedPattern,
        occurrence: usize,
    },
    /// A capture group of a raw-regex trigger (1-based).
    Group(usize),
}

/// Where a snippet may fire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    Context(InputContext),
    InlineMath,
    BlockMath,
}

impl Scope {
    /// Whether a snippet with this scope may fire in the given context.
    pub fn allows(self, context: InputContext, block_math: bool) -> bool {
        match self {
            Scope::Context(scope) => scope == context,
            Scope::InlineMath => context == InputContext::Math && !block_math,
            Scope::BlockMath => context == InputContext::Math && block_math,
        }
    }
}

/// When a snippet fires.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fire {
    /// When Tab is pressed after the trigger.
    #[default]
    OnTab,
    /// As soon as the trigger is typed.
    Instant,
}

/// The plain-word options after the expansion.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Empty means `anywhere`.
    pub scopes: Vec<Scope>,
    pub fire: Fire,
    /// The trigger must not follow a letter or a backslash, nor be followed by a letter.
    pub whole_word: bool,
    /// The trigger fires on the space typed after it, and the space is replaced too.
    pub after_space: bool,
    /// The trigger must not follow any of these characters.
    pub not_after: Option<String>,
    /// The snippet wraps the selection when its one-character trigger is typed.
    pub on_selection: bool,
    /// Higher runs first. Ties go to the longer match, then to the earlier snippet.
    pub priority: i32,
}

impl Options {
    /// Whether the snippet may fire in the given context.
    pub fn allows(&self, context: InputContext, block_math: bool) -> bool {
        self.scopes.is_empty()
            || self
                .scopes
                .iter()
                .any(|scope| scope.allows(context, block_math))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_scopes_mean_anywhere() {
        let options = Options::default();
        assert!(options.allows(InputContext::Code, false));
    }

    #[test]
    fn block_and_inline_math_are_told_apart() {
        assert!(Scope::BlockMath.allows(InputContext::Math, true));
        assert!(!Scope::BlockMath.allows(InputContext::Math, false));
        assert!(Scope::InlineMath.allows(InputContext::Math, false));
        assert!(!Scope::InlineMath.allows(InputContext::Text, false));
    }
}
