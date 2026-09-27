//! The right sidebar from the keyboard: the arrows move between the rows
//! that do something, Enter does what a click does, Left and Right fold a
//! tag, Mod+Enter links an unlinked mention, and Escape hands the
//! keyboard back to the editor.

use gpui::{Context, KeyDownEvent, Window};

use super::sidebar::{KnowledgeSidebar, Row, SidebarEvent, SidebarView};

/// What the sidebar's own keys do, for the sheet holding Mod shows.
pub const KEY_HINTS: [(&str, &str); 4] = [
    ("Enter", "Open"),
    ("Mod+Enter", "Link the mention"),
    ("Right", "Unfold a tag"),
    ("Escape", "Back to the editor"),
];

const PAGE_ROWS: isize = 10;

type KeyHandler = fn(&mut KnowledgeSidebar, &mut Context<KnowledgeSidebar>);

/// Keys pressed without modifiers.
const PLAIN_KEYS: [(&str, KeyHandler); 10] = [
    ("up", |sidebar, cx| sidebar.move_selection(-1, cx)),
    ("down", |sidebar, cx| sidebar.move_selection(1, cx)),
    ("pageup", |sidebar, cx| {
        sidebar.move_selection(-PAGE_ROWS, cx)
    }),
    ("pagedown", |sidebar, cx| {
        sidebar.move_selection(PAGE_ROWS, cx)
    }),
    ("home", |sidebar, cx| {
        sidebar.move_selection(isize::MIN / 2, cx)
    }),
    ("end", |sidebar, cx| {
        sidebar.move_selection(isize::MAX / 2, cx)
    }),
    ("left", |sidebar, cx| sidebar.fold_selected(false, cx)),
    ("right", |sidebar, cx| sidebar.fold_selected(true, cx)),
    ("enter", |sidebar, cx| sidebar.activate_selected(cx)),
    ("escape", |_, cx| cx.emit(SidebarEvent::Dismissed)),
];

/// Keys pressed with Mod (Cmd on macOS, Ctrl elsewhere).
const MOD_KEYS: [(&str, KeyHandler); 1] = [("enter", |sidebar, cx| {
    if let Some(selected) = sidebar.selected {
        sidebar.link_mention(selected, cx);
    }
})];

/// Whether a row does something when chosen, so the keys stop on it.
pub fn is_actionable(row: &Row) -> bool {
    match row {
        Row::Summary(_) | Row::Message(_) => false,
        Row::Outgoing {
            exists, is_note, ..
        } => *exists || *is_note,
        _ => true,
    }
}

impl KnowledgeSidebar {
    /// Takes the keyboard, on the current heading in the outline or the
    /// first row that does something elsewhere.
    pub fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle);
        if self.selected.is_none() {
            let current = self.rows().iter().position(|row| {
                self.view() == SidebarView::Outline
                    && matches!(row, Row::Heading { current: true, .. })
            });
            self.selected = current.or_else(|| self.step_from(None, 1));
        }
        self.reveal_selected();
        cx.notify();
    }

    /// The row the keys act on.
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Keeps the selection on a row that does something after the rows
    /// change, or drops it when none does.
    pub(super) fn keep_selection(&mut self) {
        let Some(selected) = self.selected else {
            return;
        };
        let last = self.rows().len().checked_sub(1);
        let clamped = last.map(|last| selected.min(last));
        self.selected = match clamped {
            Some(at) if is_actionable(&self.rows()[at]) => Some(at),
            Some(at) => self
                .step_from(Some(at), -1)
                .or_else(|| self.step_from(Some(at), 1)),
            None => None,
        };
    }

    /// The next row that does something, `delta` rows on from `from`
    /// (before the first row for `None`), stopping at either end.
    fn step_from(&self, from: Option<usize>, delta: isize) -> Option<usize> {
        let rows = self.rows();
        let start = from.map_or(-1, |at| at as isize);
        let direction = delta.signum();
        let mut target = None;
        let mut steps = delta.abs();
        let mut at = start;
        while steps > 0 {
            at += direction;
            let Some(row) = usize::try_from(at).ok().and_then(|at| rows.get(at)) else {
                break;
            };
            if is_actionable(row) {
                target = Some(at as usize);
                steps -= 1;
            }
        }
        target
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(next) = self.step_from(self.selected, delta) {
            self.selected = Some(next);
            self.reveal_selected();
            cx.notify();
        }
    }

    fn reveal_selected(&self) {
        if let Some(selected) = self.selected {
            self.list.scroll_to_reveal_item(selected);
        }
    }

    /// Does what clicking row `index` does.
    pub fn activate(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(row) = self.rows().get(index).cloned() else {
            return;
        };
        let event = match row {
            Row::Source { path, .. } => SidebarEvent::Open { path, offset: None },
            Row::Context { path, offset, .. } => SidebarEvent::Open {
                path,
                offset: Some(offset),
            },
            Row::Outgoing { target, .. } => SidebarEvent::Follow(target),
            Row::Heading { offset, .. } => SidebarEvent::Jump(offset),
            Row::Tag { name, .. } => SidebarEvent::SearchTag(format!("#{name}")),
            Row::UnlinkedToggle { .. } => return self.toggle_unlinked(cx),
            Row::Summary(_) | Row::Message(_) => return,
        };
        cx.emit(event);
    }

    /// Links the unlinked mention on row `index`, as its Link button does.
    pub fn link_mention(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(Row::Context {
            path,
            mention: Some((range, expected, link)),
            ..
        }) = self.rows().get(index).cloned()
        else {
            return;
        };
        cx.emit(SidebarEvent::LinkMention {
            source: path,
            range,
            expected,
            link,
        });
    }

    /// Folds (`open` false) or unfolds the selected tag's nested tags.
    fn fold_selected(&mut self, open: bool, cx: &mut Context<Self>) {
        let Some(Row::Tag {
            name,
            children: true,
            collapsed,
            ..
        }) = self.selected.and_then(|at| self.rows().get(at)).cloned()
        else {
            return;
        };
        if collapsed == open {
            self.toggle_tag(&name, cx);
        }
    }

    fn activate_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(selected) = self.selected {
            self.activate(selected, cx);
        }
    }

    pub(super) fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        let key = keystroke.key.as_str();
        let handler = if modifiers.secondary() && !modifiers.alt && !modifiers.shift {
            MOD_KEYS.iter().find(|(name, _)| *name == key)
        } else if !modifiers.modified() {
            PLAIN_KEYS.iter().find(|(name, _)| *name == key)
        } else {
            None
        };
        if let Some((_, handler)) = handler {
            handler(self, cx);
            cx.stop_propagation();
        }
    }
}
