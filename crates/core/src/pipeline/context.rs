//! Input contexts and the filters that limit a step to some of them.

use std::fmt;
use std::str::FromStr;

use crate::document::Document;

/// What kind of Markdown the cursor is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InputContext {
    Text,
    Math,
    Code,
    Link,
    Frontmatter,
    Table,
    Html,
    Comment,
}

impl InputContext {
    pub const ALL: [Self; 8] = [
        Self::Text,
        Self::Math,
        Self::Code,
        Self::Link,
        Self::Frontmatter,
        Self::Table,
        Self::Html,
        Self::Comment,
    ];

    /// The name used in config files, such as `math`.
    pub fn name(self) -> &'static str {
        const NAMES: [&str; 8] = [
            "text",
            "math",
            "code",
            "link",
            "frontmatter",
            "table",
            "html",
            "comment",
        ];
        NAMES[self as usize]
    }

    fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

impl fmt::Display for InputContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// The text was not one of the context names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownContext(pub String);

impl fmt::Display for UnknownContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown input context `{}`", self.0)
    }
}

impl std::error::Error for UnknownContext {}

impl FromStr for InputContext {
    type Err = UnknownContext;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|context| context.name() == text)
            .ok_or_else(|| UnknownContext(text.to_owned()))
    }
}

/// A set of input contexts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ContextSet(u8);

impl ContextSet {
    pub const EMPTY: Self = Self(0);
    pub const ALL: Self = Self(u8::MAX);

    pub fn of(contexts: &[InputContext]) -> Self {
        contexts
            .iter()
            .fold(Self::EMPTY, |set, context| set.with(*context))
    }

    pub fn with(self, context: InputContext) -> Self {
        Self(self.0 | context.bit())
    }

    pub fn without(self, context: InputContext) -> Self {
        Self(self.0 & !context.bit())
    }

    pub fn contains(self, context: InputContext) -> bool {
        self.0 & context.bit() != 0
    }
}

/// Where a step runs: in an allowed context that is not denied.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContextFilter {
    pub allow: ContextSet,
    pub deny: ContextSet,
}

impl ContextFilter {
    /// Runs everywhere.
    pub fn any() -> Self {
        Self {
            allow: ContextSet::ALL,
            deny: ContextSet::EMPTY,
        }
    }

    /// Runs only in the given contexts.
    pub fn only(contexts: &[InputContext]) -> Self {
        Self {
            allow: ContextSet::of(contexts),
            deny: ContextSet::EMPTY,
        }
    }

    /// Runs everywhere except the given contexts.
    pub fn except(contexts: &[InputContext]) -> Self {
        Self {
            allow: ContextSet::ALL,
            deny: ContextSet::of(contexts),
        }
    }

    pub fn allows(self, context: InputContext) -> bool {
        self.allow.contains(context) && !self.deny.contains(context)
    }
}

impl Default for ContextFilter {
    fn default() -> Self {
        Self::any()
    }
}

/// The math around an offset: `$…$`, `$$…$$` on one line, or a `$$`
/// block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MathSpan {
    /// The whole span, delimiters included.
    pub outer: std::ops::Range<usize>,
    /// The source between the delimiters. An unclosed span runs to the end
    /// of `outer`.
    pub inner: std::ops::Range<usize>,
    /// Whether it's `$$` math rather than inline `$`.
    pub block: bool,
}

/// Tells the pipeline what context an offset is in. The parser implements
/// this for real documents.
pub trait ContextProvider {
    fn context_at(&self, doc: &Document, offset: usize) -> InputContext;

    /// The math the offset is in, when it's in math and the provider knows
    /// where that math starts and ends.
    fn math_at(&self, _doc: &Document, _offset: usize) -> Option<MathSpan> {
        None
    }
}

/// Reports the same context everywhere, `text` by default. For tests and for
/// documents that have not been parsed yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedContext(pub InputContext);

impl Default for FixedContext {
    fn default() -> Self {
        Self(InputContext::Text)
    }
}

impl ContextProvider for FixedContext {
    fn context_at(&self, _doc: &Document, _offset: usize) -> InputContext {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for context in InputContext::ALL {
            assert_eq!(context.name().parse::<InputContext>(), Ok(context));
        }
        assert!("prose".parse::<InputContext>().is_err());
    }

    #[test]
    fn filters_allow_and_deny() {
        let no_math = ContextFilter::except(&[InputContext::Math]);
        assert!(no_math.allows(InputContext::Text));
        assert!(!no_math.allows(InputContext::Math));
        let only_code = ContextFilter::only(&[InputContext::Code]);
        assert!(only_code.allows(InputContext::Code));
        assert!(!only_code.allows(InputContext::Text));
        assert!(ContextFilter::any().allows(InputContext::Comment));
    }

    #[test]
    fn set_operations() {
        let set = ContextSet::of(&[InputContext::Html, InputContext::Table]);
        assert!(set.contains(InputContext::Html));
        assert!(!set.without(InputContext::Html).contains(InputContext::Html));
        assert!(set.with(InputContext::Link).contains(InputContext::Link));
    }
}
