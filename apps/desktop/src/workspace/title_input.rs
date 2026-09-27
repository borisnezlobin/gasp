//! The inline title above a note's text: the file name as an editable
//! heading. Committing a new title renames the file.
//!
//! It sets the editor's key context, so the same key rules move the cursor
//! and delete text here as in the note. Commands it doesn't run bubble up.

use std::ops::Range;

use editor_core::document::Document;
use editor_core::motion;
use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId,
    KeyDownEvent, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, PaintQuad, Pixels, Point,
    ShapedLine, SharedString, Style, Subscription, TextRun, UTF16Selection, UnderlineStyle, Window,
    div, fill, point, prelude::*, relative, size,
};

use crate::keymap::{KEY_CONTEXT, RunCommand};
use crate::text_offsets::{offset_to_utf16, range_from_utf16, range_to_utf16};
use crate::theme::Theme;

/// What the title tells the workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TitleEvent {
    /// The title was edited and confirmed; rename the note to it.
    Committed(String),
    /// Focus should move to the note's text.
    Done,
}

/// A one-line heading input.
pub struct TitleInput {
    focus_handle: FocusHandle,
    text: Document,
    /// The title as of the last commit, for Escape to go back to.
    committed: String,
    anchor: usize,
    head: usize,
    marked: Option<Range<usize>>,
    is_selecting: bool,
    last_line: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    theme: Theme,
    _blur: Subscription,
}

type Handler = fn(&mut TitleInput, &mut Window, &mut Context<TitleInput>);

type MotionFn = fn(&Document, usize) -> usize;

