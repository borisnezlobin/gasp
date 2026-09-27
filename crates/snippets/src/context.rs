//! The kind of text the cursor is in, and the key that asked for an expansion.

use serde::{Deserialize, Serialize};

/// The kind of text the cursor is in.
///
/// This mirrors the input contexts shared across the app; it is local so this crate
/// doesn't depend on the config crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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

const CONTEXT_NAMES: [(InputContext, &str); 8] = [
    (InputContext::Text, "text"),
    (InputContext::Math, "math"),
    (InputContext::Code, "code"),
    (InputContext::Link, "link"),
    (InputContext::Frontmatter, "frontmatter"),
    (InputContext::Table, "table"),
    (InputContext::Html, "html"),
    (InputContext::Comment, "comment"),
];

impl InputContext {
    /// Every context, in a fixed order.
    pub const ALL: [InputContext; 8] = [
        InputContext::Text,
        InputContext::Math,
        InputContext::Code,
        InputContext::Link,
        InputContext::Frontmatter,
        InputContext::Table,
        InputContext::Html,
        InputContext::Comment,
    ];

    /// The lowercase name used in files, such as `math`.
    pub fn name(self) -> &'static str {
        CONTEXT_NAMES
            .iter()
            .find(|(context, _)| *context == self)
            .map_or("text", |(_, name)| name)
    }

    /// Looks a context up by its lowercase name.
    pub fn from_name(name: &str) -> Option<InputContext> {
        CONTEXT_NAMES
            .iter()
            .find(|(_, candidate)| *candidate == name)
            .map(|(context, _)| *context)
    }
}

/// The key that asked for an expansion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerKey {
    /// Tab was pressed. It has not been inserted into the text.
    Tab,
    /// A character was typed. Without a selection it is already the last character of
    /// the text before the cursor; with a selection it would replace the selection.
    Char(char),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for context in InputContext::ALL {
            assert_eq!(InputContext::from_name(context.name()), Some(context));
        }
        assert_eq!(InputContext::from_name("prose"), None);
    }
}
