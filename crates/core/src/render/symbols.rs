//! Turning the `markdown.symbols` settings into the render planner's
//! reveal settings.

use super::{RevealMode, RevealScope, RevealSettings};
use crate::syntax::SyntaxKind;
use editor_config::settings::{
    RevealScope as ConfigScope, SymbolMode, SymbolSettings, SyntaxKind as ConfigSyntax,
};

/// Which planner syntax kinds each settings kind covers. Emphasis, strong,
/// strikethrough and highlight delimiters share one planner kind.
const SYNTAX_MAP: [(ConfigSyntax, &[SyntaxKind]); 18] = [
    (ConfigSyntax::Emphasis, &[SyntaxKind::Emphasis]),
    (ConfigSyntax::Strong, &[SyntaxKind::Emphasis]),
    (ConfigSyntax::Strikethrough, &[SyntaxKind::Emphasis]),
    (ConfigSyntax::Highlight, &[SyntaxKind::Emphasis]),
    (ConfigSyntax::Heading, &[SyntaxKind::Heading]),
    (ConfigSyntax::LinkUrl, &[SyntaxKind::LinkUrl]),
    (ConfigSyntax::LinkText, &[SyntaxKind::Link]),
    (ConfigSyntax::Wikilink, &[SyntaxKind::WikiLink]),
    (ConfigSyntax::InlineCode, &[SyntaxKind::InlineCode]),
    (ConfigSyntax::CodeFence, &[SyntaxKind::CodeBlock]),
    (ConfigSyntax::Math, &[SyntaxKind::Math]),
    (ConfigSyntax::Blockquote, &[SyntaxKind::Quote]),
    (ConfigSyntax::Callout, &[SyntaxKind::Callout]),
    (ConfigSyntax::Footnote, &[SyntaxKind::Footnote]),
    (ConfigSyntax::Comment, &[SyntaxKind::Comment]),
    (ConfigSyntax::Html, &[SyntaxKind::Html]),
    (
        ConfigSyntax::ListMarker,
        &[SyntaxKind::List, SyntaxKind::Task],
    ),
    (ConfigSyntax::Frontmatter, &[SyntaxKind::Frontmatter]),
];

/// The planner's settings for the symbol settings.
pub fn reveal_settings(symbols: &SymbolSettings) -> RevealSettings {
    let scope = reveal_scope(symbols.scope);
    let mut settings = RevealSettings::new(reveal_mode(symbols.mode, scope));
    for (syntax, mode) in &symbols.overrides {
        let kinds = SYNTAX_MAP
            .iter()
            .find(|(candidate, _)| candidate == syntax)
            .map_or(&[][..], |(_, kinds)| *kinds);
        for kind in kinds {
            settings = settings.with_override(*kind, reveal_mode(*mode, scope));
        }
    }
    settings
}

fn reveal_mode(mode: SymbolMode, scope: RevealScope) -> RevealMode {
    match mode {
        SymbolMode::AlwaysShown => RevealMode::AlwaysShown,
        SymbolMode::AroundCursor => RevealMode::AroundCursor { scope },
        SymbolMode::AlwaysHidden => RevealMode::AlwaysHidden,
    }
}

fn reveal_scope(scope: ConfigScope) -> RevealScope {
    match scope {
        ConfigScope::Element => RevealScope::Element,
        ConfigScope::Line => RevealScope::Line,
        ConfigScope::Block => RevealScope::Block,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_and_scope_carry_over() {
        let symbols = SymbolSettings {
            mode: SymbolMode::AroundCursor,
            scope: ConfigScope::Line,
            overrides: Default::default(),
        };
        let settings = reveal_settings(&symbols);
        assert_eq!(
            settings.mode,
            RevealMode::AroundCursor {
                scope: RevealScope::Line
            }
        );
    }

    #[test]
    fn overrides_map_to_planner_kinds() {
        let mut symbols = SymbolSettings::default();
        symbols
            .overrides
            .insert(ConfigSyntax::LinkUrl, SymbolMode::AlwaysHidden);
        symbols
            .overrides
            .insert(ConfigSyntax::ListMarker, SymbolMode::AlwaysShown);
        let settings = reveal_settings(&symbols);
        assert_eq!(
            settings.mode_for(SyntaxKind::LinkUrl),
            RevealMode::AlwaysHidden
        );
        assert_eq!(settings.mode_for(SyntaxKind::Task), RevealMode::AlwaysShown);
        assert_eq!(
            settings.mode_for(SyntaxKind::Emphasis),
            RevealMode::AroundCursor {
                scope: RevealScope::Element
            }
        );
    }
}