/// (move command, select command, where it goes).
const MOTIONS: [(&str, &str, MotionFn); 8] = [
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

const HANDLERS: [(&str, Handler); 15] = [
    ("edit.delete-backward", |title, _, cx| {
        title.delete_or(|doc, at| doc.prev_char_boundary(at)..at, cx)
    }),
    ("edit.delete-forward", |title, _, cx| {
        title.delete_or(|doc, at| at..doc.next_char_boundary(at), cx)
    }),
    ("edit.delete-word-backward", |title, _, cx| {
        title.delete_or(|doc, at| motion::word_left(doc, at)..at, cx)
    }),
    ("edit.delete-word-forward", |title, _, cx| {
        title.delete_or(|doc, at| at..motion::word_right(doc, at), cx)
    }),
    ("edit.delete-to-line-start", |title, _, cx| {
        title.delete_or(|_, at| 0..at, cx)
    }),
    ("edit.delete-to-line-end", |title, _, cx| {
        title.delete_or(|doc, at| at..doc.len(), cx)
    }),
    ("select.all", |title, _, cx| title.select_all(cx)),
    ("edit.copy", |title, _, cx| title.copy(cx)),
    ("edit.cut", |title, _, cx| title.cut(cx)),
    ("edit.paste", |title, _, cx| title.paste(cx)),
    ("edit.paste-plain", |title, _, cx| title.paste(cx)),
    ("edit.undo", |title, _, cx| title.revert(cx)),
    ("edit.newline", |title, _, cx| title.finish(cx)),
    ("edit.indent", |title, _, cx| title.finish(cx)),
    ("cursor.down", |title, _, cx| title.finish(cx)),
];

impl EventEmitter<TitleEvent> for TitleInput {}

impl Focusable for TitleInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl TitleInput {
    pub fn new(title: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let blur = cx.on_blur(&focus_handle, window, |title, _, cx| title.commit(cx));
        TitleInput {
            focus_handle,
            text: Document::from(title),
            committed: title.to_owned(),
            anchor: 0,
            head: 0,
            marked: None,
            is_selecting: false,
            last_line: None,
            last_bounds: None,
            theme: Theme::default(),
            _blur: blur,
        }
    }

    pub fn text(&self) -> String {
        self.text.to_string()
    }

    pub fn selected_range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    /// Shows `title`, as after a rename, unless it's being edited.
    pub fn set_title(&mut self, title: &str, cx: &mut Context<Self>) {
        if self.text() != self.committed {
            self.committed = title.to_owned();
            return;
        }
        self.text = Document::from(title);
        self.committed = title.to_owned();
        self.anchor = self.anchor.min(self.text.len());
        self.head = self.head.min(self.text.len());
        cx.notify();
    }

    /// Selects the whole title, ready to type over.
    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        self.anchor = 0;
        self.head = self.text.len();
        cx.notify();
    }

    fn select(&mut self, anchor: usize, head: usize, cx: &mut Context<Self>) {
        self.anchor = self.text.floor_char_boundary(anchor.min(self.text.len()));
        self.head = self.text.floor_char_boundary(head.min(self.text.len()));
        cx.notify();
    }

    fn replace(&mut self, range: Range<usize>, inserted: &str, cx: &mut Context<Self>) {
        let inserted: String = inserted.chars().filter(|ch| !ch.is_control()).collect();
        let text = self.text();
        let range = range.start.min(text.len())..range.end.min(text.len());
        let joined = format!("{}{inserted}{}", &text[..range.start], &text[range.end..]);
        self.text = Document::from(joined.as_str());
        let cursor = range.start + inserted.len();
        self.marked = None;
        self.select(cursor, cursor, cx);
    }

    fn delete_or(
        &mut self,
        around: impl FnOnce(&Document, usize) -> Range<usize>,
        cx: &mut Context<Self>,
    ) {
        let mut range = self.selected_range();
        if range.is_empty() {
            range = around(&self.text, self.head);
        }
        if !range.is_empty() {
            self.replace(range, "", cx);
        }
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        let range = self.selected_range();
        if !range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.text.slice(range)));
        }
    }

    fn cut(&mut self, cx: &mut Context<Self>) {
        self.copy(cx);
        self.delete_or(|_, at| at..at, cx);
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let line = text.lines().next().unwrap_or_default().to_owned();
            self.replace(self.selected_range(), &line, cx);
        }
    }

    /// Goes back to the last committed title.
    fn revert(&mut self, cx: &mut Context<Self>) {
        self.text = Document::from(self.committed.as_str());
        self.select(self.text.len(), self.text.len(), cx);
    }

    /// Reports the title if it changed.
    pub fn commit(&mut self, cx: &mut Context<Self>) {
        let text = self.text();
        if text != self.committed {
            self.committed = text.clone();
            cx.emit(TitleEvent::Committed(text));
        }
    }

    fn finish(&mut self, cx: &mut Context<Self>) {
        self.commit(cx);
        cx.emit(TitleEvent::Done);
    }

    fn on_run_command(&mut self, action: &RunCommand, window: &mut Window, cx: &mut Context<Self>) {
        let id = action.id.as_ref();
        if let Some((move_id, _, target)) = MOTIONS
            .iter()
            .find(|(move_id, select_id, _)| *move_id == id || *select_id == id)
        {
            let head = target(&self.text, self.head);
            let anchor = if *move_id == id { head } else { self.anchor };
            return self.select(anchor, head, cx);
        }
        match HANDLERS.iter().find(|(name, _)| *name == id) {
            Some((_, handler)) => handler(self, window, cx),
            None => cx.propagate(),
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            self.revert(cx);
            cx.emit(TitleEvent::Done);
            cx.stop_propagation();
        }
    }

    fn offset_for_position(&self, position: Point<Pixels>) -> usize {
        let (Some(bounds), Some(line)) = (self.last_bounds, self.last_line.as_ref()) else {
            return self.text.len();
        };
        line.closest_index_for_x(position.x - bounds.left())
            .min(self.text.len())
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
        if event.click_count >= 2 {
            return self.select_all(cx);
        }
        let anchor = if event.modifiers.shift {
            self.anchor
        } else {
            offset
        };
        self.select(anchor, offset, cx);
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting && event.dragging() {
            let offset = self.offset_for_position(event.position);
            self.select(self.anchor, offset, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &gpui::MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }
}

impl EntityInputHandler for TitleInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = range_from_utf16(&self.text, &range_utf16);
        adjusted.replace(range_to_utf16(&self.text, &range));
        Some(self.text.slice(range))
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: range_to_utf16(&self.text, &self.selected_range()),
            reversed: self.head < self.anchor,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked
            .as_ref()
            .map(|range| range_to_utf16(&self.text, range))
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
        if !text.is_empty() {
            self.marked = Some(range.start..range.start + text.len());
        }
        if let Some(selected) = selected_utf16 {
            let start = range.start + selected.start.min(text.len());
            let end = range.start + selected.end.min(text.len());
            self.select(start, end, cx);
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
        let range = range_from_utf16(&self.text, &range_utf16);
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
        let offset = self.offset_for_position(position);
        Some(offset_to_utf16(&self.text, offset))
    }
}

