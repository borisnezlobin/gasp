//! The one single-line text input every view uses: the picker query, the
//! find and vault search fields, settings fields, the file tree's inline
//! rename and the note's title.
//!
//! It types and composes through GPUI's input handler (IME included),
//! selects with the mouse and keyboard, and runs the editor's editing
//! commands (`cursor.*`, `select.*`, `edit.delete-*`, clipboard, undo)
//! from the key rules, bound in [`TEXT_INPUT_CONTEXT`] by
//! `keymap::bind_rules`. Commands it doesn't run bubble up.
//!
//! Owners create one with [`TextInput::new`], pick a [`TextInputStyle`]
//! and listen for [`TextInputEvent`]s.

mod commands;
mod element;
mod ime;
mod state;

use std::ops::Range;

use gpui::{
    App, ClipboardItem, Context, CursorStyle, EventEmitter, FocusHandle, Focusable, Font,
    FontWeight, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    ShapedLine, SharedString, Subscription, Window, actions, div, font, prelude::*,
};

pub use commands::{InputCommand, bind_keys, handles, input_bindings, input_command};
pub use state::{EditKind, LineState, Motion, single_line};

use crate::keymap::RunCommand;
use crate::theme::InputTheme;
use element::TextLine;

/// The key context every input sets.
pub const TEXT_INPUT_CONTEXT: &str = "TextInput";

actions!(text_input, [Submit, Cancel]);

/// What an input tells its owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextInputEvent {
    /// The user changed the text. [`TextInput::set_text`] doesn't emit it.
    Changed,
    /// Enter was pressed.
    Submitted,
    /// Escape was pressed.
    Cancelled,
    /// The input lost keyboard focus.
    Blurred,
}

/// How an input looks. Every look takes its values from [`InputTheme`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextInputStyle {
    /// A filled field in a form or bar, with a ring while focused.
    #[default]
    Field,
    /// A compact field inside a list row, such as an inline rename.
    Inline,
    /// Bare, larger text at the top of a picker.
    Query,
    /// Bare heading text: the note's title.
    Title,
}

/// Font and size for one style.
struct Look {
    font: Font,
    font_size: Pixels,
    line_height: Pixels,
    text: Hsla,
    /// The field's height, for the styles drawn as a filled field.
    field_height: Option<Pixels>,
}

impl TextInputStyle {
    fn look(self, theme: &InputTheme) -> Look {
        let (font_size, weight, field_height) = match self {
            TextInputStyle::Field => (theme.font_size, None, Some(theme.field_height)),
            TextInputStyle::Inline => (theme.font_size, None, Some(theme.inline_height)),
            TextInputStyle::Query => (theme.query_font_size, None, None),
            TextInputStyle::Title => (theme.title_font_size, Some(theme.title_weight), None),
        };
        let mut look_font = font(theme.font_family.clone());
        look_font.weight = weight.unwrap_or(FontWeight::NORMAL);
        let text = if self == TextInputStyle::Title {
            theme.title_text
        } else {
            theme.text
        };
        Look {
            font: look_font,
            font_size,
            line_height: theme.line_height(font_size),
            text,
            field_height,
        }
    }
}

/// The line as last painted, for mapping the mouse and the IME to offsets.
struct PaintedLine {
    /// None while the placeholder shows.
    line: Option<ShapedLine>,
    /// Where offset 0 is drawn, after horizontal scrolling.
    origin: Point<Pixels>,
}

/// A one-line text input.
pub struct TextInput {
    focus_handle: FocusHandle,
    state: LineState,
    placeholder: SharedString,
    style: TextInputStyle,
    theme: InputTheme,
    invalid: bool,
    bubbles_enter_and_escape: bool,
    is_selecting: bool,
    /// How far the text is scrolled left to keep the cursor in view.
    scroll_x: Pixels,
    painted: Option<PaintedLine>,
    _blur: Subscription,
}

