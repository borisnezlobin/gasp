//! Mouse handling and the view's render tree. Keys are bound in
//! [`crate::keymap`] and run through [`crate::commands`].

use std::ops::Range;

use editor_core::motion;
use gpui::{
    Context, CursorStyle, KeyDownEvent, ModifiersChangedEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollWheelEvent, Window, div, prelude::*,
};

use crate::editor::{EditorEvent, EditorView};

/// How often a drag held past the note's edge scrolls it: once a frame.
const DRAG_SCROLL_TICK: std::time::Duration = std::time::Duration::from_millis(16);
/// Pixels scrolled each frame for every pixel the pointer is past the
/// edge, so a little past creeps and far past runs.
const DRAG_SCROLL_RATE: f32 = 0.4;
/// The fastest a held drag scrolls, in pixels a frame.
const DRAG_SCROLL_MAX: Pixels = gpui::px(60.);
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
        if keystroke.key != "escape" || keystroke.modifiers.modified() {
            return;
        }
        let previewing = self.hover.open.is_some();
        self.close_preview(cx);
        let offered = self.dismiss_card_offer(cx);
        if self.dismiss_suggestions(cx) || previewing || offered {
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
        self.close_preview(cx);
        if !self.read_only {
            window.focus(&self.focus_handle);
        }
        let secondary = event.modifiers.secondary();
        if self.click_copy_button(event.position, cx)
            || self.click_widget(event.position, secondary, cx)
        {
            return;
        }
        let offset = self.offset_for_point(event.position, window);
        if event.modifiers.secondary()
            && let Some(target) = self.link_at(offset)
        {
            cx.emit(EditorEvent::OpenLink(target));
            return;
        }
        if self.read_only {
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

    /// The pointer moved, anywhere in the window, during a drag selection:
    /// selects to it, and scrolls while it's past the note's edge.
    pub(crate) fn drag_moved(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_selecting {
            return;
        }
        let offset = self.offset_for_point(position, window);
        self.extend_by_unit(offset, cx);
        self.pointer_at = Some(position);
        self.start_drag_scroll(window, cx);
    }

    /// How far the pointer is past the note's top (negative) or bottom
    /// (positive): zero while it's over the note.
    fn drag_overshoot(&self, y: Pixels) -> Pixels {
        let Some(frame) = self.frame.as_ref() else {
            return Pixels::ZERO;
        };
        let (top, bottom) = (frame.bounds.top(), frame.bounds.bottom());
        if y < top {
            y - top
        } else if y > bottom {
            y - bottom
        } else {
            Pixels::ZERO
        }
    }

    /// Keeps a drag selection held past the note's edge scrolling, each
    /// frame, until it comes back, lets go or reaches the end.
    fn start_drag_scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let past = self
            .pointer_at
            .is_some_and(|at| self.drag_overshoot(at.y) != Pixels::ZERO);
        if !past || self.drag_scroll.is_some() {
            return;
        }
        self.drag_scroll = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(DRAG_SCROLL_TICK).await;
                let going = this
                    .update_in(cx, |view, window, cx| view.drag_scroll_tick(window, cx))
                    .unwrap_or(false);
                if !going {
                    break;
                }
            }
            this.update(cx, |view, _| view.drag_scroll = None).ok();
        }));
    }

    /// One frame of a drag held past the edge: scrolls faster the further
    /// past it the pointer is, and selects to the line now under it.
    fn drag_scroll_tick(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(at) = self.pointer_at.filter(|_| self.is_selecting) else {
            return false;
        };
        let past = self.drag_overshoot(at.y);
        if past == Pixels::ZERO {
            return false;
        }
        let before = self.scroll_offset();
        let step = (past * DRAG_SCROLL_RATE).clamp(-DRAG_SCROLL_MAX, DRAG_SCROLL_MAX);
        self.scroll_by(step, cx);
        let offset = self.offset_for_point(at, window);
        self.extend_by_unit(offset, cx);
        self.scroll_offset() != before
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

    /// Clicks on a checkbox, a callout's fold control or a card's Open
    /// button act on it instead of placing the cursor, as Mod+click on a
    /// link card does.
    fn click_widget(
        &mut self,
        position: Point<Pixels>,
        secondary: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let hit = self
            .frame
            .as_ref()
            .and_then(|frame| frame.piece_at(position))
            .map(|(_, piece)| piece.hit.clone());
        match hit {
            Some(Hit::Checkbox { .. }) if self.read_only => {}
            Some(Hit::Checkbox { marker }) => self.toggle_task(marker, cx),
            Some(Hit::Fold { header, folded }) => self.toggle_fold(header, folded, cx),
            Some(Hit::Link { url }) => cx.emit(EditorEvent::OpenLink(url)),
            Some(Hit::Card { url }) if secondary => cx.emit(EditorEvent::OpenLink(url)),
            _ => return false,
        }
        true
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        // The editor's element follows a drag selection window-wide.
        if self.is_selecting {
            return;
        }
        let over = self.hover_target_at_point(event.position);
        self.hover_moved(over, event.modifiers.secondary(), cx);
        self.pointer_at = Some(event.position);
        self.point_at(event.modifiers.secondary(), cx);
        self.hover_code(Some(event.position), cx);
    }

    /// Follows what's under the pointer: the task box and link card that
    /// show they're clickable, and the pointer's look. Redraws only when
    /// one of them changes.
    fn point_at(&mut self, secondary: bool, cx: &mut Context<Self>) {
        let under = self
            .pointer_at
            .zip(self.frame.as_ref())
            .and_then(|(position, frame)| frame.piece_at(position))
            .map(|(placed, piece)| (placed.visual.line, piece.hit.clone()));
        let task = match &under {
            Some((_, Hit::Checkbox { marker })) => Some(marker.start),
            _ => None,
        };
        let card = match &under {
            Some((line, Hit::Card { .. })) => Some((*line, false)),
            Some((line, Hit::Link { .. })) => Some((*line, true)),
            _ => None,
        };
        let on_link = || {
            self.pointer_at
                .zip(self.frame.as_ref())
                .and_then(|(position, frame)| frame.offset_at(position))
                .and_then(|offset| self.link_at(offset))
                .is_some()
        };
        let cursor = pointer_style(under.as_ref().map(|(_, hit)| hit), secondary, on_link);
        let changed =
            (task, card, cursor) != (self.hovered_task, self.hovered_card, self.pointer_cursor);
        if changed {
            self.hovered_task = task;
            self.hovered_card = card;
            self.pointer_cursor = cursor;
            cx.notify();
        }
    }

    /// The pointer left the editor: link previews and code copy buttons
    /// that follow it go away.
    fn on_hover_editor(&mut self, hovered: &bool, _: &mut Window, cx: &mut Context<Self>) {
        if !*hovered {
            self.hover_left(cx);
            self.hover_code(None, cx);
            self.pointer_at = None;
            self.point_at(false, cx);
        }
    }

    fn on_modifiers_changed(
        &mut self,
        event: &ModifiersChangedEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.hover_modifiers_changed(event.modifiers.secondary(), cx);
        self.point_at(event.modifiers.secondary(), cx);
    }

    fn on_scroll(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.close_preview(cx);
        let delta = event.delta.pixel_delta(self.theme.body_line_height());
        self.scroll_by(-delta.y, cx);
    }
}

