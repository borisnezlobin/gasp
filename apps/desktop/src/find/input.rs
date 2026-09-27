//! A single-line text field for queries, shared by the find bar and the
//! vault search panel. It answers to the same editing commands as the
//! editor (motions, deletes, copy and paste) through key rules bound in its
//! own key context, and takes typed text and IME input through GPUI's
//! input handler.

use std::ops::Range;

use editor_config::{Platform, RuleSet};
use editor_core::document::Document;
use editor_core::motion;
use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, KeyBinding,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    ShapedLine, SharedString, Style, TextRun, UTF16Selection, UnderlineStyle, Window, div, fill,
    point, prelude::*, relative, size,
};

use crate::keymap::{RunCommand, all_bindings};
use crate::text_offsets::{range_from_utf16, range_to_utf16};
use crate::theme::FindUiTheme;

/// The key context the field sets.
pub const INPUT_CONTEXT: &str = "QueryInput";

/// What the field tells its owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryInputEvent {
    /// The text changed.
    Changed,
}

/// (command, motion, extends the selection).
type Motion = fn(&Document, usize) -> usize;

const MOTIONS: [(&str, &str, Motion); 8] = [
    ("cursor.left", "select.left", |doc, at| {
        doc.prev_char_boundary(at)
    }),
    ("cursor.right", "select.right", |doc, at| {
        doc.next_char_boundary(at)
    }),
    ("cursor.word-left", "select.word-left", motion::word_left),
    ("cursor.word-right", "select.word-right", motion::word_right),
    ("cursor.line-start", "select.line-start", |_, _| 0),
    ("cursor.line-end", "select.line-end", |doc, _| doc.len()),
    ("cursor.doc-start", "select.doc-start", |_, _| 0),
    ("cursor.doc-end", "select.doc-end", |doc, _| doc.len()),
];

type Delete = fn(&Document, usize) -> Range<usize>;

const DELETES: [(&str, Delete); 6] = [
    ("edit.delete-backward", |doc, at| {
        doc.prev_char_boundary(at)..at
    }),
    ("edit.delete-forward", |doc, at| {
        at..doc.next_char_boundary(at)
    }),
    ("edit.delete-word-backward", |doc, at| {
        motion::word_left(doc, at)..at
    }),
    ("edit.delete-word-forward", |doc, at| {
        at..motion::word_right(doc, at)
    }),
    ("edit.delete-to-line-start", |_, at| 0..at),
    ("edit.delete-to-line-end", |doc, at| at..doc.len()),
];

const CLIPBOARD_COMMANDS: [&str; 5] = [
    "select.all",
    "edit.copy",
    "edit.cut",
    "edit.paste",
    "edit.paste-plain",
];

/// Whether the field runs command `id`.
pub fn handles(id: &str) -> bool {
    MOTIONS
        .iter()
        .any(|(move_id, select_id, _)| *move_id == id || *select_id == id)
        || DELETES.iter().any(|(name, _)| *name == id)
        || CLIPBOARD_COMMANDS.contains(&id)
}

/// Binds the key rules the field runs in its own key context.
pub fn bind_input_keys(rules: &RuleSet, cx: &mut App) {
    let bindings = all_bindings(rules, Platform::current())
        .into_iter()
        .filter(|binding| handles(&binding.command))
        .map(|binding| {
            let action = RunCommand {
                id: binding.command.into(),
            };
            KeyBinding::new(&binding.keystroke, action, Some(INPUT_CONTEXT))
        });
    cx.bind_keys(bindings);
}

/// A one-line text field.
pub struct QueryInput {
    focus_handle: FocusHandle,
    text: String,
    placeholder: SharedString,
    selected: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    invalid: bool,
    theme: FindUiTheme,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
}

impl EventEmitter<QueryInputEvent> for QueryInput {}

