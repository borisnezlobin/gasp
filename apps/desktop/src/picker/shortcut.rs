//! Shortcuts as people read them, and keystrokes turned back into chords
//! for `rules.toml`.

use editor_config::Platform;
use editor_config::keys::{Key, KeyChord, Modifiers, NamedKey};
use gpui::Keystroke;

/// Apple's modifier glyphs, in the order macOS menus print them.
const APPLE_MODIFIERS: [(Modifiers, &str); 4] = [
    (Modifiers::CTRL, "⌃"),
    (Modifiers::ALT, "⌥"),
    (Modifiers::SHIFT, "⇧"),
    (Modifiers::META, "⌘"),
];

const APPLE_KEYS: [(NamedKey, &str); 14] = [
    (NamedKey::Enter, "↩"),
    (NamedKey::Tab, "⇥"),
    (NamedKey::Space, "Space"),
    (NamedKey::Escape, "⎋"),
    (NamedKey::Backspace, "⌫"),
    (NamedKey::Delete, "⌦"),
    (NamedKey::Left, "←"),
    (NamedKey::Right, "→"),
    (NamedKey::Up, "↑"),
    (NamedKey::Down, "↓"),
    (NamedKey::Home, "↖"),
    (NamedKey::End, "↘"),
    (NamedKey::PageUp, "⇞"),
    (NamedKey::PageDown, "⇟"),
];

/// How a shortcut is shown on `platform`: `⇧⌘P` on Apple platforms and
/// `Ctrl+Shift+P` elsewhere.
pub fn shortcut_label(chord: KeyChord, platform: Platform) -> String {
    if !platform.is_apple() {
        return chord.display_for(platform);
    }
    let chord = chord.resolve(platform);
    let mut label: String = APPLE_MODIFIERS
        .iter()
        .filter(|(modifier, _)| chord.modifiers.contains(*modifier))
        .map(|(_, glyph)| *glyph)
        .collect();
    label.push_str(&apple_key(chord.key));
    label
}

fn apple_key(key: Key) -> String {
    let Key::Named(named) = key else {
        return key.to_string();
    };
    APPLE_KEYS
        .iter()
        .find(|(candidate, _)| *candidate == named)
        .map_or_else(|| key.to_string(), |(_, glyph)| (*glyph).to_string())
}

/// GPUI key names and the names `rules.toml` uses for them.
const GPUI_KEYS: [(&str, &str); 14] = [
    ("enter", "Enter"),
    ("tab", "Tab"),
    ("space", "Space"),
    ("escape", "Escape"),
    ("backspace", "Backspace"),
    ("delete", "Delete"),
    ("left", "Left"),
    ("right", "Right"),
    ("up", "Up"),
    ("down", "Down"),
    ("home", "Home"),
    ("end", "End"),
    ("pageup", "PageUp"),
    ("pagedown", "PageDown"),
];

fn key_name(gpui_key: &str) -> Option<String> {
    let mut chars = gpui_key.chars();
    if let (Some(only), None) = (chars.next(), chars.next()) {
        return Some(only.to_uppercase().collect());
    }
    if let Some((_, name)) = GPUI_KEYS.iter().find(|(gpui, _)| *gpui == gpui_key) {
        return Some((*name).to_string());
    }
    let digits = gpui_key.strip_prefix('f')?;
    let number: u8 = digits.parse().ok()?;
    (1..=24).contains(&number).then(|| format!("F{number}"))
}

/// The portable chord text for a pressed keystroke, such as `Mod+Shift+K`,
/// or `None` when it can't be a shortcut: a lone modifier, or a key that
/// types text or edits without Mod, Ctrl, Alt or the OS key.
pub fn chord_text(keystroke: &Keystroke, platform: Platform) -> Option<String> {
    let key = key_name(&keystroke.key)?;
    let pressed = &keystroke.modifiers;
    let (primary, control, meta) = if platform.is_apple() {
        (pressed.platform, pressed.control, false)
    } else {
        (pressed.control, false, pressed.platform)
    };
    let parts = [
        (primary, "Mod"),
        (control, "Ctrl"),
        (pressed.alt, "Alt"),
        (pressed.shift, "Shift"),
        (meta, "Meta"),
    ];
    let mut text: Vec<&str> = parts
        .iter()
        .filter(|(held, _)| *held)
        .map(|(_, name)| *name)
        .collect();
    text.push(&key);
    let text = text.join("+");
    let chord = KeyChord::parse(&text).ok()?;
    let has_command_modifier = primary || control || pressed.alt || meta;
    let is_function_key = matches!(chord.key, Key::Function(_));
    (has_command_modifier || is_function_key).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(text: &str, platform: Platform) -> String {
        shortcut_label(KeyChord::parse(text).unwrap(), platform)
    }

    fn chord(keystroke: &str, platform: Platform) -> Option<String> {
        chord_text(&Keystroke::parse(keystroke).unwrap(), platform)
    }

    #[test]
    fn macos_uses_glyphs_in_menu_order() {
        assert_eq!(label("Mod+Shift+P", Platform::Macos), "⇧⌘P");
        assert_eq!(label("Mod+Alt+Left", Platform::Macos), "⌥⌘←");
        assert_eq!(label("Ctrl+Shift+Alt+Mod+K", Platform::Macos), "⌃⌥⇧⌘K");
        assert_eq!(label("Mod+Enter", Platform::Ios), "⌘↩");
        assert_eq!(label("Alt+0", Platform::Macos), "⌥0");
        assert_eq!(label("F5", Platform::Macos), "F5");
    }

    #[test]
    fn other_platforms_spell_modifiers_out() {
        assert_eq!(label("Mod+Shift+P", Platform::Linux), "Ctrl+Shift+P");
        assert_eq!(label("Mod+Shift+P", Platform::Windows), "Ctrl+Shift+P");
        assert_eq!(label("Mod+Alt+Left", Platform::Linux), "Ctrl+Alt+Left");
        assert_eq!(label("Mod+\\", Platform::Windows), "Ctrl+\\");
    }

    #[test]
    fn keystrokes_become_portable_chords() {
        assert_eq!(
            chord("cmd-shift-k", Platform::Macos).as_deref(),
            Some("Mod+Shift+K")
        );
        assert_eq!(
            chord("ctrl-shift-k", Platform::Linux).as_deref(),
            Some("Mod+Shift+K")
        );
        assert_eq!(
            chord("ctrl-cmd-k", Platform::Macos).as_deref(),
            Some("Mod+Ctrl+K")
        );
        assert_eq!(
            chord("alt-enter", Platform::Windows).as_deref(),
            Some("Alt+Enter")
        );
        assert_eq!(chord("f5", Platform::Linux).as_deref(), Some("F5"));
        assert_eq!(chord("ctrl-/", Platform::Linux).as_deref(), Some("Mod+/"));
    }

    #[test]
    fn typing_keys_and_lone_modifiers_are_not_shortcuts() {
        assert_eq!(chord("k", Platform::Linux), None);
        assert_eq!(chord("shift-k", Platform::Macos), None);
        assert_eq!(chord("enter", Platform::Macos), None);
        assert_eq!(chord("shift", Platform::Linux), None);
    }

    #[test]
    fn captured_chords_show_like_configured_ones() {
        let text = chord("cmd-alt-j", Platform::Macos).unwrap();
        assert_eq!(label(&text, Platform::Macos), "⌥⌘J");
    }
}
