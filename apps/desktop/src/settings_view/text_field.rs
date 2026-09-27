//! A single-line text field with IME support, for inline renames, the
//! settings search box and text settings.

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId,
    InspectorElementId, KeyDownEvent, LayoutId, MouseButton, MouseDownEvent, Pixels, Point,
    ShapedLine, SharedString, Style, TextRun, UTF16Selection, UnderlineStyle, Window, div, fill,
    point, prelude::*, relative, size,
};

use crate::theme::PanelTheme;

/// What a text field tells its owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextFieldEvent {
    /// The text changed.
    Changed,
    /// Enter was pressed.
    Submitted,
    /// Escape was pressed.
    Cancelled,
}

/// A one-line text input. The owner decides what Enter and Escape mean.
pub struct TextField {
    focus_handle: FocusHandle,
    text: String,
    selected: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    placeholder: SharedString,
    theme: PanelTheme,
    last_line: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
}

impl EventEmitter<TextFieldEvent> for TextField {}

impl Focusable for TextField {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl TextField {
    pub fn new(cx: &mut Context<Self>) -> TextField {
        TextField {
            focus_handle: cx.focus_handle(),
            text: String::new(),
            selected: 0..0,
            reversed: false,
            marked: None,
            placeholder: SharedString::default(),
            theme: PanelTheme::default(),
            last_line: None,
            last_bounds: None,
        }
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<SharedString>) -> TextField {
        self.placeholder = placeholder.into();
        self
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn selected_range(&self) -> Range<usize> {
        self.selected.clone()
    }

    /// Replaces the text and puts the cursor at the end. Doesn't emit.
    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.text = text.replace(['\n', '\r'], " ");
        self.selected = self.text.len()..self.text.len();
        self.marked = None;
        cx.notify();
    }

    /// Selects a byte range, clamped to character boundaries.
    pub fn select(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        let start = self.floor(range.start);
        let end = self.floor(range.end.max(range.start));
        self.selected = start..end;
        self.reversed = false;
        cx.notify();
    }

    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        self.select(0..self.text.len(), cx);
    }

    fn floor(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    fn cursor(&self) -> usize {
        if self.reversed {
            self.selected.start
        } else {
            self.selected.end
        }
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.text[..offset]
            .char_indices()
            .next_back()
            .map_or(0, |(at, _)| at)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.text[offset..]
            .chars()
            .next()
            .map_or(offset, |ch| offset + ch.len_utf8())
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected = offset..offset;
        self.reversed = false;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.reversed {
            self.selected.start = offset;
        } else {
            self.selected.end = offset;
        }
        if self.selected.end < self.selected.start {
            self.reversed = !self.reversed;
            self.selected = self.selected.end..self.selected.start;
        }
        cx.notify();
    }

    /// Replaces `range` with `text` and emits [`TextFieldEvent::Changed`].
    fn replace(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        let text = text.replace(['\n', '\r'], " ");
        self.text.replace_range(range.clone(), &text);
        let end = range.start + text.len();
        self.selected = end..end;
        self.reversed = false;
        self.marked = None;
        cx.emit(TextFieldEvent::Changed);
        cx.notify();
    }

    fn delete_or(&mut self, target: usize, cx: &mut Context<Self>) {
        let range = if self.selected.is_empty() {
            target.min(self.cursor())..target.max(self.cursor())
        } else {
            self.selected.clone()
        };
        if !range.is_empty() {
            self.replace(range, "", cx);
        }
    }

    fn horizontal(&mut self, target: usize, extend: bool, cx: &mut Context<Self>) {
        if extend {
            self.select_to(target, cx);
        } else {
            self.move_to(target, cx);
        }
    }

    fn copy(&self, cx: &mut Context<Self>) {
        if !self.selected.is_empty() {
            let text = self.text[self.selected.clone()].to_string();
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace(self.selected.clone(), &text, cx);
        }
    }

    /// Handles a key. Returns false for keys the field leaves to its owner
    /// or to the input handler, such as Tab and printable characters.
    fn handle_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        if modifiers.secondary() && !modifiers.alt {
            return self.handle_shortcut(&keystroke.key, modifiers.shift, cx);
        }
        let extend = modifiers.shift;
        if let Some(target) = self.motion_target(&keystroke.key, extend) {
            self.horizontal(target, extend, cx);
            return true;
        }
        match keystroke.key.as_str() {
            "backspace" => self.delete_or(self.previous_boundary(self.cursor()), cx),
            "delete" => self.delete_or(self.next_boundary(self.cursor()), cx),
            "enter" => cx.emit(TextFieldEvent::Submitted),
            "escape" => cx.emit(TextFieldEvent::Cancelled),
            _ => return false,
        }
        true
    }

