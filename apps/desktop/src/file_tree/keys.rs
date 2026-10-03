//! Keyboard handling for the tree. Arrows move, Enter opens, typing jumps
//! to a name, and the context menu and trash prompt take keys while open.

use std::time::{Duration, Instant};

use gpui::{Context, KeyDownEvent, Keystroke, Window};

use super::FileTreeEvent;
use super::entries::EntryKind;
use super::view::FileTree;
use crate::keymap::RunCommand;

/// Letters typed further apart than this start a new search.
const TYPE_AHEAD_RESET: Duration = Duration::from_millis(900);
const PAGE_ROWS: isize = 10;

type KeyHandler = fn(&mut FileTree, &mut Window, &mut Context<FileTree>);

/// Keys pressed without modifiers.
const PLAIN_KEYS: [(&str, KeyHandler); 13] = [
    ("up", |tree, _, cx| tree.move_selection(-1, cx)),
    ("down", |tree, _, cx| tree.move_selection(1, cx)),
    ("left", |tree, _, cx| tree.collapse_or_parent(cx)),
    ("right", |tree, _, cx| tree.expand_or_child(cx)),
    ("home", |tree, _, cx| {
        tree.move_selection(isize::MIN / 2, cx)
    }),
    ("end", |tree, _, cx| tree.move_selection(isize::MAX / 2, cx)),
    ("pageup", |tree, _, cx| tree.move_selection(-PAGE_ROWS, cx)),
    ("pagedown", |tree, _, cx| tree.move_selection(PAGE_ROWS, cx)),
    ("enter", |tree, _, cx| tree.activate(false, cx)),
    ("f2", |tree, window, cx| tree.start_rename(window, cx)),
    ("delete", |tree, _, cx| tree.request_trash(cx)),
    ("backspace", |tree, _, cx| tree.request_trash(cx)),
    ("escape", |tree, _, cx| tree.escape(cx)),
];

/// Keys pressed with Mod (Cmd on macOS, Ctrl elsewhere): (key, with Alt, handler).
const MOD_KEYS: [(&str, bool, KeyHandler); 7] = [
    ("enter", false, |tree, _, cx| tree.activate(true, cx)),
    ("backspace", false, |tree, _, cx| tree.request_trash(cx)),
    ("n", false, |tree, window, cx| {
        tree.start_create(EntryKind::Note, window, cx)
    }),
    ("n", true, |tree, window, cx| {
        tree.start_create(EntryKind::Folder, window, cx)
    }),
    ("x", false, |tree, _, cx| tree.cut_selected(cx)),
    ("v", false, |tree, _, cx| tree.paste(cx)),
    ("c", true, |tree, _, cx| tree.copy_selected_path(cx)),
];

/// What the tree's own keys do, for the sheet holding Mod shows. The
/// arrows, Home, End and typing a name need no telling.
pub const KEY_HINTS: [(&str, &str); 10] = [
    ("Enter", "Open"),
    ("Mod+Enter", "Open in a new tab"),
    ("Mod+N", "New note here"),
    ("Mod+Alt+N", "New folder here"),
    ("F2", "Rename"),
    ("Mod+Backspace", "Move to the trash"),
    ("Mod+X", "Cut"),
    ("Mod+V", "Paste"),
    ("Mod+Alt+C", "Copy the path"),
    ("Escape", "Back to the editor"),
];

/// The hints when a plain open already goes to a new tab, so Mod+Enter
/// opens in the current one.
const KEY_HINTS_OPENING_IN_NEW_TAB: [(&str, &str); 10] = {
    let mut hints = KEY_HINTS;
    hints[1] = ("Mod+Enter", "Open in the current tab");
    hints
};

/// The tree's key hints, for whether a plain open goes to a new tab.
pub fn key_hints(opens_in_new_tab: bool) -> &'static [(&'static str, &'static str)] {
    if opens_in_new_tab {
        &KEY_HINTS_OPENING_IN_NEW_TAB
    } else {
        &KEY_HINTS
    }
}