/// A hand where a click acts (and over links and cards while Mod is
/// held, as Mod+click opens them), an arrow over a card, which a click
/// selects rather than types into, and a text cursor elsewhere.
fn pointer_style(hit: Option<&Hit>, secondary: bool, on_link: impl Fn() -> bool) -> CursorStyle {
    match hit {
        Some(hit) if hit.is_control() => CursorStyle::PointingHand,
        Some(Hit::Card { .. }) if secondary => CursorStyle::PointingHand,
        Some(Hit::Card { .. }) => CursorStyle::Arrow,
        _ if secondary && on_link() => CursorStyle::PointingHand,
        _ => CursorStyle::IBeam,
    }
}

impl Render for EditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.read_only {
            return div()
                .id("preview")
                .size_full()
                .bg(self.theme.background)
                .rounded(self.corner_radius)
                .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
                .on_scroll_wheel(cx.listener(Self::on_scroll))
                .child(EditorElement::new(cx.entity()));
        }
        div()
            .id("editor")
            .size_full()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .cursor(self.pointer_cursor)
            .bg(self.theme.background)
            .on_action(cx.listener(Self::on_run_command))
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_hover(cx.listener(Self::on_hover_editor))
            .on_modifiers_changed(cx.listener(Self::on_modifiers_changed))
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .on_drop(cx.listener(Self::on_drop_paths))
            .child(EditorElement::new(cx.entity()))
    }
}
