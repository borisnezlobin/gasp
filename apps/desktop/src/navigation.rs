//! Moving the cursor by character, word, line, page and note.

use editor_core::motion;
use gpui::{Context, Pixels, Window, px};

use crate::editor::EditorView;

/// Where a cursor command moves to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordLeft,
    WordRight,
    LineStart,
    LineEnd,
    DocStart,
    DocEnd,
    PageUp,
    PageDown,
}

impl EditorView {
    /// Moves the cursor, or extends the selection when `extend` is set.
    pub fn apply_motion(
        &mut self,
        motion: Motion,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match motion {
            Motion::Up => self.move_lines(-1, extend, window, cx),
            Motion::Down => self.move_lines(1, extend, window, cx),
            Motion::PageUp => self.move_lines(-self.page_lines(), extend, window, cx),
            Motion::PageDown => self.move_lines(self.page_lines(), extend, window, cx),
            _ => {
                let target = self.horizontal_target(motion, extend);
                self.goal_x = None;
                self.move_to(target, extend, cx);
            }
        }
    }

    /// Left and Right without Shift collapse a selection to its edge
    /// instead of moving past it.
    fn horizontal_target(&self, motion: Motion, extend: bool) -> usize {
        let selection = self.selected_range();
        let collapses = !extend && !selection.is_empty();
        let doc = self.doc();
        let at = self.cursor();
        match motion {
            Motion::Left if collapses => selection.start,
            Motion::Right if collapses => selection.end,
            Motion::Left => doc.prev_char_boundary(at),
            Motion::Right => doc.next_char_boundary(at),
            Motion::WordLeft => motion::word_left(doc, at),
            Motion::WordRight => motion::word_right(doc, at),
            Motion::LineStart => motion::line_start(doc, at),
            Motion::LineEnd => motion::line_end(doc, at),
            Motion::DocStart => 0,
            _ => doc.len(),
        }
    }

    /// Lines in one screenful, less one so a line of context stays.
    fn page_lines(&self) -> isize {
        let line_height = self.theme.line_height(self.theme.body_font_size);
        let viewport = self
            .frame
            .as_ref()
            .map_or(px(0.), |frame| frame.bounds.size.height);
        let lines = (viewport / line_height).floor() as isize;
        (lines - 1).max(1)
    }

    /// Moves `delta` lines up or down, keeping the column the cursor had
    /// before the first vertical move.
    fn move_lines(
        &mut self,
        delta: isize,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let line = self.doc().line_of_offset(self.cursor());
        let last = self.doc().line_count() as isize - 1;
        let target_line = line as isize + delta;
        if target_line < 0 || target_line > last {
            let edge = if delta < 0 { 0 } else { self.doc().len() };
            return self.move_to(edge, extend, cx);
        }
        let goal_x = self.goal_column(line, window);
        let target = self.visual_line(target_line as usize, window);
        self.move_to(target.start + target.offset_for_x(goal_x), extend, cx);
        self.goal_x = Some(goal_x);
    }

    fn goal_column(&mut self, line: usize, window: &mut Window) -> Pixels {
        if let Some(goal) = self.goal_x {
            return goal;
        }
        let current = self.visual_line(line, window);
        current.x_for_offset(self.cursor() - current.start)
    }
}