    /// Where an arrow, Home or End key moves the cursor. Without Shift,
    /// left and right first collapse a selection to its edge.
    fn motion_target(&self, key: &str, extend: bool) -> Option<usize> {
        let collapse = !extend && !self.selected.is_empty();
        match key {
            "left" if collapse => Some(self.selected.start),
            "right" if collapse => Some(self.selected.end),
            "left" => Some(self.previous_boundary(self.cursor())),
            "right" => Some(self.next_boundary(self.cursor())),
            "home" => Some(0),
            "end" => Some(self.text.len()),
            _ => None,
        }
    }

    fn handle_shortcut(&mut self, key: &str, shift: bool, cx: &mut Context<Self>) -> bool {
        match key {
            "a" => self.select_all(cx),
            "c" => self.copy(cx),
            "x" => {
                self.copy(cx);
                self.delete_or(self.cursor(), cx);
            }
            "v" => self.paste(cx),
            "left" => self.horizontal(0, shift, cx),
            "right" => self.horizontal(self.text.len(), shift, cx),
            "backspace" => self.delete_or(0, cx),
            _ => return false,
        }
        true
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.handle_key(event, cx) {
            cx.stop_propagation();
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
        let offset = self.offset_for_point(event.position);
        if event.modifiers.shift {
            self.select_to(offset, cx);
        } else {
            self.move_to(offset, cx);
        }
    }

    fn offset_for_point(&self, position: Point<Pixels>) -> usize {
        match (&self.last_bounds, &self.last_line) {
            (Some(bounds), Some(line)) => line.closest_index_for_x(position.x - bounds.left()),
            _ => self.text.len(),
        }
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf16 = 0;
        for (at, ch) in self.text.char_indices() {
            if utf16 >= offset {
                return at;
            }
            utf16 += ch.len_utf16();
        }
        self.text.len()
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        self.text[..offset.min(self.text.len())]
            .chars()
            .map(char::len_utf16)
            .sum()
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn input_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        range_utf16
            .map(|range| self.range_from_utf16(&range))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selected.clone())
    }
}

