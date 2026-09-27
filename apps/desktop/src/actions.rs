//! Keyboard actions, their default bindings, and mouse handling.

use gpui::{
    App, ClipboardItem, Context, CursorStyle, KeyBinding, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ScrollWheelEvent, Window, actions, div, prelude::*,
};

use crate::editor::EditorView;
use crate::element::EditorElement;

actions!(
    editor,
    [
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        MoveToLineStart,
        MoveToLineEnd,
        SelectToLineStart,
        SelectToLineEnd,
        SelectAll,
        Backspace,
        Delete,
        Newline,
        Undo,
        Redo,
        Copy,
        Cut,
        Paste,
        Quit,
    ]
);

const KEY_CONTEXT: &str = "Editor";

/// Binds the editor's default keys. `secondary` is Cmd on macOS and Ctrl
/// elsewhere.
pub fn bind_keys(cx: &mut App) {
    let context = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("left", MoveLeft, context),
        KeyBinding::new("right", MoveRight, context),
        KeyBinding::new("up", MoveUp, context),
        KeyBinding::new("down", MoveDown, context),
        KeyBinding::new("shift-left", SelectLeft, context),
        KeyBinding::new("shift-right", SelectRight, context),
        KeyBinding::new("shift-up", SelectUp, context),
        KeyBinding::new("shift-down", SelectDown, context),
        KeyBinding::new("home", MoveToLineStart, context),
        KeyBinding::new("end", MoveToLineEnd, context),
        KeyBinding::new("shift-home", SelectToLineStart, context),
        KeyBinding::new("shift-end", SelectToLineEnd, context),
        KeyBinding::new("secondary-a", SelectAll, context),
        KeyBinding::new("backspace", Backspace, context),
        KeyBinding::new("delete", Delete, context),
        KeyBinding::new("enter", Newline, context),
        KeyBinding::new("secondary-z", Undo, context),
        KeyBinding::new("secondary-shift-z", Redo, context),
        KeyBinding::new("secondary-c", Copy, context),
        KeyBinding::new("secondary-x", Cut, context),
        KeyBinding::new("secondary-v", Paste, context),
        KeyBinding::new("secondary-q", Quit, None),
    ]);
    cx.on_action(|_: &Quit, cx| cx.quit());
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Backward,
    Forward,
}

impl EditorView {
    fn move_horizontally(&mut self, direction: Direction, extend: bool, cx: &mut Context<Self>) {
        self.goal_x = None;
        let selection = self.selected_range();
        if !extend && !selection.is_empty() {
            let edge = match direction {
                Direction::Backward => selection.start,
                Direction::Forward => selection.end,
            };
            return self.move_to(edge, false, cx);
        }
        let target = match direction {
            Direction::Backward => self.doc().prev_char_boundary(self.cursor()),
            Direction::Forward => self.doc().next_char_boundary(self.cursor()),
        };
        self.move_to(target, extend, cx);
    }

    fn move_vertically(
        &mut self,
        direction: Direction,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let line = self.doc().line_of_offset(self.cursor());
        let target_line = match direction {
            Direction::Backward => line.checked_sub(1),
            Direction::Forward => Some(line + 1).filter(|next| *next < self.doc().line_count()),
        };
        let Some(target_line) = target_line else {
            let edge = if direction == Direction::Backward {
                0
            } else {
                self.doc().len()
            };
            return self.move_to(edge, extend, cx);
        };
        let current = self.visual_line(line, window);
        let cursor = self.cursor();
        let goal_x = *self
            .goal_x
            .get_or_insert_with(|| current.x_for_offset(cursor - current.start));
        let target = self.visual_line(target_line, window);
        self.move_to(target.start + target.offset_for_x(goal_x), extend, cx);
        self.goal_x = Some(goal_x);
    }

    fn move_to_line_edge(&mut self, to_end: bool, extend: bool, cx: &mut Context<Self>) {
        self.goal_x = None;
        let line = self.doc().line_of_offset(self.cursor());
        let range = self.doc().line_range(line);
        let target = if to_end { range.end } else { range.start };
        self.move_to(target, extend, cx);
    }