impl Focusable for QueryInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl QueryInput {
    pub fn new(
        placeholder: impl Into<SharedString>,
        theme: FindUiTheme,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            text: String::new(),
            placeholder: placeholder.into(),
            selected: 0..0,
            reversed: false,
            marked: None,
            invalid: false,
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

    /// Replaces the text and selects all of it, so typing replaces it.
    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let text = single_line(text);
        if text != self.text {
            self.text = text;
            cx.emit(QueryInputEvent::Changed);
        }
        self.marked = None;
        self.select_all(cx);
    }

    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        self.selected = 0..self.text.len();
        self.reversed = false;
        cx.notify();
    }

    /// Tints the field when its text isn't a valid query.
    pub fn set_invalid(&mut self, invalid: bool, cx: &mut Context<Self>) {
        if self.invalid != invalid {
            self.invalid = invalid;
            cx.notify();
        }
    }

    pub fn is_invalid(&self) -> bool {
        self.invalid
    }

    fn doc(&self) -> Document {
        Document::from(self.text.as_str())
    }

    fn cursor(&self) -> usize {
        if self.reversed {
            self.selected.start
        } else {
            self.selected.end
        }
    }

    fn anchor(&self) -> usize {
        if self.reversed {
            self.selected.end
        } else {
            self.selected.start
        }
    }

    fn select(&mut self, anchor: usize, head: usize, cx: &mut Context<Self>) {
        self.reversed = head < anchor;
        self.selected = anchor.min(head)..anchor.max(head);
        cx.notify();
    }

    /// Replaces `range` and puts the caret after the new text.
    fn splice(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) -> Range<usize> {
        let text = single_line(text);
        self.text.replace_range(range.clone(), &text);
        let inserted = range.start..range.start + text.len();
        self.select(inserted.end, inserted.end, cx);
        self.marked = None;
        cx.emit(QueryInputEvent::Changed);
        inserted
    }

    fn on_run_command(&mut self, action: &RunCommand, _: &mut Window, cx: &mut Context<Self>) {
        if !self.run_command(&action.id, cx) {
            cx.propagate();
        }
    }

    /// Runs a command by id. Returns false when the field doesn't know it.
    pub fn run_command(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        if let Some((move_id, _, motion)) = MOTIONS
            .iter()
            .find(|(move_id, select_id, _)| *move_id == id || *select_id == id)
        {
            self.apply_motion(*motion, *move_id != id, cx);
            return true;
        }
        if let Some((_, around)) = DELETES.iter().find(|(name, _)| *name == id) {
            self.delete_or(*around, cx);
            return true;
        }
        self.run_clipboard_command(id, cx)
    }

    fn run_clipboard_command(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        match id {
            "select.all" => self.select_all(cx),
            "edit.copy" => self.copy(cx),
            "edit.cut" => {
                self.copy(cx);
                self.splice(self.selected.clone(), "", cx);
            }
            "edit.paste" | "edit.paste-plain" => self.paste(cx),
            _ => return false,
        }
        true
    }

    fn apply_motion(&mut self, motion: Motion, extend: bool, cx: &mut Context<Self>) {
        let target = motion(&self.doc(), self.cursor());
        // Moving without extending collapses a selection towards the motion.
        let head = if extend || self.selected.is_empty() {
            target
        } else if target < self.cursor() {
            self.selected.start.min(target)
        } else {
            self.selected.end.max(target)
        };
        let anchor = if extend { self.anchor() } else { head };
        self.select(anchor, head, cx);
    }

    fn delete_or(&mut self, around: Delete, cx: &mut Context<Self>) {
        let mut range = self.selected.clone();
        if range.is_empty() {
            range = around(&self.doc(), range.start);
        }
        if !range.is_empty() {
            self.splice(range, "", cx);
        }
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        if !self.selected.is_empty() {
            let text = self.text[self.selected.clone()].to_owned();
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.splice(self.selected.clone(), &text, cx);
        }
    }

    fn index_for_position(&self, position: Point<Pixels>) -> usize {
        let (Some(bounds), Some(line)) = (self.last_bounds, self.last_layout.as_ref()) else {
            return 0;
        };
        if self.text.is_empty() {
            return 0;
        }
        line.closest_index_for_x(position.x - bounds.left())
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
        self.is_selecting = true;
        let offset = self.index_for_position(event.position);
        let anchor = if event.modifiers.shift {
            self.anchor()
        } else {
            offset
        };
        self.select(anchor, offset, cx);
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            let offset = self.index_for_position(event.position);
            self.select(self.anchor(), offset, cx);
        }
    }

    fn input_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        range_utf16
            .map(|range| range_from_utf16(&self.doc(), &range))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selected.clone())
    }
}

/// Query fields hold one line: line breaks become spaces.
fn single_line(text: &str) -> String {
    text.replace("\r\n", " ").replace(['\n', '\r'], " ")
}

