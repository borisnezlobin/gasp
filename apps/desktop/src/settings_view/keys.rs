//! Keyboard handling for the settings screen. Tab moves between the
//! search box, the section list and the controls; arrows move within
//! them; Space and Enter toggle a switch, open a dropdown or press a
//! button; left and right change a choice, number or colour; Delete
//! resets a setting or removes a row's last shortcut; Escape closes.

use gasp_config::schema::SettingKind;
use gpui::{App, Context, DismissEvent, Focusable, KeyDownEvent, Keystroke, Window};

use super::model::{ICON_SOURCE_URL, SettingItem};
use super::view::{ControlRow, SettingsFocus, SettingsView};

impl SettingsView {
    pub(super) fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystroke = &event.keystroke;
        if self.focus == SettingsFocus::Search && !self.search.focus_handle(cx).is_focused(window) {
            self.focus = SettingsFocus::Sections;
        }
        let typing_hex = self.hex_field.focus_handle(cx).is_focused(window);
        let handled = if self.snippet_editor_key(keystroke, window, cx) {
            true
        } else if self.menu.is_some() {
            self.menu_key(&keystroke.key, window, cx)
        } else if typing_hex {
            // The hex field takes typing; only Tab leaves it.
            self.tab_key(keystroke, window, cx)
        } else {
            self.focus_key(keystroke, window, cx)
        };
        if handled {
            cx.stop_propagation();
        }
    }

    fn focus_key(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.tab_key(keystroke, window, cx)
            || match self.focus {
                SettingsFocus::Search => self.search_key(keystroke, window, cx),
                SettingsFocus::Sections => self.sections_key(keystroke, window, cx),
                SettingsFocus::Control(index) => self.control_key(index, keystroke, window, cx),
            }
    }

    /// The order Tab walks: search, sections, then each control.
    fn focus_order(&self) -> Vec<SettingsFocus> {
        let controls = self
            .rows()
            .iter()
            .enumerate()
            .filter(|(_, row)| self.takes_focus(row))
            .map(|(index, _)| SettingsFocus::Control(index))
            .collect::<Vec<_>>();
        [SettingsFocus::Search, SettingsFocus::Sections]
            .into_iter()
            .chain(controls)
            .collect()
    }

    fn tab_key(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if keystroke.key != "tab" || keystroke.modifiers.control || keystroke.modifiers.platform {
            return false;
        }
        let order = self.focus_order();
        let at = order.iter().position(|f| *f == self.focus).unwrap_or(0) as isize;
        let step = if keystroke.modifiers.shift { -1 } else { 1 };
        let next = (at + step).rem_euclid(order.len() as isize) as usize;
        self.set_focus(order[next], window, cx);
        true
    }

    fn search_key(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if keystroke.key != "down" {
            return false;
        }
        if self.rows().iter().any(|row| self.takes_focus(row)) {
            self.focus_first_control(window, cx);
        } else {
            self.set_focus(SettingsFocus::Sections, window, cx);
        }
        true
    }

    fn sections_key(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let current = self.current;
        match keystroke.key.as_str() {
            "up" if current == 0 => self.set_focus(SettingsFocus::Search, window, cx),
            "up" => self.select_section(current - 1, cx),
            "down" => self.select_section(current + 1, cx),
            "right" | "enter" => self.focus_first_control(window, cx),
            "escape" => cx.emit(DismissEvent),
            _ => return self.type_to_search(keystroke, window, cx),
        }
        true
    }

    /// Typing anywhere outside a field starts a search.
    fn type_to_search(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let modifiers = keystroke.modifiers;
        let text = keystroke
            .key_char
            .as_deref()
            .filter(|text| !modifiers.control && !modifiers.platform && !text.trim().is_empty());
        let Some(text) = text else {
            return false;
        };
        let text = text.to_string();
        self.set_focus(SettingsFocus::Search, window, cx);
        self.search.update(cx, |field, cx| {
            let typed = format!("{}{text}", field.text());
            field.set_text(&typed, cx);
        });
        let typed = self.search.read(cx).text().to_string();
        self.set_query(&typed, cx);
        true
    }

    fn control_key(
        &mut self,
        index: usize,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let rows = self.rows();
        let Some(row) = rows.get(index).cloned() else {
            self.set_focus(SettingsFocus::Sections, window, cx);
            return false;
        };
        let moves_item = keystroke.modifiers.alt && matches!(row, ControlRow::ToolbarItem { .. });
        match keystroke.key.as_str() {
            "up" if !moves_item => self.step_control(index, -1, &rows, window, cx),
            "down" if !moves_item => self.step_control(index, 1, &rows, window, cx),
            "enter" if self.row_uses_field(&row) => {
                self.set_focus(SettingsFocus::Control(index), window, cx)
            }
            "escape" => self.escape_control(cx),
            _ if self.row_uses_field(&row) => return false,
            _ => return self.edit_key(index, &row, keystroke, window, cx),
        }
        true
    }

    /// Moves to the next row that takes focus. Up from the first goes back
    /// to the section list.
    fn step_control(
        &mut self,
        index: usize,
        delta: isize,
        rows: &[ControlRow],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut target = index as isize + delta;
        while target >= 0
            && (target as usize) < rows.len()
            && !self.takes_focus(&rows[target as usize])
        {
            target += delta;
        }
        if target < 0 {
            self.set_focus(SettingsFocus::Sections, window, cx);
        } else if (target as usize) < rows.len() {
            self.set_focus(SettingsFocus::Control(target as usize), window, cx);
        }
    }

    /// Escape drops a half-typed number, or else closes the screen.
    fn escape_control(&mut self, cx: &mut Context<Self>) {
        if self.number_edit.take().is_some() {
            cx.notify();
        } else {
            cx.emit(DismissEvent);
        }
    }

    /// Keys that change the focused control's value.
    fn edit_key(
        &mut self,
        index: usize,
        row: &ControlRow,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = keystroke.key.as_str();
        let handled = match row {
            ControlRow::Setting(item) | ControlRow::MapEntry { item, .. } => {
                self.setting_key(index, item, keystroke, window, cx)
            }
            ControlRow::Font(slot) => self.font_key(index, slot.token(), key, window, cx),
            ControlRow::Accent => self.accent_key(key, window, cx),
            ControlRow::Vault | ControlRow::ObsidianImport => self.button_key(key, row, cx),
            ControlRow::IconCredit => open_url_key(key, ICON_SOURCE_URL, cx),
            ControlRow::Shortcut(shortcut) => self.shortcut_key(row, &shortcut.id, key, window, cx),
            ControlRow::MapAdd(_) => self.menu_button_key(index, row, key, window, cx),
            ControlRow::SyncAccount | ControlRow::SyncRemote => self.sync_row_key(row, key, cx),
            ControlRow::ListEntry { list, value } => self.list_entry_key(list, value, key, cx),
            ControlRow::SnippetsFile | ControlRow::Snippet(_) | ControlRow::Replacement(_) => {
                self.typing_row_key(row, key, window, cx)
            }
            _ if row.is_toolbar_row() => self.toolbar_key(index, row, keystroke, window, cx),
            _ => false,
        };
        handled || self.type_to_search(keystroke, window, cx)
    }

    /// Enter or Space opens the row's menu, for rows that are only a menu.
    fn menu_button_key(
        &mut self,
        index: usize,
        row: &ControlRow,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let opens = matches!(key, "enter" | "space") && Self::menu_target(row).is_some();
        if opens {
            self.open_menu(index, window, cx);
        }
        opens
    }

    fn setting_key(
        &mut self,
        index: usize,
        item: &SettingItem,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = keystroke.key.as_str();
        if self.is_inactive(item) {
            // Its switch is off: the keys that would change it do nothing.
            return matches!(
                key,
                "left" | "right" | "space" | "enter" | "delete" | "backspace"
            );
        }
        if matches!(key, "delete" | "backspace") && self.number_edit.is_none() {
            self.reset(&item.key, cx);
            return true;
        }
        match &item.kind {
            SettingKind::Bool => self.bool_key(item, key, cx),
            SettingKind::Choice(_) => self.choice_key(index, item, key, window, cx),
            SettingKind::Integer | SettingKind::Number => self.number_key(item, keystroke, cx),
            _ => false,
        }
    }

    fn bool_key(&mut self, item: &SettingItem, key: &str, cx: &mut Context<Self>) -> bool {
        let on = self.current_value(item).as_bool().unwrap_or(false);
        let wanted = match key {
            "space" | "enter" => !on,
            "left" => false,
            "right" => true,
            _ => return false,
        };
        if wanted != on {
            self.toggle(item, cx);
        }
        true
    }

    fn choice_key(
        &mut self,
        index: usize,
        item: &SettingItem,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match key {
            "left" => self.step_choice(item, -1, cx),
            "right" => self.step_choice(item, 1, cx),
            "space" | "enter" => self.open_menu(index, window, cx),
            _ => return false,
        }
        true
    }

    fn number_key(
        &mut self,
        item: &SettingItem,
        keystroke: &Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        match keystroke.key.as_str() {
            "left" | "-" if self.number_edit.is_none() => self.step_number(item, -1, cx),
            "right" | "=" | "+" if self.number_edit.is_none() => self.step_number(item, 1, cx),
            "enter" => self.commit_number(cx),
            "backspace" => self.backspace_number(cx),
            _ => {
                let Some(digits) = keystroke
                    .key_char
                    .as_deref()
                    .filter(|text| text.chars().all(|c| c.is_ascii_digit() || c == '.'))
                else {
                    return false;
                };
                self.type_number(item, digits, cx);
            }
        }
        true
    }

    fn font_key(
        &mut self,
        index: usize,
        token: &str,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match key {
            "space" | "enter" => self.open_menu(index, window, cx),
            "delete" | "backspace" => self.write_token(token, None, cx),
            _ => return false,
        }
        true
    }

    fn accent_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match key {
            "left" => self.step_accent(-1, cx),
            "right" => self.step_accent(1, cx),
            "enter" => window.focus(&self.hex_field.focus_handle(cx)),
            "delete" | "backspace" => {
                self.write_token(self.accent_token(), None, cx);
            }
            _ => return false,
        }
        true
    }

    /// Enter or Space on a General page button runs its command.
    fn button_key(&mut self, key: &str, row: &ControlRow, cx: &mut Context<Self>) -> bool {
        let command = match row {
            ControlRow::ObsidianImport => "vault.import-obsidian",
            _ => "vault.open",
        };
        if matches!(key, "space" | "enter") {
            self.request_command(command, cx);
            return true;
        }
        false
    }

    fn shortcut_key(
        &mut self,
        row: &ControlRow,
        command: &str,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match key {
            "space" | "enter" | "+" | "=" => self.start_capture(command, window, cx),
            "delete" | "backspace" => self.delete_on_shortcut_row(row, cx),
            _ => return false,
        }
        true
    }

    /// Space switches a snippet or replacement on or off, Enter opens a
    /// snippet in the editor, and either adds one on the first row.
    fn typing_row_key(
        &mut self,
        row: &ControlRow,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match (row, key) {
            (ControlRow::SnippetsFile, "space" | "enter") => {
                self.open_snippet_editor(None, window, cx)
            }
            (ControlRow::Snippet(row), "enter") => {
                self.open_snippet_editor(Some(row.line), window, cx)
            }
            (ControlRow::Snippet(row), "space") => self.toggle_snippet(row.line, cx),
            (ControlRow::Replacement(row), "space" | "enter") => {
                self.toggle_replacement(row.index, cx)
            }
            _ => return false,
        }
        true
    }
}

/// Enter or Space on a row whose only control is a link opens it.
fn open_url_key(key: &str, url: &str, cx: &mut App) -> bool {
    if matches!(key, "space" | "enter") {
        crate::sandbox::open_url(url, cx);
        return true;
    }
    false
}
