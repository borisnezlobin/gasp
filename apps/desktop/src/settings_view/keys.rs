//! Keyboard handling for the settings screen. Tab moves between the
//! search box, the section list and the controls; arrows move within
//! them; Space and Enter toggle; left and right change a choice or number;
//! Delete resets a setting to its default; Escape closes.

use editor_config::schema::SettingKind;
use gpui::{Context, DismissEvent, Focusable, KeyDownEvent, Keystroke, Window};

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
        let handled = self.tab_key(keystroke, window, cx)
            || match self.focus {
                SettingsFocus::Search => self.search_key(keystroke, window, cx),
                SettingsFocus::Sections => self.sections_key(keystroke, window, cx),
                SettingsFocus::Control(index) => self.control_key(index, keystroke, window, cx),
            };
        if handled {
            cx.stop_propagation();
        }
    }

    /// The order Tab walks: search, sections, then each control.
    fn focus_order(&self) -> Vec<SettingsFocus> {
        let controls = (0..self.rows().len()).map(SettingsFocus::Control);
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
        match keystroke.key.as_str() {
            "down" if !self.rows().is_empty() => {
                self.set_focus(SettingsFocus::Control(0), window, cx)
            }
            "down" => self.set_focus(SettingsFocus::Sections, window, cx),
            _ => return false,
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
            "right" | "enter" if !self.rows().is_empty() => {
                self.set_focus(SettingsFocus::Control(0), window, cx)
            }
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
        self.query = self.search.read(cx).text().to_string();
        self.select_section(0, cx);
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
        match keystroke.key.as_str() {
            "up" => self.step_control(index, -1, rows.len(), window, cx),
            "down" => self.step_control(index, 1, rows.len(), window, cx),
            "enter" if row.uses_field() => {
                self.set_focus(SettingsFocus::Control(index), window, cx)
            }
            "escape" => self.escape_control(cx),
            _ if row.uses_field() => return false,
            _ => return self.edit_key(&row, keystroke, window, cx),
        }
        true
    }

    /// Up from the first control goes back to the section list.
    fn step_control(
        &mut self,
        index: usize,
        delta: isize,
        count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = index as isize + delta;
        if target < 0 {
            self.set_focus(SettingsFocus::Sections, window, cx);
        } else if (target as usize) < count {
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
        row: &ControlRow,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(item) = row.item().cloned() else {
            return self.type_to_search(keystroke, window, cx);
        };
        let key = keystroke.key.as_str();
        if matches!(key, "delete" | "backspace") && self.number_edit.is_none() {
            self.reset(&item.key, cx);
            return true;
        }
        match &item.kind {
            SettingKind::Bool => self.bool_key(&item, key, cx),
            SettingKind::Choice(_) => self.choice_key(&item, key, cx),
            SettingKind::Integer | SettingKind::Number => self.number_key(&item, keystroke, cx),
            _ => false,
        }
    }

    fn bool_key(
        &mut self,
        item: &super::model::SettingItem,
        key: &str,
        cx: &mut Context<Self>,
    ) -> bool {
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
        item: &super::model::SettingItem,
        key: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        match key {
            "left" => self.step_choice(item, -1, false, cx),
            "right" => self.step_choice(item, 1, false, cx),
            "space" | "enter" => self.step_choice(item, 1, true, cx),
            _ => return false,
        }
        true
    }

    fn number_key(
        &mut self,
        item: &super::model::SettingItem,
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
}