impl EntityInputHandler for QueryInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let doc = self.doc();
        let range = range_from_utf16(&doc, &range_utf16);
        adjusted_range.replace(range_to_utf16(&doc, &range));
        Some(self.text[range].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: range_to_utf16(&self.doc(), &self.selected),
            reversed: self.reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked
            .as_ref()
            .map(|range| range_to_utf16(&self.doc(), range))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.input_range(range_utf16);
        self.splice(range, text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.input_range(range_utf16);
        let inserted = self.splice(range, new_text, cx);
        self.marked = (!inserted.is_empty()).then(|| inserted.clone());
        if let Some(selected_utf16) = new_selected_range_utf16 {
            let doc = self.doc();
            let base = range_to_utf16(&doc, &(inserted.start..inserted.start)).start;
            let selected = range_from_utf16(
                &doc,
                &(base + selected_utf16.start..base + selected_utf16.end),
            );
            self.select(selected.start, selected.end.min(inserted.end), cx);
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_layout.as_ref()?;
        let range = range_from_utf16(&self.doc(), &range_utf16);
        Some(Bounds::from_corners(
            point(bounds.left() + line.x_for_index(range.start), bounds.top()),
            point(bounds.left() + line.x_for_index(range.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let offset = self.index_for_position(point);
        Some(range_to_utf16(&self.doc(), &(offset..offset)).start)
    }
}

impl Render for QueryInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let focused = self.focus_handle.is_focused(window);
        let background = if self.invalid {
            theme.input_error_background
        } else {
            theme.input_background
        };
        let ring = if focused {
            theme.input_focus_ring
        } else {
            gpui::transparent_black()
        };
        div()
            .key_context(INPUT_CONTEXT)
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::on_run_command))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .flex()
            .items_center()
            .flex_1()
            .min_w_0()
            .h(theme.input_height)
            .px(theme.input_padding_x)
            .rounded(theme.radius)
            .bg(background)
            .overflow_hidden()
            .shadow(vec![gpui::BoxShadow {
                color: ring,
                offset: point(gpui::px(0.), gpui::px(0.)),
                blur_radius: gpui::px(0.),
                spread_radius: theme.input_ring_width,
            }])
            .font_family(theme.font_family)
            .text_size(theme.font_size)
            .text_color(theme.text)
            .child(QueryLine { input: cx.entity() })
    }
}

/// Paints the field's text, selection and caret, and registers the input
/// handler.
struct QueryLine {
    input: Entity<QueryInput>,
}

struct QueryLinePaint {
    line: ShapedLine,
    selection: Option<PaintQuad>,
    caret: Option<PaintQuad>,
}

impl IntoElement for QueryLine {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl QueryLine {
    fn runs(input: &QueryInput, len: usize, run: TextRun) -> Vec<TextRun> {
        let Some(marked) = input.marked.clone() else {
            return vec![run];
        };
        let underline = UnderlineStyle {
            color: Some(run.color),
            thickness: input.theme.caret_width,
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
}

impl Element for QueryLine {
    type RequestLayoutState = ();
    type PrepaintState = QueryLinePaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> QueryLinePaint {
        let input = self.input.read(cx);
        let theme = &input.theme;
        let style = window.text_style();
        let (display, color) = if input.text.is_empty() {
            (input.placeholder.clone(), theme.placeholder)
        } else {
            (SharedString::from(input.text.clone()), style.color)
        };
        let run = TextRun {
            len: display.len(),
            font: style.font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if input.text.is_empty() {
            vec![run]
        } else {
            QueryLine::runs(input, display.len(), run)
        };
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display, font_size, &runs, None);
        let selected = input.selected.clone();
        let caret_x = line.x_for_index(input.cursor());
        let selection = (!selected.is_empty()).then(|| {
            fill(
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
                theme.input_selection,
            )
        });
        let caret = selected.is_empty().then(|| {
            fill(
                Bounds::new(
                    point(bounds.left() + caret_x, bounds.top()),
                    size(theme.caret_width, bounds.size.height),
                ),
                theme.caret,
            )
        });
        QueryLinePaint {
            line,
            selection,
            caret,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut QueryLinePaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        let line_height = window.line_height();
        prepaint
            .line
            .paint(bounds.origin, line_height, window, cx)
            .ok();
        if focus_handle.is_focused(window)
            && let Some(caret) = prepaint.caret.take()
        {
            window.paint_quad(caret);
        }
        let line = prepaint.line.clone();
        self.input.update(cx, |input, _| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_breaks_become_spaces() {
        assert_eq!(single_line("a\nb\r\nc"), "a b c");
    }

    #[test]
    fn editing_commands_are_handled() {
        assert!(handles("edit.paste"));
        assert!(handles("select.word-left"));
        assert!(!handles("edit.newline"));
        assert!(!handles("cursor.up"));
    }
}
