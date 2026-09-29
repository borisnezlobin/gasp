//! A shortcut drawn as one flat chip holding each of its keys, the same
//! wherever a shortcut shows: the settings screen, the command palette,
//! menus, tooltips and the help dialog.
//!
//! Keys that carry a symbol on every keyboard (Shift, Return, Backspace,
//! the arrows, and Apple's modifiers) are drawn as Phosphor icons; keys
//! whose name is printed on them (Ctrl, Alt, Tab, Esc) and letters are set
//! in the platform's sans. Nothing is joined with "+": the gap between
//! glyphs does that.

use gasp_config::keys::{Key, Modifiers, NamedKey};
use gpui::{Div, SharedString, div, prelude::*};

use crate::icons::{IconName, icon};
use crate::picker::shortcut::Shortcut;
use crate::theme::KeycapTheme;

/// One key of a chord as it's drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Glyph {
    Icon(IconName),
    Text(SharedString),
}

/// Apple's modifiers in the order macOS menus print them.
const APPLE_MODIFIERS: [(Modifiers, IconName); 4] = [
    (Modifiers::CTRL, IconName::Control),
    (Modifiers::ALT, IconName::Option),
    (Modifiers::SHIFT, IconName::ArrowFatUp),
    (Modifiers::META, IconName::Command),
];

/// Other platforms' modifiers in the order they write them.
const PC_MODIFIERS: [(Modifiers, Glyph); 4] = [
    (
        Modifiers::CTRL,
        Glyph::Text(SharedString::new_static("Ctrl")),
    ),
    (Modifiers::ALT, Glyph::Text(SharedString::new_static("Alt"))),
    (Modifiers::SHIFT, Glyph::Icon(IconName::ArrowFatUp)),
    (
        Modifiers::META,
        Glyph::Text(SharedString::new_static("Super")),
    ),
];

/// Named keys drawn as icons on every platform.
const KEY_ICONS: [(NamedKey, IconName); 6] = [
    (NamedKey::Enter, IconName::KeyReturn),
    (NamedKey::Backspace, IconName::Backspace),
    (NamedKey::Left, IconName::ArrowLeft),
    (NamedKey::Right, IconName::ArrowRight),
    (NamedKey::Up, IconName::ArrowUp),
    (NamedKey::Down, IconName::ArrowDown),
];

/// Named keys drawn as the short word printed on them.
const KEY_WORDS: [(NamedKey, &str); 8] = [
    (NamedKey::Tab, "Tab"),
    (NamedKey::Space, "Space"),
    (NamedKey::Escape, "Esc"),
    (NamedKey::Delete, "Del"),
    (NamedKey::Home, "Home"),
    (NamedKey::End, "End"),
    (NamedKey::PageUp, "PgUp"),
    (NamedKey::PageDown, "PgDn"),
];

/// The glyphs a shortcut is drawn with, modifiers first.
pub fn glyphs(shortcut: Shortcut) -> Vec<Glyph> {
    let held = shortcut.chord.modifiers;
    let mut glyphs: Vec<Glyph> = if shortcut.platform.is_apple() {
        APPLE_MODIFIERS
            .iter()
            .filter(|(modifier, _)| held.contains(*modifier))
            .map(|(_, name)| Glyph::Icon(*name))
            .collect()
    } else {
        PC_MODIFIERS
            .iter()
            .filter(|(modifier, _)| held.contains(*modifier))
            .map(|(_, glyph)| glyph.clone())
            .collect()
    };
    glyphs.push(key_glyph(shortcut.chord.key));
    glyphs
}

fn key_glyph(key: Key) -> Glyph {
    let Key::Named(named) = key else {
        return Glyph::Text(key.to_string().into());
    };
    if let Some((_, name)) = KEY_ICONS.iter().find(|(known, _)| *known == named) {
        return Glyph::Icon(*name);
    }
    KEY_WORDS
        .iter()
        .find(|(known, _)| *known == named)
        .map_or_else(
            || Glyph::Text(key.to_string().into()),
            |(_, word)| Glyph::Text(SharedString::new_static(word)),
        )
}

/// A shortcut as one chip. Callers add hover, a ring or a trailing
/// button to the returned element.
pub fn keycap(shortcut: Shortcut, theme: &KeycapTheme) -> Div {
    keycap_glyphs(glyphs(shortcut), theme)
}

/// A chip of any glyphs, such as a family of shortcuts that share their
/// modifiers and differ by arrow.
pub fn keycap_glyphs(glyphs: Vec<Glyph>, theme: &KeycapTheme) -> Div {
    let glyphs = glyphs.into_iter().map(|glyph| match glyph {
        Glyph::Icon(name) => icon(name)
            .flex_none()
            .size(theme.icon_size)
            .text_color(theme.glyph)
            .into_any_element(),
        Glyph::Text(text) => div().child(text).into_any_element(),
    });
    div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .gap(theme.gap)
        .h(theme.height)
        .min_w(theme.height)
        .px(theme.padding_x)
        .rounded(theme.radius)
        .bg(theme.fill)
        .font_family(theme.font_family.clone())
        .text_size(theme.font_size)
        .font_weight(theme.font_weight)
        .line_height(theme.height)
        .text_color(theme.glyph)
        .whitespace_nowrap()
        .children(glyphs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gasp_config::Platform;
    use gasp_config::keys::KeyChord;

    fn drawn(text: &str, platform: Platform) -> Vec<Glyph> {
        glyphs(Shortcut::new(KeyChord::parse(text).unwrap(), platform))
    }

    fn word(text: &'static str) -> Glyph {
        Glyph::Text(SharedString::new_static(text))
    }

    #[test]
    fn apple_modifiers_are_icons_in_menu_order() {
        assert_eq!(
            drawn("Mod+Shift+P", Platform::Macos),
            [
                Glyph::Icon(IconName::ArrowFatUp),
                Glyph::Icon(IconName::Command),
                word("P")
            ]
        );
        assert_eq!(
            drawn("Ctrl+Alt+Enter", Platform::Macos),
            [
                Glyph::Icon(IconName::Control),
                Glyph::Icon(IconName::Option),
                Glyph::Icon(IconName::KeyReturn)
            ]
        );
    }

    #[test]
    fn other_platforms_print_modifier_names() {
        assert_eq!(
            drawn("Mod+Shift+P", Platform::Linux),
            [word("Ctrl"), Glyph::Icon(IconName::ArrowFatUp), word("P")]
        );
        assert_eq!(
            drawn("Alt+PageDown", Platform::Windows),
            [word("Alt"), word("PgDn")]
        );
        assert_eq!(drawn("F5", Platform::Linux), [word("F5")]);
        assert_eq!(
            drawn("Mod+Left", Platform::Linux),
            [word("Ctrl"), Glyph::Icon(IconName::ArrowLeft)]
        );
    }
}
