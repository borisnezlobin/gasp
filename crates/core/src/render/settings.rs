//! When Markdown symbols are shown.

use std::collections::HashMap;

use crate::syntax::SyntaxKind;

/// How much around the cursor counts as "near" for [`RevealMode::AroundCursor`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RevealScope {
    /// Only the element the cursor touches, like Typora.
    Element,
    /// Every element on the cursor's line.
    Line,
    /// Every element in the cursor's block, such as its paragraph.
    Block,
}

/// When a kind of Markdown symbol is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RevealMode {
    AlwaysShown,
    AroundCursor { scope: RevealScope },
    AlwaysHidden,
}

/// The reveal mode, with per-syntax overrides such as keeping link URLs
/// hidden while emphasis follows the cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevealSettings {
    pub mode: RevealMode,
    pub overrides: HashMap<SyntaxKind, RevealMode>,
    /// Whether brackets in shown math source are coloured by how deeply
    /// they nest, so the two halves of a pair match.
    pub bracket_colours: bool,
}

impl Default for RevealSettings {
    fn default() -> Self {
        Self::new(RevealMode::AroundCursor {
            scope: RevealScope::Element,
        })
    }
}

impl RevealSettings {
    pub fn new(mode: RevealMode) -> Self {
        Self {
            mode,
            overrides: HashMap::new(),
            bracket_colours: true,
        }
    }

    /// Returns the settings with `kind` overridden to `mode`.
    pub fn with_override(mut self, kind: SyntaxKind, mode: RevealMode) -> Self {
        self.overrides.insert(kind, mode);
        self
    }

    pub fn mode_for(&self, kind: SyntaxKind) -> RevealMode {
        self.overrides.get(&kind).copied().unwrap_or(self.mode)
    }
}
