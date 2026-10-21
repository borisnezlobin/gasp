//! The encryption dialogs: one form for every password question
//! (choosing the vault password, typing it, changing it), and the
//! options of one encrypted note.

use gasp_crypt::{ClaudeAccess, NoteOptions};
use gpui::{
    AnyElement, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    SharedString, Subscription, Window, div, prelude::*,
};

use crate::settings_view::controls::{row_text, segment, segmented, toggle_switch, two_column_row};
use crate::text_input::{TextInput, TextInputEvent};
use crate::theme::UiTheme;
use crate::ui::{Button, Selectable, ui_theme};

/// What a password form does with the typed passwords, in field order:
/// `Ok` closes it, `Err` shows the sentence under the fields.
pub type PasswordSubmit = Box<dyn FnMut(&[String], &mut Window, &mut App) -> Result<(), String>>;

/// One password question.
pub struct PasswordForm {
    pub title: &'static str,
    pub detail: Option<String>,
    /// Each field's label.
    pub fields: Vec<&'static str>,
    pub submit_label: &'static str,
    pub on_submit: PasswordSubmit,
}

pub struct PasswordDialog {
    form: PasswordForm,
    inputs: Vec<Entity<TextInput>>,
    error: Option<SharedString>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<DismissEvent> for PasswordDialog {}

impl Focusable for PasswordDialog {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.inputs
            .first()
            .map_or_else(|| self.focus_handle.clone(), |input| input.focus_handle(cx))
    }
}

/// A secure field that leaves Enter and Escape to its form.
fn password_input(window: &mut Window, cx: &mut App) -> Entity<TextInput> {
    cx.new(|cx| TextInput::new(window, cx).secure())
}

/// Two passwords that must match, as a new password and its confirmation.
pub fn matching(first: &str, second: &str) -> Result<(), String> {
    match first == second {
        true => Ok(()),
        false => Err("The two passwords don’t match. Type the same one twice.".to_owned()),
    }
}

impl PasswordDialog {
    pub fn new(form: PasswordForm, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let inputs: Vec<_> = form
            .fields
            .iter()
            .map(|_| password_input(window, cx))
            .collect();
        let subscriptions = inputs
            .iter()
            .map(|input| cx.subscribe_in(input, window, Self::on_input_event))
            .collect();
        PasswordDialog {
            form,
            inputs,
            error: None,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    /// Fills the fields in order, as typing would.
    pub fn fill(&mut self, values: &[&str], cx: &mut Context<Self>) {
        for (input, value) in self.inputs.iter().zip(values) {
            input.update(cx, |input, cx| input.set_text(value, cx));
        }
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_ref().map(SharedString::as_ref)
    }

    fn on_input_event(
        &mut self,
        input: &Entity<TextInput>,
        event: &TextInputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TextInputEvent::Submitted => self.advance_from(input, window, cx),
            TextInputEvent::Cancelled => cx.emit(DismissEvent),
            TextInputEvent::Changed | TextInputEvent::Blurred => {}
        }
    }

    /// Enter moves to the next field, and from the last one submits.
    fn advance_from(&mut self, input: &Entity<TextInput>, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.inputs.iter().position(|known| known == input);
        match index.and_then(|index| self.inputs.get(index + 1)) {
            Some(next) => window.focus(&next.focus_handle(cx)),
            None => self.submit(window, cx),
        }
    }

    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let values: Vec<String> = self
            .inputs
            .iter()
            .map(|input| input.read(cx).text().to_owned())
            .collect();
        match (self.form.on_submit)(&values, window, cx) {
            Ok(()) => cx.emit(DismissEvent),
            Err(error) => {
                self.error = Some(error.into());
                cx.notify();
            }
        }
    }

    fn render_field(&self, index: usize, ui: &UiTheme) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(ui.space_xs)
            .child(
                div()
                    .text_size(ui.small_font_size)
                    .text_color(ui.text_muted)
                    .child(self.form.fields[index]),
            )
            .child(self.inputs[index].clone())
            .into_any_element()
    }
}

/// The line under a form's fields that says why it didn't go through. It
/// keeps its height when empty, so nothing moves when it fills.
fn error_line(id: &'static str, error: Option<SharedString>, ui: &UiTheme) -> impl IntoElement {
    div()
        .id(id)
        .selector(move || id.to_owned())
        .min_h(ui.font_size * 2.6)
        .text_size(ui.small_font_size)
        .text_color(ui.error)
        .children(error)
}

/// A dialog's Cancel and main buttons, at its bottom right.
fn dialog_buttons(primary: Button, cancel: Button, ui: &UiTheme) -> impl IntoElement {
    div()
        .flex()
        .justify_end()
        .gap(ui.space_sm)
        .child(cancel)
        .child(primary.primary())
}