impl TitleInput {
    fn input_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        range_utf16
            .map(|range| range_from_utf16(&self.text, &range))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selected_range())
    }

    #[cfg(test)]
    fn head_utf16(&self) -> usize {
        offset_to_utf16(&self.text, self.head)
    }
}

impl Render for TitleInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme.workspace;
        div()
            .id("inline-title")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .w_full()
            .px(self.theme.text_padding)
            .pt(theme.space_xxl)
            .on_action(cx.listener(Self::on_run_command))
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .child(TitleElement { input: cx.entity() })
    }
}

/// Draws the title's text, selection and cursor, and takes IME input.
struct TitleElement {
    input: Entity<TitleInput>,
}

struct TitlePrepaint {
    line: ShapedLine,
    selection: Option<PaintQuad>,
    cursor: Option<PaintQuad>,
}

impl IntoElement for TitleElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for TitleElement {
    type RequestLayoutState = ();
    type PrepaintState = TitlePrepaint;

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
        let theme = &self.input.read(cx).theme;
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = theme.line_height(theme.workspace.title_font_size).into();
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
    ) -> TitlePrepaint {
        let input = self.input.read(cx);
        let theme = &input.theme;
        let text = input.text();
        let (shown, color): (SharedString, _) = if text.is_empty() {
            (super::files::UNTITLED.into(), theme.workspace.text_faint)
        } else {
            (text.into(), theme.heading_text)
        };
        let runs = title_runs(shown.len(), input.marked.clone(), theme, color);
        let line =
            window
                .text_system()
                .shape_line(shown, theme.workspace.title_font_size, &runs, None);
        let selection = input.selected_range();
        let x = |offset: usize| bounds.left() + line.x_for_index(offset.min(input.text.len()));
        let cursor = selection.is_empty().then(|| {
            fill(
                Bounds::new(
                    point(x(input.head), bounds.top()),
                    size(theme.cursor_width, bounds.size.height),
                ),
                theme.cursor,
            )
        });
        let selection = (!selection.is_empty()).then(|| {
            fill(
                Bounds::from_corners(
                    point(x(selection.start), bounds.top()),
                    point(x(selection.end), bounds.bottom()),
                ),
                theme.selection,
            )
        });
        TitlePrepaint {
            line,
            selection,
            cursor,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut TitlePrepaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        let focused = focus_handle.is_focused(window);
        if focused && let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        let line_height = bounds.size.height;
        prepaint
            .line
            .paint(bounds.origin, line_height, window, cx)
            .ok();
        if focused && let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }
        let line = prepaint.line.clone();
        self.input.update(cx, |input, _| {
            input.last_line = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

fn title_runs(
    len: usize,
    marked: Option<Range<usize>>,
    theme: &Theme,
    color: gpui::Hsla,
) -> Vec<TextRun> {
    let run = TextRun {
        len,
        font: theme.heading_font(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let Some(marked) = marked.filter(|marked| marked.end <= len) else {
        return vec![run];
    };
    let underline = UnderlineStyle {
        color: Some(theme.composition_underline),
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

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;

    use super::*;

    #[gpui::test]
    fn commands_edit_and_commit_the_title(cx: &mut TestAppContext) {
        let (title, cx) = cx.add_window_view(|window, cx| TitleInput::new("Plans", window, cx));
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&title, move |_, event: &TitleEvent, _| {
                seen.borrow_mut().push(event.clone())
            })
            .detach()
        });
        cx.update(|window, cx| {
            title.update(cx, |title, cx| {
                title.select(5, 5, cx);
                title.replace_text_in_range(None, " 2", window, cx);
                title.on_run_command(
                    &RunCommand {
                        id: "cursor.left".into(),
                    },
                    window,
                    cx,
                );
                title.on_run_command(
                    &RunCommand {
                        id: "edit.delete-backward".into(),
                    },
                    window,
                    cx,
                );
                title.on_run_command(
                    &RunCommand {
                        id: "edit.newline".into(),
                    },
                    window,
                    cx,
                );
            })
        });
        cx.run_until_parked();
        assert_eq!(title.read_with(cx, |title, _| title.text()), "Plans2");
        assert_eq!(title.read_with(cx, |title, _| title.head_utf16()), 5);
        assert_eq!(
            *events.borrow(),
            vec![TitleEvent::Committed("Plans2".into()), TitleEvent::Done]
        );
    }
}
