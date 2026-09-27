//! Key chords such as `Mod+Shift+P`, with `Mod` resolved per platform.

use std::fmt;

use crate::platform::Platform;

/// A set of modifier keys. `MOD` is the portable modifier and disappears on resolve.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const NONE: Modifiers = Modifiers(0);
    pub const MOD: Modifiers = Modifiers(1);
    pub const CTRL: Modifiers = Modifiers(2);
    pub const ALT: Modifiers = Modifiers(4);
    pub const SHIFT: Modifiers = Modifiers(8);
    /// Cmd on Apple platforms, the Windows key on Windows, Super on Linux.
    pub const META: Modifiers = Modifiers(16);

    pub fn contains(self, other: Modifiers) -> bool {
        self.0 & other.0 == other.0 && other.0 != 0
    }

    pub fn union(self, other: Modifiers) -> Modifiers {
        Modifiers(self.0 | other.0)
    }

    fn without(self, other: Modifiers) -> Modifiers {
        Modifiers(self.0 & !other.0)
    }
}

const MODIFIER_NAMES: &[(&str, Modifiers)] = &[
    ("mod", Modifiers::MOD),
    ("ctrl", Modifiers::CTRL),
    ("control", Modifiers::CTRL),
    ("alt", Modifiers::ALT),
    ("option", Modifiers::ALT),
    ("shift", Modifiers::SHIFT),
    ("cmd", Modifiers::META),
    ("command", Modifiers::META),
    ("win", Modifiers::META),
    ("super", Modifiers::META),
    ("meta", Modifiers::META),
];

/// A key that isn't a single printable character.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NamedKey {
    Enter,
    Tab,
    Space,
    Escape,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
}

const NAMED_KEYS: &[(&str, NamedKey)] = &[
    ("Enter", NamedKey::Enter),
    ("Return", NamedKey::Enter),
    ("Tab", NamedKey::Tab),
    ("Space", NamedKey::Space),
    ("Escape", NamedKey::Escape),
    ("Esc", NamedKey::Escape),
    ("Backspace", NamedKey::Backspace),
    ("Delete", NamedKey::Delete),
    ("Left", NamedKey::Left),
    ("Right", NamedKey::Right),
    ("Up", NamedKey::Up),
    ("Down", NamedKey::Down),
    ("Home", NamedKey::Home),
    ("End", NamedKey::End),
    ("PageUp", NamedKey::PageUp),
    ("PageDown", NamedKey::PageDown),
];

/// The non-modifier key of a chord.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    /// A printable key. Letters are stored uppercase.
    Char(char),
    Named(NamedKey),
    /// F1 to F24.
    Function(u8),
}

impl Key {
    /// Parses a key name such as `P`, `/`, `Enter` or `F5` (case-insensitive for names).
    pub fn parse(name: &str) -> Result<Key, ChordError> {
        let mut chars = name.chars();
        if let (Some(only), None) = (chars.next(), chars.next()) {
            return Ok(Key::Char(only.to_ascii_uppercase()));
        }
        named_key(name)
            .map(Key::Named)
            .or_else(|| function_key(name))
            .ok_or_else(|| ChordError::UnknownKey(name.to_string()))
    }
}

fn named_key(name: &str) -> Option<NamedKey> {
    NAMED_KEYS
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(name))
        .map(|(_, key)| *key)
}

fn function_key(name: &str) -> Option<Key> {
    let digits = name.strip_prefix(['F', 'f'])?;
    let number: u8 = digits.parse().ok()?;
    (1..=24).contains(&number).then_some(Key::Function(number))
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Char(c) => write!(f, "{c}"),
            Key::Function(n) => write!(f, "F{n}"),
            Key::Named(named) => {
                let name = NAMED_KEYS
                    .iter()
                    .find(|(_, key)| key == named)
                    .map_or("?", |(name, _)| name);
                f.write_str(name)
            }
        }
    }
}

/// Why a chord string couldn't be parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChordError {
    Empty,
    UnknownModifier(String),
    DuplicateModifier(String),
    UnknownKey(String),
}

impl fmt::Display for ChordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChordError::Empty => f.write_str("the key chord is empty"),
            ChordError::UnknownModifier(m) => write!(f, "unknown modifier `{m}`"),
            ChordError::DuplicateModifier(m) => write!(f, "modifier `{m}` appears twice"),
            ChordError::UnknownKey(k) => write!(f, "unknown key `{k}`"),
        }
    }
}

impl std::error::Error for ChordError {}

/// A key plus modifiers. It may still contain `Mod` until [`KeyChord::resolve`] is called.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyChord {
    pub modifiers: Modifiers,
    pub key: Key,
}

impl KeyChord {
    pub fn new(modifiers: Modifiers, key: Key) -> KeyChord {
        KeyChord { modifiers, key }
    }

    /// Parses `Mod+Shift+P`. `Mod++` means the `+` key.
    pub fn parse(text: &str) -> Result<KeyChord, ChordError> {
        let text = text.trim();
        let (modifier_part, key_name) = split_key(text)?;
        let mut modifiers = Modifiers::NONE;
        for name in modifier_part.split('+').filter(|part| !part.is_empty()) {
            modifiers = add_modifier(modifiers, name)?;
        }
        Ok(KeyChord::new(modifiers, Key::parse(key_name)?))
    }

    /// Parses and resolves `Mod` for `platform` in one step.
    pub fn parse_for(text: &str, platform: Platform) -> Result<KeyChord, ChordError> {
        KeyChord::parse(text).map(|chord| chord.resolve(platform))
    }