impl Render for PasswordDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = ui_theme(cx);
        let fields: Vec<AnyElement> = (0..self.inputs.len())
            .map(|index| self.render_field(index, &ui))
            .collect();
        let submit = Button::new("password-dialog-submit", self.form.submit_label)
            .on_click(cx.listener(|dialog, _, window, cx| dialog.submit(window, cx)));
        let cancel = Button::new("password-dialog-cancel", "Cancel")
            .on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)));
        crate::ui::dialog(&ui)
            .id("password-dialog")
            .selector(|| "password-dialog".to_owned())
            .track_focus(&self.focus_handle)
            .w(ui.small_dialog_width)
            .p(ui.space_xl)
            .gap(ui.space_md)
            .child(
                div()
                    .text_size(ui.font_size + gpui::px(1.))
                    .font_weight(ui.strong_weight)
                    .child(self.form.title),
            )
            .children(
                self.form
                    .detail
                    .clone()
                    .map(|detail| div().text_color(ui.text_muted).child(detail)),
            )
            .children(fields)
            .child(error_line("password-dialog-error", self.error.clone(), &ui))
            .child(dialog_buttons(submit, cancel, &ui))
    }
}

/// Which password opens a note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasswordSource {
    Vault,
    Own,
}

/// What the options dialog asks for when it's saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionsChoice {
    pub source: PasswordSource,
    /// A new password for the note itself; `None` keeps the one it has.
    pub new_password: Option<String>,
    pub options: NoteOptions,
}

/// What saving the options does: `Err` shows the sentence in the dialog.
pub type OptionsSubmit = Box<dyn FnMut(OptionsChoice, &mut Window, &mut App) -> Result<(), String>>;

/// The options of one open encrypted note.
pub struct NoteOptionsDialog {
    title: String,
    had_own_password: bool,
    source: PasswordSource,
    options: NoteOptions,
    new_password: Entity<TextInput>,
    confirm: Entity<TextInput>,
    error: Option<SharedString>,
    on_submit: OptionsSubmit,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<DismissEvent> for NoteOptionsDialog {}

impl Focusable for NoteOptionsDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// What each Claude access choice is called.
const CLAUDE_LABELS: [(ClaudeAccess, &str); 3] = [
    (ClaudeAccess::Never, "Never"),
    (ClaudeAccess::WhileUnlocked, "While unlocked"),
    (ClaudeAccess::Always, "Always"),
];

impl NoteOptionsDialog {
    pub fn new(
        title: String,
        options: NoteOptions,
        has_own_password: bool,
        on_submit: OptionsSubmit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let new_password = password_input(window, cx);
        let confirm = password_input(window, cx);
        let subscriptions = [&new_password, &confirm]
            .map(|input| cx.subscribe_in(input, window, Self::on_input_event))
            .into();
        let source = match has_own_password {
            true => PasswordSource::Own,
            false => PasswordSource::Vault,
        };
        NoteOptionsDialog {
            title,
            had_own_password: has_own_password,
            source,
            options,
            new_password,
            confirm,
            error: None,
            on_submit,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    pub fn set_source(&mut self, source: PasswordSource, cx: &mut Context<Self>) {
        self.source = source;
        cx.notify();
    }

    pub fn set_hide_name(&mut self, hidden: bool, cx: &mut Context<Self>) {
        self.options.name = match hidden {
            true => gasp_crypt::NameVisibility::Hidden,
            false => gasp_crypt::NameVisibility::Shown,
        };
        cx.notify();
    }

    pub fn set_claude(&mut self, claude: ClaudeAccess, cx: &mut Context<Self>) {
        self.options.claude = claude;
        cx.notify();
    }

    /// Fills the new password and its confirmation, as typing would.
    pub fn fill_password(&mut self, password: &str, confirm: &str, cx: &mut Context<Self>) {
        self.new_password
            .update(cx, |input, cx| input.set_text(password, cx));
        self.confirm.update(cx, |input, cx| input.set_text(confirm, cx));
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_ref().map(SharedString::as_ref)
    }

    fn hides_name(&self) -> bool {
        self.options.name == gasp_crypt::NameVisibility::Hidden
    }

    fn on_input_event(
        &mut self,
        _: &Entity<TextInput>,
        event: &TextInputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TextInputEvent::Submitted => self.submit(window, cx),
            TextInputEvent::Cancelled => cx.emit(DismissEvent),
            TextInputEvent::Changed | TextInputEvent::Blurred => {}
        }
    }

    /// The choice as the form has it, or why it isn't one yet.
    fn choice(&self, cx: &App) -> Result<OptionsChoice, String> {
        let typed = self.new_password.read(cx).text().to_owned();
        matching(&typed, self.confirm.read(cx).text())?;
        let needs_password = self.source == PasswordSource::Own && !self.had_own_password;
        if needs_password && typed.is_empty() {
            return Err("Choose a password for this note.".to_owned());
        }
        let new_password = (self.source == PasswordSource::Own && !typed.is_empty()).then_some(typed);
        Ok(OptionsChoice {
            source: self.source,
            new_password,
            options: self.options,
        })
    }

    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = self
            .choice(cx)
            .and_then(|choice| (self.on_submit)(choice, window, cx));
        match result {
            Ok(()) => cx.emit(DismissEvent),
            Err(error) => {
                self.error = Some(error.into());
                cx.notify();
            }
        }
    }

