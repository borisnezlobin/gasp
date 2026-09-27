//! Mouse handling and the view's render tree. Keys are bound in
//! [`crate::keymap`] and run through [`crate::commands`].

use std::ops::Range;

use editor_core::motion;
use gpui::{
    Context, CursorStyle, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, ScrollWheelEvent, Window, div, prelude::*,
};

use crate::editor::{EditorEvent, EditorView};
use crate::element::EditorElement;
use crate::keymap::{KEY_CONTEXT, RunCommand};
use crate::line_layout::Hit;

pub use crate::keymap::bind_keys;

/// What a click selects: one click places the caret, two select a word,
/// three select a line. Dragging extends by the same unit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClickUnit {
    #[default]
    Character,
    Word,
    Line,
}

impl ClickUnit {
    fn from_count(count: usize) -> ClickUnit {
        match count {
            0 | 1 => ClickUnit::Character,
            2 => ClickUnit::Word,
            _ => ClickUnit::Line,
        }
    }
}

impl EditorView {
    /// Commands the editor doesn't run bubble up to the workspace.
    fn on_run_command(&mut self, action: &RunCommand, window: &mut Window, cx: &mut Context<Self>) {
        if !self.run_command(&action.id, window, cx) {
            cx.propagate();
        }
    }

    /// Escape closes the suggestion list; otherwise it goes on to the
    /// workspace.
    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if keystroke.key == "escape"
            && !keystroke.modifiers.modified()
            && self.dismiss_suggestions(cx)
        {
            cx.stop_propagation();
        }
    }

    /// The range `unit` covers at `offset`.
    fn unit_at(&self, unit: ClickUnit, offset: usize) -> Range<usize> {
        match unit {
            ClickUnit::Character => offset..offset,
            ClickUnit::Word => motion::word_at(self.doc(), offset),
            ClickUnit::Line => motion::line_at(self.doc(), offset),
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
        if self.click_copy_button(event.position, cx) || self.click_widget(event.position, cx) {
            return;
        }
        let offset = self.offset_for_point(event.position, window);
        if event.modifiers.secondary()
            && let Some(target) = self.link_at(offset)
        {
            cx.emit(EditorEvent::OpenLink(target));
            return;
        }
        self.is_selecting = true;
        self.goal_x = None;
        self.click_unit = ClickUnit::from_count(event.click_count);
        if event.modifiers.shift {
            self.click_origin = self.anchor()..self.anchor();
            return self.extend_by_unit(offset, cx);
        }
        let range = self.unit_at(self.click_unit, offset);
        self.click_origin = range.clone();
        self.select(range.start, range.end, cx);
    }

    /// Selects from the click's unit to the unit under `offset`, so a
    /// double-click drag grows word by word.
    fn extend_by_unit(&mut self, offset: usize, cx: &mut Context<Self>) {
        let origin = self.click_origin.clone();
        let under = self.unit_at(self.click_unit, offset);
        if under.start < origin.start {
            self.select(origin.end, under.start, cx);
        } else {
            self.select(origin.start, under.end.max(origin.end), cx);
        }
    }

    /// Clicks on a checkbox or a callout's fold control act on it instead
    /// of placing the cursor.
    fn click_widget(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        let hit = self
            .frame
            .as_ref()
            .and_then(|frame| frame.piece_at(position))
            .map(|(_, piece)| piece.hit.clone());
        match hit {
            Some(Hit::Checkbox { marker }) => self.toggle_task(marker, cx),
            Some(Hit::Fold { header, folded }) => self.toggle_fold(header, folded, cx),
            _ => return false,
        }
        true
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_selecting {
            let offset = self.offset_for_point(event.position, window);
            self.extend_by_unit(offset, cx);
        }
        self.hover_code(Some(event.position), cx);
    }

    fn on_scroll(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(self.theme.body_line_height());
        self.scroll_by(-delta.y, cx);
    }
}

impl Render for EditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("editor")
            .size_full()
            .key_context(KEY_CONTEXT)
            .on_hover(cx.listener(|editor, hovered: &bool, _, cx| {
                if !*hovered {
                    editor.hover_code(None, cx);
                }
            }))
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .bg(self.theme.background)
            .on_action(cx.listener(Self::on_run_command))
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .on_drop(cx.listener(Self::on_drop_paths))
            .child(EditorElement::new(cx.entity()))
    }
}
