//! The picker's one-line query input: typing, deleting by character, word
//! or to the start, cursor motion, selection, the clipboard, the mouse and
//! IME composition through `EntityInputHandler`.
//!
//! Editing keys come from the same rules as the editor's (`cursor.*`,
//! `select.*`, `edit.delete-*`, `edit.copy`, ...), bound as `RunCommand` in
//! the [`INPUT_CONTEXT`] key context by [`crate::picker::bind_keys`].

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PaintQuad, ParentElement, Pixels, Point, Render, ShapedLine, SharedString, Style,
    Styled, TextRun, UTF16Selection, UnderlineStyle, Window, div, fill, point, prelude::*,
    relative, size,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::keymap::RunCommand;
use crate::theme::PickerTheme;

/// The key context the query input sets.
pub const INPUT_CONTEXT: &str = "PickerInput";

/// What the query input tells its picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryEvent {
    /// The text changed.
    Changed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Motion {
    Left,
    Right,
    WordLeft,
    WordRight,
    Start,
    End,
}

type InputHandler = fn(&mut QueryInput, &mut Context<QueryInput>);

/// The commands the input runs, by id.
const INPUT_COMMANDS: [(&str, InputHandler); 25] = [
    ("cursor.left", |input, cx| input.go(Motion::Left, false, cx)),
    ("cursor.right", |input, cx| {
        input.go(Motion::Right, false, cx)
    }),
    ("cursor.word-left", |input, cx| {
        input.go(Motion::WordLeft, false, cx)
    }),
    ("cursor.word-right", |input, cx| {
        input.go(Motion::WordRight, false, cx)
    }),
    ("cursor.line-start", |input, cx| {
        input.go(Motion::Start, false, cx)
    }),
    ("cursor.line-end", |input, cx| {
        input.go(Motion::End, false, cx)
    }),
    ("cursor.doc-start", |input, cx| {
        input.go(Motion::Start, false, cx)
    }),
    ("cursor.doc-end", |input, cx| {
        input.go(Motion::End, false, cx)
    }),
    ("select.left", |input, cx| input.go(Motion::Left, true, cx)),
    ("select.right", |input, cx| {
        input.go(Motion::Right, true, cx)
    }),
    ("select.word-left", |input, cx| {
        input.go(Motion::WordLeft, true, cx)
    }),
    ("select.word-right", |input, cx| {
        input.go(Motion::WordRight, true, cx)
    }),
    ("select.line-start", |input, cx| {
        input.go(Motion::Start, true, cx)
    }),
    ("select.line-end", |input, cx| {
        input.go(Motion::End, true, cx)
    }),
    ("select.doc-start", |input, cx| {
        input.go(Motion::Start, true, cx)
    }),
    ("select.doc-end", |input, cx| {
        input.go(Motion::End, true, cx)
    }),
    ("select.all", |input, cx| input.select_all(cx)),
    ("edit.delete-backward", |input, cx| {
        input.delete(Motion::Left, cx)
    }),
    ("edit.delete-forward", |input, cx| {
        input.delete(Motion::Right, cx)
    }),
    ("edit.delete-word-backward", |input, cx| {
        input.delete(Motion::WordLeft, cx)
    }),
    ("edit.delete-word-forward", |input, cx| {
        input.delete(Motion::WordRight, cx)
    }),
    ("edit.delete-to-line-start", |input, cx| {
        input.delete(Motion::Start, cx)
    }),
    ("edit.delete-to-line-end", |input, cx| {
        input.delete(Motion::End, cx)
    }),
    ("edit.copy", |input, cx| input.copy(cx)),
    ("edit.cut", |input, cx| input.cut(cx)),
];

/// Clipboard commands kept apart because paste has two ids.
const PASTE_COMMANDS: [&str; 2] = ["edit.paste", "edit.paste-plain"];

/// Every command id the query input runs, for binding its keys.
pub fn input_command_ids() -> impl Iterator<Item = &'static str> {
    INPUT_COMMANDS
        .iter()
        .map(|(id, _)| *id)
        .chain(PASTE_COMMANDS)
}

/// A single-line text input.
pub struct QueryInput {
    focus_handle: FocusHandle,
    text: String,
    selected: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    placeholder: SharedString,
    theme: PickerTheme,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
}

impl EventEmitter<QueryEvent> for QueryInput {}