    /// Replaces `Mod` with Cmd on Apple platforms and Ctrl elsewhere.
    pub fn resolve(self, platform: Platform) -> KeyChord {
        if !self.modifiers.contains(Modifiers::MOD) {
            return self;
        }
        let concrete = if platform.is_apple() {
            Modifiers::META
        } else {
            Modifiers::CTRL
        };
        let modifiers = self.modifiers.without(Modifiers::MOD).union(concrete);
        KeyChord::new(modifiers, self.key)
    }

    /// Renders the chord the way a user on `platform` would write it.
    pub fn display_for(self, platform: Platform) -> String {
        let chord = self.resolve(platform);
        let meta = meta_name(platform);
        let labels = [
            (Modifiers::CTRL, "Ctrl"),
            (Modifiers::ALT, "Alt"),
            (Modifiers::SHIFT, "Shift"),
            (Modifiers::META, meta),
        ];
        let mut parts: Vec<String> = labels
            .iter()
            .filter(|(modifier, _)| chord.modifiers.contains(*modifier))
            .map(|(_, label)| label.to_string())
            .collect();
        parts.push(chord.key.to_string());
        parts.join("+")
    }
}

fn meta_name(platform: Platform) -> &'static str {
    const META_NAMES: [(Platform, &str); 4] = [
        (Platform::Macos, "Cmd"),
        (Platform::Ios, "Cmd"),
        (Platform::Windows, "Win"),
        (Platform::Linux, "Super"),
    ];
    META_NAMES
        .iter()
        .find(|(p, _)| *p == platform)
        .map_or("Meta", |(_, name)| name)
}

fn split_key(text: &str) -> Result<(&str, &str), ChordError> {
    if text.is_empty() {
        return Err(ChordError::Empty);
    }
    if text == "+" {
        return Ok(("", "+"));
    }
    if let Some(prefix) = text.strip_suffix("++") {
        return Ok((prefix, "+"));
    }
    let (modifiers, key) = text.rsplit_once('+').unwrap_or(("", text));
    if key.is_empty() {
        return Err(ChordError::Empty);
    }
    Ok((modifiers, key))
}

fn add_modifier(current: Modifiers, name: &str) -> Result<Modifiers, ChordError> {
    let lower = name.trim().to_ascii_lowercase();
    let modifier = MODIFIER_NAMES
        .iter()
        .find(|(known, _)| *known == lower)
        .map(|(_, modifier)| *modifier)
        .ok_or_else(|| ChordError::UnknownModifier(name.to_string()))?;
    if current.contains(modifier) {
        return Err(ChordError::DuplicateModifier(name.to_string()));
    }
    Ok(current.union(modifier))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mod_shift_p() {
        let chord = KeyChord::parse("Mod+Shift+P").unwrap();
        assert_eq!(chord.modifiers, Modifiers::MOD.union(Modifiers::SHIFT));
        assert_eq!(chord.key, Key::Char('P'));
    }

    #[test]
    fn mod_resolves_per_platform() {
        let chord = KeyChord::parse("Mod+B").unwrap();
        assert_eq!(chord.display_for(Platform::Macos), "Cmd+B");
        assert_eq!(chord.display_for(Platform::Ios), "Cmd+B");
        assert_eq!(chord.display_for(Platform::Windows), "Ctrl+B");
        assert_eq!(chord.display_for(Platform::Linux), "Ctrl+B");
    }

    #[test]
    fn letters_are_case_insensitive() {
        assert_eq!(KeyChord::parse("mod+b"), KeyChord::parse("Mod+B"));
    }

    #[test]
    fn parses_punctuation_and_named_keys() {
        for text in ["Mod+/", "Mod+;", "Mod+[", "Mod+\\", "Mod+,", "Cmd+`"] {
            assert!(KeyChord::parse(text).is_ok(), "{text}");
        }
        let chord = KeyChord::parse("Mod+Alt+Left").unwrap();
        assert_eq!(chord.key, Key::Named(NamedKey::Left));
        assert_eq!(KeyChord::parse("Alt+F4").unwrap().key, Key::Function(4));
        assert_eq!(
            KeyChord::parse("Ctrl+Enter").unwrap().key,
            Key::Named(NamedKey::Enter)
        );
    }

    #[test]
    fn plus_key_is_written_twice() {
        let chord = KeyChord::parse("Mod++").unwrap();
        assert_eq!(chord.key, Key::Char('+'));
        assert_eq!(chord.modifiers, Modifiers::MOD);
    }

    #[test]
    fn win_and_super_are_the_same_modifier() {
        assert_eq!(KeyChord::parse("Win+E"), KeyChord::parse("Super+E"));
    }

    #[test]
    fn mod_equals_ctrl_off_apple_and_cmd_on_apple() {
        let portable = KeyChord::parse("Mod+S").unwrap();
        assert_eq!(
            portable.resolve(Platform::Linux),
            KeyChord::parse("Ctrl+S").unwrap()
        );
        assert_eq!(
            portable.resolve(Platform::Macos),
            KeyChord::parse("Cmd+S").unwrap()
        );
    }

    #[test]
    fn rejects_bad_chords() {
        assert_eq!(KeyChord::parse(""), Err(ChordError::Empty));
        assert!(matches!(
            KeyChord::parse("Hyper+A"),
            Err(ChordError::UnknownModifier(_))
        ));
        assert!(matches!(
            KeyChord::parse("Mod+Mod+A"),
            Err(ChordError::DuplicateModifier(_))
        ));
        assert!(matches!(
            KeyChord::parse("Mod+Banana"),
            Err(ChordError::UnknownKey(_))
        ));
        assert_eq!(KeyChord::parse("Mod+"), Err(ChordError::Empty));
    }

    #[test]
    fn display_lists_modifiers_in_canonical_order() {
        let chord = KeyChord::parse("Shift+Alt+Mod+X").unwrap();
        assert_eq!(chord.display_for(Platform::Linux), "Ctrl+Alt+Shift+X");
    }
}