/// Commands from the keymap the tree runs while it has focus.
const COMMANDS: [(&str, KeyHandler); 4] = [
    ("link.follow", |tree, _, cx| tree.activate(true, cx)),
    ("note.new", |tree, window, cx| {
        tree.start_create(EntryKind::Note, window, cx)
    }),
    ("note.rename", |tree, window, cx| {
        tree.start_rename(window, cx)
    }),
    ("note.delete", |tree, _, cx| tree.request_trash(cx)),
];

impl FileTree {
    pub(super) fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Keys meant for the inline name field aren't the tree's.
        if !self.focus_handle.is_focused(window) {
            return;
        }
        let keystroke = &event.keystroke;
        let handled = if self.menu.is_some() {
            self.menu_key(keystroke, window, cx)
        } else if !self.pending_trash.is_empty() {
            self.trash_prompt_key(keystroke, cx)
        } else {
            self.tree_key(keystroke, window, cx)
        };
        if handled {
            cx.stop_propagation();
        }
    }

    /// Runs keymap commands that mean something in the tree, such as
    /// Mod+Enter (`link.follow`) opening in a new tab.
    pub(super) fn on_run_command(
        &mut self,
        action: &RunCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let handler = COMMANDS.iter().find(|(id, _)| *id == action.id.as_ref());
        match handler {
            Some((_, handler)) if self.edit.is_none() => handler(self, window, cx),
            _ => cx.propagate(),
        }
    }

    fn tree_key(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let modifiers = keystroke.modifiers;
        let key = keystroke.key.as_str();
        if modifiers.secondary() {
            let alt = modifiers.alt;
            let found = MOD_KEYS.iter().find(|(k, a, _)| *k == key && *a == alt);
            return found
                .map(|(_, _, handler)| handler(self, window, cx))
                .is_some();
        }
        if (modifiers.shift && key == "f10") || key == "menu" {
            let target = self.selected.clone();
            self.open_menu(target, None, cx);
            return true;
        }
        let plain = PLAIN_KEYS.iter().find(|(k, _)| *k == key);
        if let (false, Some((_, handler))) = (modifiers.modified(), plain) {
            handler(self, window, cx);
            return true;
        }
        self.typed_key(keystroke, cx)
    }

    fn typed_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        let modifiers = keystroke.modifiers;
        if modifiers.control || modifiers.platform || modifiers.alt {
            return false;
        }
        let Some(text) = keystroke.key_char.as_deref() else {
            return false;
        };
        if text.chars().any(char::is_control)
            || (text.trim().is_empty() && !self.type_ahead.active())
        {
            return false;
        }
        self.type_to_jump(text, cx);
        true
    }

    fn escape(&mut self, cx: &mut Context<Self>) {
        if !self.clear_marks(cx) {
            cx.emit(FileTreeEvent::Dismissed);
        }
    }

    fn menu_key(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = self.menu.as_mut() else {
            return false;
        };
        match keystroke.key.as_str() {
            "up" => menu.move_highlight(-1),
            "down" => menu.move_highlight(1),
            "enter" | "space" => {
                if let Some(item) = menu.highlighted_item() {
                    self.run_menu_item(item, window, cx);
                }
            }
            "escape" => self.close_menu(cx),
            _ => {}
        }
        cx.notify();
        true
    }

    fn trash_prompt_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        match keystroke.key.as_str() {
            "enter" => self.confirm_trash(cx),
            "escape" => self.cancel_trash(cx),
            _ => {}
        }
        true
    }
}

/// What's been typed for type-to-jump.
#[derive(Debug, Default)]
pub(super) struct TypeAhead {
    typed: String,
    last: Option<Instant>,
}