impl EntityInputHandler for TextField {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        adjusted.replace(self.range_to_utf16(&range));
        Some(self.text[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected),
            reversed: self.reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.input_range(range_utf16);
        self.replace(range, text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        selected_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.input_range(range_utf16);
        self.replace(range.clone(), text, cx);
        let inserted = range.start..range.start + text.len();
        self.marked = (!inserted.is_empty()).then(|| inserted.clone());
        if let Some(selected) = selected_utf16 {
            let base = self.offset_to_utf16(inserted.start);
            let selected = self.range_from_utf16(&(base + selected.start..base + selected.end));
            self.selected = selected.start.min(inserted.end)..selected.end.min(inserted.end);
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_line.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(bounds.left() + line.x_for_index(range.start), bounds.top()),
            point(bounds.left() + line.x_for_index(range.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let offset = self.offset_for_point(position);
        Some(self.offset_to_utf16(offset))
    }
}

impl Render for TextField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        div()
            .key_context("TextField")
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .w_full()
            .h(theme.input_height)
            .px(theme.padding_x)
            .flex()
            .items_center()
            .rounded(theme.radius)
            .bg(theme.input_background)
            .overflow_hidden()
            .child(TextFieldElement { field: cx.entity() })
    }
}

/// Draws a text field's line, selection and caret, and registers it for
/// platform text input while it has focus.
struct TextFieldElement {
    field: Entity<TextField>,
}

struct TextFieldPaint {
    line: ShapedLine,
    selection: Option<Bounds<Pixels>>,
    caret: Option<Bounds<Pixels>>,
}

impl IntoElement for TextFieldElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextFieldElement {
    type RequestLayoutState = ();
    type PrepaintState = TextFieldPaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let field = self.field.read(cx);
        let style = window.text_style();
        let showing_placeholder = field.text.is_empty();
        let (text, color): (SharedString, _) = if showing_placeholder {
            (field.placeholder.clone(), field.theme.muted_text)
        } else {
            (field.text.clone().into(), style.color)
        };
        let base = TextRun {
            len: text.len(),
            font: style.font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = match (&field.marked, showing_placeholder) {
            (Some(marked), false) => marked_runs(
                &base,
                marked,
                text.len(),
                field.theme.composition_underline_thickness,
            ),
            _ => vec![base],
        };
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(text, font_size, &runs, None);
        let x = |offset: usize| {
            if showing_placeholder {
                Pixels::ZERO
            } else {
                line.x_for_index(offset)
            }
        };
        let selected = field.selected.clone();
        let (selection, caret) = if selected.is_empty() {
            let caret = Bounds::new(
                point(bounds.left() + x(field.cursor()), bounds.top()),
                size(field.theme.caret_width, bounds.size.height),
            );
            (None, Some(caret))
        } else {
            let selection = Bounds::from_corners(
                point(bounds.left() + x(selected.start), bounds.top()),
                point(bounds.left() + x(selected.end), bounds.bottom()),
            );
            (Some(selection), None)
        };
        TextFieldPaint {
            line,
            selection,
            caret,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (focus_handle, theme) = {
            let field = self.field.read(cx);
            (field.focus_handle.clone(), field.theme.clone())
        };
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.field.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection {
            window.paint_quad(fill(selection, theme.text_selection));
        }
        let line_height = window.line_height();
        prepaint
            .line
            .paint(bounds.origin, line_height, window, cx)
            .ok();
        if focus_handle.is_focused(window)
            && let Some(caret) = prepaint.caret
        {
            window.paint_quad(fill(caret, theme.caret));
        }
        let line = prepaint.line.clone();
        self.field.update(cx, |field, _| {
            field.last_line = (!field.text.is_empty()).then_some(line);
            field.last_bounds = Some(bounds);
        });
    }
}

/// Runs that underline an IME composition.
fn marked_runs(
    base: &TextRun,
    marked: &Range<usize>,
    len: usize,
    thickness: Pixels,
) -> Vec<TextRun> {
    let underline = UnderlineStyle {
        color: Some(base.color),
        thickness,
        wavy: false,
    };
    [
        (marked.start, None),
        (marked.end - marked.start, Some(underline)),
        (len.saturating_sub(marked.end), None),
    ]
    .into_iter()
    .filter(|(run_len, _)| *run_len > 0)
    .map(|(run_len, underline)| TextRun {
        len: run_len,
        underline,
        ..base.clone()
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, VisualTestContext};

    use super::*;

    fn field(cx: &mut TestAppContext) -> (Entity<TextField>, &mut VisualTestContext) {
        let (field, cx) = cx.add_window_view(|_, cx| TextField::new(cx));
        cx.update(|window, cx| window.focus(&field.focus_handle(cx)));
        cx.run_until_parked();
        (field, cx)
    }

    fn text(field: &Entity<TextField>, cx: &mut VisualTestContext) -> String {
        field.read_with(cx, |field, _| field.text().to_string())
    }

    #[gpui::test]
    fn typing_and_editing_keys(cx: &mut TestAppContext) {
        let (field, cx) = field(cx);
        cx.simulate_input("héllo");
        assert_eq!(text(&field, cx), "héllo");
        cx.simulate_keystrokes("left left backspace");
        assert_eq!(text(&field, cx), "hélo");
        cx.simulate_keystrokes("home delete");
        assert_eq!(text(&field, cx), "élo");
        cx.simulate_keystrokes("shift-end");
        cx.simulate_input("x");
        assert_eq!(text(&field, cx), "x");
    }

    #[gpui::test]
    fn select_all_then_type_replaces(cx: &mut TestAppContext) {
        let (field, cx) = field(cx);
        field.update(cx, |field, cx| field.set_text("old name", cx));
        cx.simulate_keystrokes("secondary-a");
        cx.simulate_input("new");
        assert_eq!(text(&field, cx), "new");
    }

    #[gpui::test]
    fn enter_and_escape_are_reported(cx: &mut TestAppContext) {
        let (field, cx) = field(cx);
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&field, move |_, event: &TextFieldEvent, _| {
                seen.borrow_mut().push(event.clone());
            })
            .detach();
        });
        cx.simulate_input("a");
        cx.simulate_keystrokes("enter escape");
        assert_eq!(
            *events.borrow(),
            [
                TextFieldEvent::Changed,
                TextFieldEvent::Submitted,
                TextFieldEvent::Cancelled
            ]
        );
    }

    #[gpui::test]
    fn ime_composition_marks_then_commits(cx: &mut TestAppContext) {
        let (field, cx) = field(cx);
        field.update_in(cx, |field, window, cx| {
            field.replace_and_mark_text_in_range(None, "ni", None, window, cx);
        });
        let marked = field.update_in(cx, |field, window, cx| field.marked_text_range(window, cx));
        assert_eq!(marked, Some(0..2));
        field.update_in(cx, |field, window, cx| {
            field.replace_text_in_range(None, "你", window, cx);
        });
        assert_eq!(text(&field, cx), "你");
    }
}