impl Focusable for QueryInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl QueryInput {
    pub fn new(placeholder: SharedString, theme: PickerTheme, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            text: String::new(),
            selected: 0..0,
            reversed: false,
            marked: None,
            placeholder,
            theme,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn selected_range(&self) -> Range<usize> {
        self.selected.clone()
    }

    pub fn cursor(&self) -> usize {
        if self.reversed {
            self.selected.start
        } else {
            self.selected.end
        }
    }

    /// The IME composition, if one is in progress.
    pub fn marked_range(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }

    /// Replaces the whole text and puts the cursor at the end.
    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let whole = 0..self.text.len();
        self.replace(whole, text, cx);
    }

    /// Runs one of the input's commands. Returns false for ids it doesn't
    /// know.
    pub fn run_command(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        if let Some((_, handler)) = INPUT_COMMANDS.iter().find(|(known, _)| *known == id) {
            handler(self, cx);
            return true;
        }
        if PASTE_COMMANDS.contains(&id) {
            self.paste(cx);
            return true;
        }
        false
    }

    /// Replaces `range` with `text`, leaves the cursor after it and returns
    /// the inserted range. Any composition ends.
    pub fn replace(
        &mut self,
        range: Range<usize>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Range<usize> {
        let range = self.clamp(range);
        let text = text.replace(['\n', '\r'], " ");
        let changed = self.text[range.clone()] != text;
        self.text.replace_range(range.clone(), &text);
        let inserted = range.start..range.start + text.len();
        self.set_cursor(inserted.end);
        self.marked = None;
        if changed {
            cx.emit(QueryEvent::Changed);
        }
        cx.notify();
        inserted
    }

    fn clamp(&self, range: Range<usize>) -> Range<usize> {
        let end = floor_boundary(&self.text, range.end);
        let start = floor_boundary(&self.text, range.start).min(end);
        start..end
    }

    fn set_cursor(&mut self, offset: usize) {
        self.selected = offset..offset;
        self.reversed = false;
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let anchor = if self.reversed {
            self.selected.end
        } else {
            self.selected.start
        };
        self.reversed = offset < anchor;
        self.selected = anchor.min(offset)..anchor.max(offset);
        cx.notify();
    }

    fn target(&self, motion: Motion) -> usize {
        let cursor = self.cursor();
        match motion {
            Motion::Left => previous_grapheme(&self.text, cursor),
            Motion::Right => next_grapheme(&self.text, cursor),
            Motion::WordLeft => word_left(&self.text, cursor),
            Motion::WordRight => word_right(&self.text, cursor),
            Motion::Start => 0,
            Motion::End => self.text.len(),
        }
    }

    fn go(&mut self, motion: Motion, extend: bool, cx: &mut Context<Self>) {
        if extend {
            self.select_to(self.target(motion), cx);
            return;
        }
        let collapse = match motion {
            Motion::Left if !self.selected.is_empty() => Some(self.selected.start),
            Motion::Right if !self.selected.is_empty() => Some(self.selected.end),
            _ => None,
        };
        let offset = collapse.unwrap_or_else(|| self.target(motion));
        self.set_cursor(offset);
        cx.notify();
    }

    fn delete(&mut self, motion: Motion, cx: &mut Context<Self>) {
        let range = if self.selected.is_empty() {
            let (cursor, target) = (self.cursor(), self.target(motion));
            cursor.min(target)..cursor.max(target)
        } else {
            self.selected.clone()
        };
        if !range.is_empty() {
            self.replace(range, "", cx);
        }
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        self.selected = 0..self.text.len();
        self.reversed = false;
        cx.notify();
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        if !self.selected.is_empty() {
            let text = self.text[self.selected.clone()].to_string();
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn cut(&mut self, cx: &mut Context<Self>) {
        self.copy(cx);
        if !self.selected.is_empty() {
            self.replace(self.selected.clone(), "", cx);
        }
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let range = self.marked.clone().unwrap_or_else(|| self.selected.clone());
        self.replace(range, &text, cx);
    }

    fn on_run_command(&mut self, action: &RunCommand, _: &mut Window, cx: &mut Context<Self>) {
        if !self.run_command(&action.id, cx) {
            cx.propagate();
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
        let offset = self.offset_for_position(event.position);
        self.is_selecting = true;
        if event.modifiers.shift {
            self.select_to(offset, cx);
        } else {
            self.set_cursor(offset);
            cx.notify();
        }
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.offset_for_position(event.position), cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn offset_for_position(&self, position: Point<Pixels>) -> usize {
        let (Some(bounds), Some(line)) = (self.last_bounds, self.last_layout.as_ref()) else {
            return self.text.len();
        };
        if self.text.is_empty() {
            return 0;
        }
        line.closest_index_for_x(position.x - bounds.left())
            .min(self.text.len())
    }

    /// The byte range an input method edit applies to.
    fn input_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        range_utf16
            .map(|range| range_from_utf16(&self.text, &range))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selected.clone())
    }
}

fn floor_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn previous_grapheme(text: &str, offset: usize) -> usize {
    text[..offset]
        .graphemes(true)
        .next_back()
        .map_or(0, |grapheme| offset - grapheme.len())
}

fn next_grapheme(text: &str, offset: usize) -> usize {
    text[offset..]
        .graphemes(true)
        .next()
        .map_or(text.len(), |grapheme| offset + grapheme.len())
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The start of the word before `offset`, skipping separators first.
pub(crate) fn word_left(text: &str, offset: usize) -> usize {
    let mut chars = text[..offset].char_indices().rev().peekable();
    let mut position = offset;
    for want_word in [false, true] {
        while let Some((index, _)) = chars.next_if(|(_, c)| is_word_char(*c) == want_word) {
            position = index;
        }
    }
    position
}

/// The end of the word after `offset`, skipping separators first.
pub(crate) fn word_right(text: &str, offset: usize) -> usize {
    let mut chars = text[offset..].char_indices().peekable();
    let mut position = offset;
    for want_word in [false, true] {
        while let Some((index, c)) = chars.next_if(|(_, c)| is_word_char(*c) == want_word) {
            position = offset + index + c.len_utf8();
        }
    }
    position
}

fn offset_to_utf16(text: &str, offset: usize) -> usize {
    text[..floor_boundary(text, offset)]
        .chars()
        .map(char::len_utf16)
        .sum()
}

fn offset_from_utf16(text: &str, offset_utf16: usize) -> usize {
    let mut units = 0;
    for (index, c) in text.char_indices() {
        if units >= offset_utf16 {
            return index;
        }
        units += c.len_utf16();
    }
    text.len()
}

fn range_to_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
    offset_to_utf16(text, range.start)..offset_to_utf16(text, range.end)
}

fn range_from_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
    offset_from_utf16(text, range.start)..offset_from_utf16(text, range.end)
}

impl EntityInputHandler for QueryInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = range_from_utf16(&self.text, &range_utf16);
        adjusted_range.replace(range_to_utf16(&self.text, &range));
        Some(self.text[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: range_to_utf16(&self.text, &self.selected),
            reversed: self.reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked
            .as_ref()
            .map(|range| range_to_utf16(&self.text, range))
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.input_range(range_utf16);
        self.replace(range, text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.input_range(range_utf16);
        let inserted = self.replace(range, new_text, cx);
        self.marked = (!inserted.is_empty()).then(|| inserted.clone());
        let Some(selected_utf16) = new_selected_range_utf16 else {
            return;
        };
        let base = offset_to_utf16(&self.text, inserted.start);
        let selected = range_from_utf16(
            &self.text,
            &(base + selected_utf16.start..base + selected_utf16.end),
        );
        self.selected = selected.start.min(inserted.end)..selected.end.min(inserted.end);
        self.reversed = false;
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_layout.as_ref()?;
        let range = range_from_utf16(&self.text, &range_utf16);
        Some(Bounds::from_corners(
            point(bounds.left() + line.x_for_index(range.start), bounds.top()),
            point(bounds.left() + line.x_for_index(range.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let line = self.last_layout.as_ref()?;
        let index = line.index_for_x(point.x - bounds.left())?;
        Some(offset_to_utf16(&self.text, index))
    }
}

impl Render for QueryInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context(INPUT_CONTEXT)
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::on_run_command))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .w_full()
            .h(self.theme.input_line_height)
            .child(QueryElement { input: cx.entity() })
    }
}

/// Paints the query text, its selection, composition and cursor, and
/// registers the input handler.
struct QueryElement {
    input: Entity<QueryInput>,
}

struct QueryPaint {
    line: ShapedLine,
    selection: Option<PaintQuad>,
    cursor: Option<PaintQuad>,
}

impl IntoElement for QueryElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl QueryElement {
    fn text_runs(input: &QueryInput, len: usize, is_placeholder: bool) -> Vec<TextRun> {
        let theme = &input.theme;
        let color = if is_placeholder {
            theme.placeholder_text
        } else {
            theme.text
        };
        let run = TextRun {
            len,
            font: theme.font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let Some(marked) = input.marked.clone().filter(|_| !is_placeholder) else {
            return vec![run];
        };
        let underline = UnderlineStyle {
            color: Some(color),
            thickness: theme.composition_underline_thickness,
            wavy: false,
        };
        [
            (marked.start, None),
            (marked.len(), Some(underline)),
            (len - marked.end, None),
        ]
        .into_iter()
        .filter(|(len, _)| *len > 0)
        .map(|(len, underline)| TextRun {
            len,
            underline,
            ..run.clone()
        })
        .collect()
    }

    /// The selection quad, or the caret when nothing is selected.
    fn selection_and_cursor(
        theme: &PickerTheme,
        selected: Range<usize>,
        cursor: usize,
        line: &ShapedLine,
        bounds: Bounds<Pixels>,
    ) -> (Option<PaintQuad>, Option<PaintQuad>) {
        if selected.is_empty() {
            let x = bounds.left() + line.x_for_index(cursor);
            let cursor = fill(
                Bounds::new(
                    point(x, bounds.top()),
                    size(theme.cursor_width, bounds.size.height),
                ),
                theme.cursor,
            );
            return (None, Some(cursor));
        }
        let selection = fill(
            Bounds::from_corners(
                point(
                    bounds.left() + line.x_for_index(selected.start),
                    bounds.top(),
                ),
                point(
                    bounds.left() + line.x_for_index(selected.end),
                    bounds.bottom(),
                ),
            ),
            theme.selection,
        );
        (Some(selection), None)
    }
}

impl Element for QueryElement {
    type RequestLayoutState = ();
    type PrepaintState = QueryPaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let height = self.input.read(cx).theme.input_line_height;
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = height.into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let is_placeholder = input.text.is_empty();
        let shown: SharedString = if is_placeholder {
            input.placeholder.clone()
        } else {
            input.text.clone().into()
        };
        let runs = Self::text_runs(input, shown.len(), is_placeholder);
        let line = window
            .text_system()
            .shape_line(shown, input.theme.input_font_size, &runs, None);
        let (selected, cursor) = if is_placeholder {
            (0..0, 0)
        } else {
            (input.selected.clone(), input.cursor())
        };
        let (selection, cursor) =
            Self::selection_and_cursor(&input.theme, selected, cursor, &line, bounds);
        QueryPaint {
            line,
            selection,
            cursor,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (focus_handle, line_height, is_placeholder) = {
            let input = self.input.read(cx);
            (
                input.focus_handle.clone(),
                input.theme.input_line_height,
                input.text.is_empty(),
            )
        };
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        let line = prepaint.line.clone();
        if let Err(error) = line.paint(bounds.origin, line_height, window, cx) {
            log_paint_error(&error);
        }
        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }
        self.input.update(cx, |input, _| {
            input.last_layout = (!is_placeholder).then_some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

fn log_paint_error(error: &anyhow::Error) {
    eprintln!("could not paint the query: {error}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_motions_skip_separators_then_the_word() {
        let text = "daily notes/2024-05";
        assert_eq!(word_left(text, text.len()), 17);
        assert_eq!(word_left(text, 12), 6);
        assert_eq!(word_left(text, 5), 0);
        assert_eq!(word_right(text, 0), 5);
        assert_eq!(word_right(text, 5), 11);
    }

    #[test]
    fn word_motions_handle_accents() {
        let text = "café crème";
        assert_eq!(word_left(text, text.len()), 6);
        assert_eq!(word_right(text, 0), 5);
    }

    #[test]
    fn graphemes_move_over_whole_emoji_and_accents() {
        let text = "e\u{301}👍🏽";
        assert_eq!(next_grapheme(text, 0), 3);
        assert_eq!(next_grapheme(text, 3), text.len());
        assert_eq!(previous_grapheme(text, text.len()), 3);
    }

    #[test]
    fn utf16_offsets_round_trip() {
        let text = "あ😀x";
        assert_eq!(offset_to_utf16(text, 3), 1);
        assert_eq!(offset_to_utf16(text, 7), 3);
        assert_eq!(offset_from_utf16(text, 3), 7);
        assert_eq!(offset_from_utf16(text, 99), text.len());
    }
}