    fn delete_or_selection(&mut self, direction: Direction, cx: &mut Context<Self>) {
        let mut range = self.selected_range();
        if range.is_empty() {
            range = match direction {
                Direction::Backward => self.doc().prev_char_boundary(range.start)..range.end,
                Direction::Forward => range.start..self.doc().next_char_boundary(range.end),
            };
        }
        self.replace(range, "", cx);
    }

    fn selected_text(&self) -> Option<String> {
        let range = self.selected_range();
        (!range.is_empty()).then(|| self.doc().slice(range))
    }

    fn on_move_left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.move_horizontally(Direction::Backward, false, cx);
    }

    fn on_move_right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        self.move_horizontally(Direction::Forward, false, cx);
    }

    fn on_select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.move_horizontally(Direction::Backward, true, cx);
    }

    fn on_select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.move_horizontally(Direction::Forward, true, cx);
    }

    fn on_move_up(&mut self, _: &MoveUp, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(Direction::Backward, false, window, cx);
    }

    fn on_move_down(&mut self, _: &MoveDown, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(Direction::Forward, false, window, cx);
    }

    fn on_select_up(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(Direction::Backward, true, window, cx);
    }

    fn on_select_down(&mut self, _: &SelectDown, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(Direction::Forward, true, window, cx);
    }

    fn on_line_start(&mut self, _: &MoveToLineStart, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to_line_edge(false, false, cx);
    }

    fn on_line_end(&mut self, _: &MoveToLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to_line_edge(true, false, cx);
    }

    fn on_select_line_start(
        &mut self,
        _: &SelectToLineStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_to_line_edge(false, true, cx);
    }

    fn on_select_line_end(&mut self, _: &SelectToLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to_line_edge(true, true, cx);
    }

    fn on_select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.select(0, self.doc().len(), cx);
    }

    fn on_backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        self.delete_or_selection(Direction::Backward, cx);
    }

    fn on_delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        self.delete_or_selection(Direction::Forward, cx);
    }

    fn on_newline(&mut self, _: &Newline, _: &mut Window, cx: &mut Context<Self>) {
        self.insert("\n", cx);
    }

    fn on_undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.undo(cx);
    }

    fn on_redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        self.redo(cx);
    }

    fn on_copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn on_cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            self.insert("", cx);
        }
    }

    fn on_paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.insert(&text, cx);
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
        self.is_selecting = true;
        self.goal_x = None;
        let offset = self.offset_for_point(event.position, window);
        self.move_to(offset, event.modifiers.shift, cx);
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
            self.move_to(offset, true, cx);
        }
    }

    fn on_scroll(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let line_height = self.theme.line_height(self.theme.body_font_size);
        let delta = event.delta.pixel_delta(line_height);
        self.scroll_by(-delta.y, cx);
    }
}

impl Render for EditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .bg(self.theme.background)
            .on_action(cx.listener(Self::on_move_left))
            .on_action(cx.listener(Self::on_move_right))
            .on_action(cx.listener(Self::on_select_left))
            .on_action(cx.listener(Self::on_select_right))
            .on_action(cx.listener(Self::on_move_up))
            .on_action(cx.listener(Self::on_move_down))
            .on_action(cx.listener(Self::on_select_up))
            .on_action(cx.listener(Self::on_select_down))
            .on_action(cx.listener(Self::on_line_start))
            .on_action(cx.listener(Self::on_line_end))
            .on_action(cx.listener(Self::on_select_line_start))
            .on_action(cx.listener(Self::on_select_line_end))
            .on_action(cx.listener(Self::on_select_all))
            .on_action(cx.listener(Self::on_backspace))
            .on_action(cx.listener(Self::on_delete))
            .on_action(cx.listener(Self::on_newline))
            .on_action(cx.listener(Self::on_undo))
            .on_action(cx.listener(Self::on_redo))
            .on_action(cx.listener(Self::on_copy))
            .on_action(cx.listener(Self::on_cut))
            .on_action(cx.listener(Self::on_paste))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .child(EditorElement::new(cx.entity()))
    }
}