impl TypeAhead {
    /// Adds typed text and returns the prefix to look for. Pressing the
    /// same letter again cycles through names that start with it.
    pub fn push(&mut self, text: &str, now: Instant) -> String {
        let fresh = self
            .last
            .is_none_or(|last| now.duration_since(last) > TYPE_AHEAD_RESET);
        if fresh {
            self.typed.clear();
        }
        self.last = Some(now);
        let mut letters = text.chars();
        let single = letters.next().filter(|_| letters.next().is_none());
        let repeating = single.is_some_and(|ch| {
            !self.typed.is_empty() && self.typed.chars().all(|typed| typed == ch)
        });
        if !repeating {
            self.typed.push_str(text);
        }
        self.typed.clone()
    }

    /// Whether a search is under way, so a space can be part of it.
    pub fn active(&self) -> bool {
        self.last
            .is_some_and(|last| last.elapsed() <= TYPE_AHEAD_RESET && !self.typed.is_empty())
    }
}

/// The row to jump to for `prefix`, searching down from `from` and wrapping.
/// A one-letter search starts after the current row so repeating it cycles.
pub(super) fn next_match(labels: &[&str], from: Option<usize>, prefix: &str) -> Option<usize> {
    let count = labels.len();
    let prefix = prefix.to_lowercase();
    let start = match from {
        Some(index) if prefix.chars().count() == 1 => index + 1,
        Some(index) => index,
        None => 0,
    };
    (0..count)
        .map(|step| (start + step) % count)
        .find(|index| labels[*index].to_lowercase().starts_with(&prefix))
}

#[cfg(test)]
mod tests {
    use gasp_config::keys::{Key, KeyChord, Modifiers};

    /// Every hint names a key the tree really handles.
    #[test]
    fn hints_are_keys_the_tree_handles() {
        for (text, label) in KEY_HINTS.into_iter().chain(KEY_HINTS_OPENING_IN_NEW_TAB) {
            let chord = KeyChord::parse(text).unwrap();
            let key = match chord.key {
                Key::Char(ch) => ch.to_ascii_lowercase().to_string(),
                other => other.to_string().to_lowercase(),
            };
            let handled = if chord.modifiers.contains(Modifiers::MOD) {
                let alt = chord.modifiers.contains(Modifiers::ALT);
                MOD_KEYS.iter().any(|(k, a, _)| *k == key && *a == alt)
            } else {
                PLAIN_KEYS.iter().any(|(k, _)| *k == key)
            };
            assert!(handled, "{label}: {text} isn't handled");
        }
    }

    use super::*;

    #[test]
    fn mod_enter_is_hinted_as_the_opposite_of_a_plain_open() {
        let hint = |opens_in_new_tab| {
            key_hints(opens_in_new_tab)
                .iter()
                .find(|(keys, _)| *keys == "Mod+Enter")
                .map(|(_, label)| *label)
        };
        assert_eq!(hint(false), Some("Open in a new tab"));
        assert_eq!(hint(true), Some("Open in the current tab"));
    }

    const LABELS: [&str; 5] = ["Daily", "Projects", "Notes", "Note 2", "plan"];

    #[test]
    fn prefixes_find_the_next_match_and_wrap() {
        assert_eq!(next_match(&LABELS, None, "p"), Some(1));
        assert_eq!(next_match(&LABELS, Some(1), "p"), Some(4));
        assert_eq!(next_match(&LABELS, Some(4), "p"), Some(1));
        assert_eq!(next_match(&LABELS, Some(2), "note "), Some(3));
        assert_eq!(next_match(&LABELS, Some(2), "notes"), Some(2));
        assert_eq!(next_match(&LABELS, None, "zzz"), None);
    }

    #[test]
    fn typing_quickly_builds_a_prefix() {
        let mut typed = TypeAhead::default();
        let start = Instant::now();
        assert_eq!(typed.push("n", start), "n");
        assert_eq!(typed.push("o", start + Duration::from_millis(100)), "no");
        let later = start + Duration::from_secs(5);
        assert_eq!(typed.push("d", later), "d");
    }

    #[test]
    fn repeating_a_letter_keeps_it_single() {
        let mut typed = TypeAhead::default();
        let start = Instant::now();
        typed.push("p", start);
        assert_eq!(typed.push("p", start), "p");
        assert_eq!(typed.push("l", start), "pl");
    }
}