    fn render_source(&self, cx: &mut Context<Self>) -> AnyElement {
        let style = crate::ui::settings_theme(cx);
        let choices = [
            (PasswordSource::Vault, "Vault password"),
            (PasswordSource::Own, "Its own password"),
        ];
        let segments = choices.map(|(source, label)| {
            segment(("note-password", source as usize), label, self.source == source, &style)
                .on_click(cx.listener(move |dialog, _, _, cx| dialog.set_source(source, cx)))
        });
        let text = row_text("Unlocks with", None, Vec::new(), &style);
        two_column_row(
            "note-password",
            text,
            Some(segmented("note-password-choice", segments, false, &style).into_any_element()),
            &style,
        )
        .into_any_element()
    }

    /// The new password's fields, shown only for a note with its own.
    fn render_password_fields(&self, ui: &UiTheme) -> Option<AnyElement> {
        if self.source != PasswordSource::Own {
            return None;
        }
        let label = match self.had_own_password {
            true => "New password (leave empty to keep the current one)",
            false => "Password",
        };
        let field = |label: &'static str, input: &Entity<TextInput>| {
            div()
                .flex()
                .flex_col()
                .gap(ui.space_xs)
                .child(
                    div()
                        .text_size(ui.small_font_size)
                        .text_color(ui.text_muted)
                        .child(label),
                )
                .child(input.clone())
        };
        Some(
            div()
                .flex()
                .flex_col()
                .gap(ui.space_md)
                .child(field(label, &self.new_password))
                .child(field("Type it again", &self.confirm))
                .into_any_element(),
        )
    }

    fn render_hide_name(&self, cx: &mut Context<Self>) -> AnyElement {
        let style = crate::ui::settings_theme(cx);
        let hidden = self.hides_name();
        let toggle = toggle_switch("note-hide-name", hidden, false, &style)
            .on_click(cx.listener(move |dialog, _, _, cx| dialog.set_hide_name(!hidden, cx)));
        let description = div()
            .child("The file gets a stand-in name like “Encrypted 3f9a2c1b”.")
            .into_any_element();
        let text = row_text("Hide the file name", Some(description), Vec::new(), &style);
        two_column_row("note-hide-name", text, Some(toggle.into_any_element()), &style)
            .into_any_element()
    }

    fn render_claude(&self, cx: &mut Context<Self>) -> AnyElement {
        let style = crate::ui::settings_theme(cx);
        let segments = CLAUDE_LABELS.map(|(access, label)| {
            segment(("note-claude", access as usize), label, self.options.claude == access, &style)
                .on_click(cx.listener(move |dialog, _, _, cx| dialog.set_claude(access, cx)))
        });
        let text = row_text("Claude can read it", None, Vec::new(), &style);
        two_column_row(
            "note-claude",
            text,
            Some(segmented("note-claude-choice", segments, false, &style).into_any_element()),
            &style,
        )
        .into_any_element()
    }
}

impl Render for NoteOptionsDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = ui_theme(cx);
        let save = Button::new("note-options-save", "Save changes")
            .on_click(cx.listener(|dialog, _, window, cx| dialog.submit(window, cx)));
        let cancel = Button::new("note-options-cancel", "Cancel")
            .on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)));
        crate::ui::dialog(&ui)
            .id("note-options")
            .selector(|| "note-options".to_owned())
            .track_focus(&self.focus_handle)
            .w(ui.dialog_width)
            .p(ui.space_xl)
            .gap(ui.space_lg)
            .child(
                div()
                    .text_size(ui.font_size + gpui::px(1.))
                    .font_weight(ui.strong_weight)
                    .child(format!("How “{}” is encrypted", self.title)),
            )
            .child(self.render_source(cx))
            .children(self.render_password_fields(&ui))
            .child(self.render_hide_name(cx))
            .child(self.render_claude(cx))
            .child(error_line("note-options-error", self.error.clone(), &ui))
            .child(dialog_buttons(save, cancel, &ui))
    }
}
