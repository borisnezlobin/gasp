//! Shortcuts as people read them, keystrokes turned back into chords
//! for `rules.toml`, and typed searches such as "cmd f" read as keys.

use std::fmt;

use editor_config::Platform;
use editor_config::keymap::is_reserved;
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

/// A chord as one platform shows it: what menus, tooltips and the
/// palette carry so [`crate::ui::keycap`] can draw each key, and what
/// reads as text (`⇧⌘P`, `Ctrl+Shift+P`) where only text will do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Shortcut {
    /// The chord with `Mod` resolved for `platform`.
    pub chord: KeyChord,
    pub platform: Platform,
}

impl Shortcut {
    pub fn new(chord: KeyChord, platform: Platform) -> Shortcut {
        Shortcut {
            chord: chord.resolve(platform),
            platform,
        }
    }

    /// The shortcut as text, for tooltips' plain text, screen readers and
    /// tests.
    pub fn label(&self) -> String {
        shortcut_label(self.chord, self.platform)
    }
}

impl fmt::Display for Shortcut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label())
    }
}

impl PartialEq<&str> for Shortcut {
    fn eq(&self, other: &&str) -> bool {
        self.label() == *other
    }
}

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
    let (text, chord) = pressed_chord(keystroke, platform)?;
    let modifiers = chord.resolve(platform).modifiers;
    let has_command_modifier = [Modifiers::CTRL, Modifiers::ALT, Modifiers::META]
        .iter()
        .any(|modifier| modifiers.contains(*modifier));
    let is_function_key = matches!(chord.key, Key::Function(_));
    (has_command_modifier || is_function_key).then_some(text)
}

/// Any pressed key with its modifiers, as portable text and as a chord,
/// such as for searching by keys, where Tab or Shift+Enter are fine.
pub fn pressed_chord(keystroke: &Keystroke, platform: Platform) -> Option<(String, KeyChord)> {
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
    Some((text, chord))
}

/// Whether a keystroke is only a modifier going down, which a shortcut
/// capture waits past.
pub fn is_lone_modifier(keystroke: &Keystroke) -> bool {
    matches!(
        keystroke.key.as_str(),
        "shift" | "control" | "alt" | "platform" | "function" | "fn" | "capslock"
    )
}

/// The portable chord for a keystroke pressed while capturing a new
/// shortcut, or why it can't be one, in words to show the user.
pub fn capture_chord(keystroke: &Keystroke, platform: Platform) -> Result<String, &'static str> {
    let text = chord_text(keystroke, platform).ok_or(if platform.is_apple() {
        "Hold Command, Control or Option with the key, or use a function key."
    } else {
        "Hold Ctrl or Alt with the key, or use a function key."
    })?;
    let chord =
        KeyChord::parse_for(&text, platform).map_err(|_| "That key can't be a shortcut.")?;
    if is_reserved(chord, platform) {
        return Err("The system uses that shortcut. Try another.");
    }
    Ok(text)
}

/// Words people type for modifiers. `Mod` stands for the platform's
/// main one, so "cmd f" finds Ctrl+F off Apple platforms, where there is
/// no Command key.
const MODIFIER_WORDS: [(&str, Modifiers); 17] = [
    ("mod", Modifiers::MOD),
    ("cmd", Modifiers::MOD),
    ("command", Modifiers::MOD),
    ("⌘", Modifiers::MOD),
    ("ctrl", Modifiers::CTRL),
    ("control", Modifiers::CTRL),
    ("ctl", Modifiers::CTRL),
    ("⌃", Modifiers::CTRL),
    ("alt", Modifiers::ALT),
    ("opt", Modifiers::ALT),
    ("option", Modifiers::ALT),
    ("⌥", Modifiers::ALT),
    ("shift", Modifiers::SHIFT),
    ("⇧", Modifiers::SHIFT),
    ("super", Modifiers::META),
    ("win", Modifiers::META),
    ("meta", Modifiers::META),
];

/// Key names people type that `rules.toml` spells another way.
const KEY_WORDS: [(&str, NamedKey); 4] = [
    ("del", NamedKey::Delete),
    ("pgup", NamedKey::PageUp),
    ("pgdn", NamedKey::PageDown),
    ("pagedn", NamedKey::PageDown),
];

/// A search read as keys: "cmd f", "ctrl shift p", "Ctrl+Shift+P" or
/// `⇧⌘P` as the settings screen writes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyQuery {
    /// Resolved for the platform, so without `Mod`.
    pub modifiers: Modifiers,
    /// `None` when only modifiers were typed, which finds every chord
    /// holding them.
    pub key: Option<Key>,
    /// Whether the text can only mean keys: it names a modifier and
    /// something more, so it shouldn't also match titles.
    pub only_keys: bool,
}

