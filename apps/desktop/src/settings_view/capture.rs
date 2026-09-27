//! Changing keyboard shortcuts: "+" waits for the next chord, like the
//! command palette does, and writes it to `.editor/rules.toml`; a key's
//! cross removes it, whether the user added it or it's built in; reset
//! puts a command back to its built-in keys. "Search by keys" waits for a
//! chord the same way and searches for it.

use editor_config::Platform;
use editor_config::keys::KeyChord;
use gpui::{Context, Keystroke, Subscription, Window};

use super::config_files;
use super::model::{ShortcutKey, default_rule_ids};
use super::view::{ControlRow, SettingsEvent, SettingsView};
use crate::picker::shortcut::{Shortcut, capture_chord, is_lone_modifier, pressed_chord};

/// What a pressed chord is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureFor {
    /// A new shortcut for this command.
    Command(String),
    /// A search for the commands the chord runs.
    Search,
}

/// Waiting for a chord.
pub struct Capture {
    pub target: CaptureFor,
    /// Why the last chord pressed was refused.
    pub rejection: Option<String>,
    _interceptor: Subscription,
}

impl SettingsView {
    /// The command waiting for a new shortcut, if any.
    pub fn capturing(&self) -> Option<&str> {
        match &self.capture.as_ref()?.target {
            CaptureFor::Command(command) => Some(command),
            CaptureFor::Search => None,
        }
    }

    /// Why the chord just pressed was refused, while still waiting.
    pub fn capture_rejection(&self) -> Option<&str> {
        self.capture.as_ref()?.rejection.as_deref()
    }

    /// Whether "Search by keys" is waiting for a chord.
    pub fn searching_by_keys(&self) -> bool {
        self.capture
            .as_ref()
            .is_some_and(|capture| capture.target == CaptureFor::Search)
    }

    /// Waits for the next chord and adds it to `command`.
    pub fn start_capture(&mut self, command: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.wait_for_chord(CaptureFor::Command(command.to_string()), window, cx);
    }

    /// Waits for the next chord and searches for the commands it runs.
    pub fn start_key_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.wait_for_chord(CaptureFor::Search, window, cx);
    }

    fn wait_for_chord(&mut self, target: CaptureFor, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity().downgrade();
        let interceptor = cx.intercept_keystrokes(move |event, window, cx| {
            if let Some(view) = view.upgrade() {
                view.update(cx, |view, cx| {
                    view.on_captured_keystroke(&event.keystroke, window, cx);
                });
            }
        });
        self.menu = None;
        self.error = None;
        self.capture = Some(Capture {
            target,
            rejection: None,
            _interceptor: interceptor,
        });
        window.focus(&self.focus_handle);
        cx.notify();
    }

    pub fn cancel_capture(&mut self, cx: &mut Context<Self>) {
        if self.capture.take().is_some() {
            cx.notify();
        }
    }

    fn on_captured_keystroke(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(capture) = self.capture.as_ref() else {
            return;
        };
        if !self.focus_handle.is_focused(window) || is_lone_modifier(keystroke) {
            return;
        }
        cx.stop_propagation();
        if keystroke.key == "escape" && !keystroke.modifiers.modified() {
            self.cancel_capture(cx);
            return;
        }
        let result = match capture.target.clone() {
            CaptureFor::Command(command) => self.capture_for_command(&command, keystroke, cx),
            CaptureFor::Search => self.capture_for_search(keystroke, window, cx),
        };
        match result {
            Ok(()) => self.capture = None,
            Err(reason) => {
                if let Some(capture) = self.capture.as_mut() {
                    capture.rejection = Some(reason);
                }
            }
        }
        cx.notify();
    }

    fn capture_for_command(
        &mut self,
        command: &str,
        keystroke: &Keystroke,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let chord = capture_chord(keystroke, Platform::current()).map_err(str::to_string)?;
        self.add_shortcut(command, &chord, cx)
    }

    fn capture_for_search(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let platform = Platform::current();
        let (_, chord) = pressed_chord(keystroke, platform)
            .ok_or_else(|| "That key can’t be searched for.".to_string())?;
        let label = Shortcut::new(chord, platform).label();
        self.set_query(&label, cx);
        self.focus_first_control(window, cx);
        Ok(())
    }

    /// Adds `chord` (portable, such as `Mod+Shift+K`) to `command`, or
    /// says why it didn't. A chord another command already uses in the
    /// same place is still added; the row then says which command it
    /// clashes with.
    pub fn add_shortcut(
        &mut self,
        command: &str,
        chord: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let platform = Platform::current();
        let parsed = KeyChord::parse(chord).map(|chord| chord.resolve(platform));
        if let Ok(parsed) = parsed
            && self.rules.keys_for(command, platform).contains(&parsed)
        {
            let label = Shortcut::new(parsed, platform).label();
            return Err(format!("{label} already runs this."));
        }
        let (_, rules) = config_files::add_user_key(&self.vault_root, command, chord)?;
        self.rules_written(&rules, cx);
        Ok(())
    }

    /// Removes one of a command's keys: the user's own rule goes from the
    /// file, and a built-in one is turned off there.
    pub fn remove_key(&mut self, command: &str, key: &ShortcutKey, cx: &mut Context<Self>) {
        let written = match (&key.user_rule, &key.rule) {
            (Some(rule_id), _) => config_files::remove_user_key(&self.vault_root, rule_id),
            (None, Some(rule_id)) => config_files::disable_default_key(&self.vault_root, rule_id),
            (None, None) => Err("That key comes from a rule without an id.".to_string()),
        };
        self.after_write(command, written, cx);
    }

    /// Puts a command back to its built-in keys.
    pub fn reset_shortcuts(&mut self, command: &str, cx: &mut Context<Self>) {
        let ids = default_rule_ids(command);
        let written = config_files::reset_command_keys(&self.vault_root, command, &ids);
        self.after_write(command, written, cx);
    }

    fn after_write(
        &mut self,
        key: &str,
        written: Result<editor_config::RuleSet, String>,
        cx: &mut Context<Self>,
    ) {
        match written {
            Ok(rules) => self.rules_written(&rules, cx),
            Err(message) => self.error = Some((key.to_string(), message)),
        }
        cx.notify();
    }

    fn rules_written(&mut self, rules: &editor_config::RuleSet, cx: &mut Context<Self>) {
        self.error = None;
        self.set_rules(rules, cx);
        cx.emit(SettingsEvent::Changed("rules".to_string()));
    }

    /// Delete on a shortcut row: removes its last key, or once it has
    /// none, puts back the keys it came with.
    pub(super) fn delete_on_shortcut_row(&mut self, row: &ControlRow, cx: &mut Context<Self>) {
        let ControlRow::Shortcut(shortcut) = row else {
            return;
        };
        match shortcut.keys.last() {
            Some(key) => self.remove_key(&shortcut.id, key, cx),
            None if shortcut.changed_from.is_some() => self.reset_shortcuts(&shortcut.id, cx),
            None => {}
        }
    }
}