impl EventEmitter<TextInputEvent> for TextInput {}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl TextInput {
    /// An empty field.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let blur = cx.on_blur(&focus_handle, window, |input, _, cx| {
            input.is_selecting = false;
            cx.emit(TextInputEvent::Blurred);
            cx.notify();
        });
        Self {
            focus_handle,
            state: LineState::default(),
            placeholder: SharedString::default(),
            style: TextInputStyle::default(),
            theme: InputTheme {
                font_family: crate::ui::ui_theme(cx).font_family,
                ..InputTheme::default()
            },
            invalid: false,
            bubbles_enter_and_escape: false,
            is_selecting: false,
            scroll_x: Pixels::ZERO,
            painted: None,
            _blur: blur,
        }
    }

    /// The text shown, dimmed, while the input is empty.
    pub fn with_placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Sets the size of the title style's text, as the note's settings
    /// and zoom make it.
    pub fn set_title_font_size(&mut self, size: Pixels, cx: &mut Context<Self>) {
        if self.theme.title_font_size != size {
            self.theme.title_font_size = size;
            cx.notify();
        }
    }

    pub fn with_style(mut self, style: TextInputStyle) -> Self {
        self.style = style;
        self
    }

    /// Lets Enter and Escape also reach key bindings around the input,
    /// for owners (pickers, the find bar) that bind them in their own key
    /// context. The input still emits `Submitted` and `Cancelled`.
    pub fn bubble_enter_and_escape(mut self) -> Self {
        self.bubbles_enter_and_escape = true;
        self
    }

    pub fn style(&self) -> TextInputStyle {
        self.style
    }

    pub fn text(&self) -> &str {
        self.state.text()
    }

    /// Replaces the text, as a value rather than an edit: the cursor goes
    /// to the end, undo history is cleared and `Changed` isn't emitted.
    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.state.set_text(text);
        cx.notify();
    }

    pub fn selected_range(&self) -> Range<usize> {
        self.state.selected_range()
    }

    pub fn cursor(&self) -> usize {
        self.state.cursor()
    }

    /// Selects a byte range, clamped to character boundaries.
    pub fn select(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        self.state.select(range.start, range.end);
        cx.notify();
    }

    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        self.state.select_all();
        cx.notify();
    }

    /// The IME composition, if one is in progress.
    pub fn marked_range(&self) -> Option<Range<usize>> {
        self.state.marked()
    }

    /// Tints the field while its text isn't valid, such as a bad regex.
    pub fn set_invalid(&mut self, invalid: bool, cx: &mut Context<Self>) {
        if self.invalid != invalid {
            self.invalid = invalid;
            cx.notify();
        }
    }

    pub fn is_invalid(&self) -> bool {
        self.invalid
    }

    /// Runs an editing command by id. Returns false for ids an input
    /// doesn't run.
    pub fn run_command(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        let Some(command) = input_command(id) else {
            return false;
        };
        let changed = match command {
            InputCommand::Move(motion) => self.move_by(motion, false),
            InputCommand::Select(motion) => self.move_by(motion, true),
            InputCommand::Delete(motion) => self.state.delete(motion),
            InputCommand::SelectAll => self.select_every(),
            InputCommand::Copy => self.copy(cx),
            InputCommand::Cut => self.cut(cx),
            InputCommand::Paste => self.paste(cx),
            InputCommand::Undo => self.state.undo(),
            InputCommand::Redo => self.state.redo(),
        };
        self.after_edit(changed, cx);
        true
    }

    /// Replaces `range` with `text`, as the user typing would.
    pub fn replace(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        let changed = self.state.edit(range, text, EditKind::Other).changed;
        self.after_edit(changed, cx);
    }

    fn after_edit(&mut self, changed: bool, cx: &mut Context<Self>) {
        if changed {
            cx.emit(TextInputEvent::Changed);
        }
        cx.notify();
    }

    fn move_by(&mut self, motion: Motion, extend: bool) -> bool {
        self.state.move_by(motion, extend);
        false
    }

    fn select_every(&mut self) -> bool {
        self.state.select_all();
        false
    }

    fn copy(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some(text) = self.state.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
        }
        false
    }

    fn cut(&mut self, cx: &mut Context<Self>) -> bool {
        self.copy(cx);
        let selection = self.state.selected_range();
        !selection.is_empty() && self.state.edit(selection, "", EditKind::Other).changed
    }

    fn paste(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return false;
        };
        let range = self
            .state
            .marked()
            .unwrap_or_else(|| self.state.selected_range());
        self.state.edit(range, &text, EditKind::Other).changed
    }

    fn look(&self) -> Look {
        self.style.look(&self.theme)
    }

    fn on_run_command(&mut self, action: &RunCommand, _: &mut Window, cx: &mut Context<Self>) {
        if !self.run_command(&action.id, cx) {
            cx.propagate();
        }
    }

    fn on_submit(&mut self, _: &Submit, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(TextInputEvent::Submitted);
        if self.bubbles_enter_and_escape {
            cx.propagate();
        }
    }

    fn on_cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(TextInputEvent::Cancelled);
        if self.bubbles_enter_and_escape {
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
        match event.click_count {
            2 => {
                let word = self.state.word_at(offset);
                self.state.select(word.start, word.end);
            }
            count if count >= 3 => self.state.select_all(),
            _ if event.modifiers.shift => self.state.select(self.state.anchor(), offset),
            _ => self.state.select(offset, offset),
        }
        cx.notify();
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting && event.dragging() {
            let offset = self.offset_for_position(event.position);
            self.state.select(self.state.anchor(), offset);
            cx.notify();
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    /// The text offset nearest a point in the window.
    fn offset_for_position(&self, position: Point<Pixels>) -> usize {
        let Some(painted) = self.painted.as_ref() else {
            return self.state.text().len();
        };
        let Some(line) = painted.line.as_ref() else {
            return 0;
        };
        line.closest_index_for_x(position.x - painted.origin.x)
            .min(self.state.text().len())
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let look = self.look();
        let theme = &self.theme;
        let focused = self.focus_handle.is_focused(window);
        let background = match (self.invalid, focused) {
            (true, _) => theme.error_background,
            (false, true) => theme.focused_background,
            (false, false) => theme.background,
        };
        div()
            .key_context(TEXT_INPUT_CONTEXT)
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::on_run_command))
            .on_action(cx.listener(Self::on_submit))
            .on_action(cx.listener(Self::on_cancel))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .flex()
            .items_center()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .when_some(look.field_height, |field, height| {
                field
                    .h(height)
                    .px(theme.padding_x)
                    .rounded(theme.radius)
                    .bg(background)
                    .when(focused, |field| field.shadow(vec![theme.focus_ring()]))
            })
            .child(TextLine::new(cx.entity()))
    }
}

#[cfg(test)]
mod tests;