impl KeyQuery {
    /// Reads `query` as keys on `platform`, or `None` when it's words:
    /// it names something that isn't a key, two keys, or a lone letter.
    pub fn parse(query: &str, platform: Platform) -> Option<KeyQuery> {
        let tokens: Vec<&str> = query
            .split(|c: char| c.is_whitespace() || c == '+')
            .filter(|token| !token.is_empty())
            .collect();
        let mut modifiers = Modifiers::NONE;
        let mut key = None;
        let mut parts = 0;
        for token in &tokens {
            let (glyphs, rest) = split_modifier_glyphs(token);
            modifiers = modifiers.union(glyphs);
            parts += usize::from(glyphs != Modifiers::NONE);
            if rest.is_empty() {
                continue;
            }
            parts += 1;
            match modifier_word(rest) {
                Some(named) => modifiers = modifiers.union(named),
                None if key.is_none() => key = Some(key_word(rest)?),
                None => return None,
            }
        }
        let modifiers = KeyChord::new(modifiers, Key::Char('A'))
            .resolve(platform)
            .modifiers;
        let named_modifier = modifiers != Modifiers::NONE;
        let plain_letter = !named_modifier && matches!(key, Some(Key::Char(_)));
        if plain_letter || (!named_modifier && key.is_none()) {
            return None;
        }
        Some(KeyQuery {
            modifiers,
            key,
            only_keys: named_modifier && parts > 1,
        })
    }

    /// Whether `chord` (resolved) is what was typed: the same key and
    /// modifiers, or, when no key was typed, any chord holding them.
    pub fn matches(&self, chord: KeyChord) -> bool {
        match self.key {
            Some(key) => chord.key == key && chord.modifiers == self.modifiers,
            None => chord.modifiers.contains(self.modifiers),
        }
    }
}

/// Peels Apple's modifier glyphs off the front of `token`, so `⇧⌘P`
/// reads as Shift, Command and P.
fn split_modifier_glyphs(token: &str) -> (Modifiers, &str) {
    let mut modifiers = Modifiers::NONE;
    let mut rest = token;
    while let Some(c) = rest.chars().next() {
        let glyph = c.to_string();
        let Some(modifier) = modifier_word(&glyph).filter(|_| !c.is_ascii()) else {
            break;
        };
        modifiers = modifiers.union(modifier);
        rest = &rest[c.len_utf8()..];
    }
    (modifiers, rest)
}

fn modifier_word(word: &str) -> Option<Modifiers> {
    let word = word.to_lowercase();
    MODIFIER_WORDS
        .iter()
        .find(|(name, _)| *name == word)
        .map(|(_, modifier)| *modifier)
}

/// A key as typed: its name, a glyph from a label, or a common short
/// form such as "del".
fn key_word(word: &str) -> Option<Key> {
    let lower = word.to_lowercase();
    let spelled = KEY_WORDS
        .iter()
        .find(|(name, _)| *name == lower)
        .map(|(_, key)| Key::Named(*key));
    let glyph = APPLE_KEYS
        .iter()
        .find(|(_, glyph)| *glyph == word)
        .map(|(key, _)| Key::Named(*key));
    spelled.or(glyph).or_else(|| Key::parse(word).ok())
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

    fn finds(query: &str, keys: &str, platform: Platform) -> bool {
        let chord = KeyChord::parse_for(keys, platform).unwrap();
        KeyQuery::parse(query, platform).is_some_and(|found| found.matches(chord))
    }

    #[test]
    fn typed_keys_find_chords() {
        for platform in [Platform::Linux, Platform::Macos] {
            assert!(finds("cmd f", "Mod+F", platform), "{platform:?}");
            assert!(finds("Mod+Shift+P", "Mod+Shift+P", platform));
            assert!(!finds("cmd f", "Mod+Shift+F", platform));
            assert!(finds("shift", "Mod+Shift+F", platform));
            assert!(finds("f5", "F5", platform));
            assert!(finds("alt pgdn", "Alt+PageDown", platform));
        }
        assert!(finds("ctrl shift p", "Mod+Shift+P", Platform::Linux));
        assert!(finds("Ctrl+,", "Mod+,", Platform::Windows));
        // On a Mac, Control is its own key.
        assert!(!finds("ctrl shift p", "Mod+Shift+P", Platform::Macos));
        assert!(finds("⇧⌘P", "Mod+Shift+P", Platform::Macos));
        assert!(finds("⌥⌘←", "Mod+Alt+Left", Platform::Macos));
    }

    #[test]
    fn words_are_not_keys() {
        let parse = |query| KeyQuery::parse(query, Platform::Linux);
        assert_eq!(parse("new tab"), None);
        assert_eq!(parse("p"), None);
        assert_eq!(parse(""), None);
        assert_eq!(parse("ctrl a b"), None);
        // A lone modifier or named key also matches titles; with more it's
        // only keys.
        assert!(!parse("shift").unwrap().only_keys);
        assert!(!parse("tab").unwrap().only_keys);
        assert!(parse("ctrl tab").unwrap().only_keys);
    }
}
