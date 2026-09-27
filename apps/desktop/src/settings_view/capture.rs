//! Adding and removing keyboard shortcuts: "+" waits for the next chord,
//! like the command palette does, and writes it to `.editor/rules.toml`.

use editor_config::Platform;
use editor_config::keys::KeyChord;
use gpui::{Context, Keystroke, Subscription, Window};

use super::config_files;
use super::view::{ControlRow, SettingsEvent, SettingsView};
use crate::picker::shortcut::{capture_chord, is_lone_modifier, shortcut_label};

/// Waiting for the chord to add to a command.
pub struct Capture {
    pub command: String,
    /// Why the last chord pressed was refused.
    pub rejection: Option<String>,
    _interceptor: Subscription,
}

impl SettingsView {
    /// The command waiting for a new shortcut, if any.
    pub fn capturing(&self) -> Option<&str> {
        self.capture
            .as_ref()
            .map(|capture| capture.command.as_str())
    }

    /// Waits for the next chord and adds it to `command`.
    pub fn start_capture(&mut self, command: &str, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity().downgrade();
        let interceptor = cx.intercept_keystrokes(move |event, window, cx| {
            if let Some(view) = view.upgrade() {
                view.update(cx, |view, cx| {
                    view.on_captured_keystroke(&event.keystroke, window, cx);
                });
            }
        });
        self.menu = None;
        self.capture = Some(Capture {
            command: command.to_string(),
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
        if self.capture.is_none() || !self.focus_handle.is_focused(window) {
            return;
        }
        if is_lone_modifier(keystroke) {
            return;
        }
        cx.stop_propagation();
        if keystroke.key == "escape" && !keystroke.modifiers.modified() {
            self.cancel_capture(cx);
            return;
        }
        match capture_chord(keystroke, Platform::current()) {
            Ok(chord) => self.finish_capture(&chord, cx),
            Err(reason) => {
                if let Some(capture) = self.capture.as_mut() {
                    capture.rejection = Some(reason.to_string());
                }
                cx.notify();
            }
        }
    }

    fn finish_capture(&mut self, chord: &str, cx: &mut Context<Self>) {
        let Some(capture) = self.capture.take() else {
            return;
        };
        self.add_shortcut(&capture.command, chord, cx);
    }

    /// Adds `chord` (portable, such as `Mod+Shift+K`) to `command`. A chord
    /// another command already uses in the same place is still added; the
    /// row then says which command it clashes with.
    pub fn add_shortcut(&mut self, command: &str, chord: &str, cx: &mut Context<Self>) {
        let platform = Platform::current();
        let parsed = KeyChord::parse(chord).map(|chord| chord.resolve(platform));
        if let Ok(parsed) = parsed
            && self.rules.keys_for(command, platform).contains(&parsed)
        {
            let label = shortcut_label(parsed, platform);
            self.error = Some((command.to_string(), format!("{label} already runs this.")));
            cx.notify();
            return;
        }
        match config_files::add_user_key(&self.vault_root, command, chord) {
            Ok((_, rules)) => self.rules_written(&rules, cx),
            Err(message) => self.error = Some((command.to_string(), message)),
        }
        cx.notify();
    }

    /// Removes a shortcut the user added, by its rule id.
    pub fn remove_shortcut(&mut self, rule_id: &str, cx: &mut Context<Self>) {
        match config_files::remove_user_key(&self.vault_root, rule_id) {
            Ok(rules) => self.rules_written(&rules, cx),
            Err(message) => self.error = Some((rule_id.to_string(), message)),
        }
        cx.notify();
    }

    fn rules_written(&mut self, rules: &editor_config::RuleSet, cx: &mut Context<Self>) {
        self.error = None;
        self.set_rules(rules, cx);
        cx.emit(SettingsEvent::Changed("rules".to_string()));
    }

    /// Removes the last shortcut the user added to a shortcut row.
    pub(super) fn remove_last_user_shortcut(&mut self, row: &ControlRow, cx: &mut Context<Self>) {
        let ControlRow::Shortcut(shortcut) = row else {
            return;
        };
        let last = shortcut
            .keys
            .iter()
            .rev()
            .find_map(|key| key.user_rule.clone());
        if let Some(rule_id) = last {
            self.remove_shortcut(&rule_id, cx);
        }
    }
}
