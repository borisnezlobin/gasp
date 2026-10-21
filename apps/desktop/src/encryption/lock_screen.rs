//! What a tab shows in place of the editor while its encrypted note is
//! locked: the whale resting above a password field, the reason a
//! password failed right under the field, and Touch ID where it's on. The
//! same whale covers an open encrypted note while the window is away.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gasp_crypt::{KeySource, peek};
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, Global, RenderImage,
    SharedString, Subscription, Window, div, prelude::*, px,
};

use super::keychain::{self, TouchIdError};
use crate::icons::{IconName, icon};
use crate::text_input::{TextInput, TextInputEvent};
use crate::theme::UiTheme;
use crate::ui::{Button, Selectable, ui_theme};
use crate::workspace::note_doc::NoteDoc;

/// How wide the whale draws over the field.
const WHALE_WIDTH: f32 = 168.;
/// How wide the password row is at most.
const FORM_WIDTH: f32 = 340.;

/// The still whale, decoded once per theme for every locked tab.
#[derive(Default)]
struct LockWhale(Option<(bool, Arc<RenderImage>)>);

impl Global for LockWhale {}

fn whale(cx: &mut App) -> Option<Arc<RenderImage>> {
    let dark = crate::ui::is_dark(cx);
    let kept = cx.default_global::<LockWhale>();
    if kept.0.as_ref().is_none_or(|(was_dark, _)| *was_dark != dark) {
        kept.0 = crate::tour::WhaleArt::still(dark).map(|still| (dark, still));
    }
    kept.0.as_ref().map(|(_, still)| still.clone())
}

fn drawn_whale(width: f32, cx: &mut App) -> Option<AnyElement> {
    whale(cx).map(|still| crate::tour::drawn_still(&still, px(width)))
}

/// What the lock screen tells its tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockScreenEvent {
    Unlocked,
}

pub struct LockScreen {
    doc: Entity<NoteDoc>,
    password: Entity<TextInput>,
    error: Option<SharedString>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<LockScreenEvent> for LockScreen {}

impl Focusable for LockScreen {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.password.focus_handle(cx)
    }
}

impl LockScreen {
    pub fn new(doc: Entity<NoteDoc>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let password = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_placeholder("Password")
                .secure()
        });
        let subscriptions = vec![cx.subscribe_in(&password, window, Self::on_password_event)];
        LockScreen {
            doc,
            password,
            error: None,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    /// The field the password goes in, for tests to type into.
    pub fn password_field(&self) -> &Entity<TextInput> {
        &self.password
    }

    /// Why the last try didn't unlock the note.
    pub fn error(&self) -> Option<&str> {
        self.error.as_ref().map(SharedString::as_ref)
    }

    fn path(&self, cx: &App) -> PathBuf {
        self.doc.read(cx).path().to_path_buf()
    }

    fn on_password_event(
        &mut self,
        _: &Entity<TextInput>,
        event: &TextInputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TextInputEvent::Submitted => self.unlock(window, cx),
            TextInputEvent::Changed if self.error.is_some() => self.show_error(None, cx),
            _ => {}
        }
    }

    fn show_error(&mut self, error: Option<String>, cx: &mut Context<Self>) {
        self.error = error.map(SharedString::from);
        cx.notify();
    }

    /// Tries the typed password on the note.
    pub fn unlock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let password = self.password.read(cx).text().to_owned();
        if password.is_empty() {
            self.show_error(Some("Type the password first.".to_owned()), cx);
            return;
        }
        let unlocked = self
            .doc
            .update(cx, |doc, cx| doc.unlock_with_password(&password, cx));
        self.password.update(cx, |field, cx| field.set_text("", cx));
        match unlocked {
            Ok(()) => {
                keep_key_for_touch_id(&self.path(cx), cx);
                self.finish_unlocking(window, cx);
            }
            Err(error) => self.show_error(Some(error), cx),
        }
    }

    fn finish_unlocking(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        super::reopen_locked_notes(&self.path(cx), cx);
        cx.emit(LockScreenEvent::Unlocked);
        cx.notify();
    }

    /// Asks for Touch ID, then opens the note with the key it kept.
    fn unlock_with_touch_id(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.path(cx);
        let raw = self.doc.read(cx).disk_text().to_owned();
        let Some(id) = super::read_encryption(&path, cx, |encryption, _| {
            encryption.key_id_for(&raw)
        })
        .flatten() else {
            return;
        };
        let reason = format!("unlock “{}”", crate::workspace::files::note_title(&path));
        let reading = keychain::read_with_touch_id(id.clone(), reason, cx);
        cx.spawn_in(window, async move |screen, cx| {
            let read = reading.await;
            screen
                .update_in(cx, |screen, window, cx| {
                    screen.take_touch_id_key(read, &id, window, cx)
                })
                .ok();
        })
        .detach();
    }

    fn take_touch_id_key(
        &mut self,
        read: Result<gasp_crypt::Key, TouchIdError>,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = match read {
            Ok(key) => key,
            Err(error) => return self.show_error(Some(error.to_string()), cx),
        };
        match self.doc.update(cx, |doc, cx| doc.unlock_with_key(key, cx)) {
            Ok(()) => self.finish_unlocking(window, cx),
            Err(_) => {
                keychain::forget(id, cx);
                let message = "The password changed since Touch ID last unlocked this. Type the new one.";
                self.show_error(Some(message.to_owned()), cx);
            }
        }
    }

    /// What the note asks for: its own password, or the vault's.
    fn asks_for_own_password(&self, cx: &App) -> bool {
        peek(self.doc.read(cx).disk_text())
            .is_ok_and(|header| matches!(header.source, KeySource::Note { .. }))
    }

    fn shows_touch_id(&self, cx: &App) -> bool {
        super::settings_for(&self.path(cx), cx).biometric_unlock && keychain::is_available()
    }

    fn render_form(&self, ui: &UiTheme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let unlock = Button::new("lock-screen-unlock", "Unlock")
            .primary()
            .on_click(cx.listener(|screen, _, window, cx| screen.unlock(window, cx)));
        let row = div()
            .flex()
            .items_center()
            .gap(ui.space_md)
            .child(div().flex_1().min_w_0().child(self.password.clone()))
            .child(unlock);
        // The line keeps its height when empty, so a refusal moves nothing.
        let error = div()
            .id("lock-screen-error")
            .selector(|| "lock-screen-error".to_owned())
            .min_h(ui.text_line_height * 2.)
            .text_size(ui.small_font_size)
            .text_color(ui.error)
            .children(self.error.clone());
        div()
            .w_full()
            .max_w(px(FORM_WIDTH))
            .flex()
            .flex_col()
            .gap(ui.space_sm)
            .child(row)
            .child(error)
    }

    fn render_touch_id(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.shows_touch_id(cx) {
            return None;
        }
        let ui = ui_theme(cx);
        let button = div()
            .id("lock-screen-touch-id")
            .selector(|| "lock-screen-touch-id".to_owned())
            .flex()
            .items_center()
            .gap(ui.space_sm)
            .h(ui.button_height)
            .px(ui.button_padding_x)
            .rounded(ui.icon_button_radius)
            .text_color(ui.text_muted)
            .hover(move |style| style.bg(ui.control_hover).text_color(ui.text))
            .on_click(cx.listener(|screen, _, window, cx| screen.unlock_with_touch_id(window, cx)))
            .child(icon(IconName::Fingerprint).size(ui.icon_size))
            .child("Unlock with Touch ID");
        Some(button.into_any_element())
    }
}

/// Keeps the key that just opened the note at `path` behind Touch ID,
/// when that's on.
pub fn keep_key_for_touch_id(path: &Path, cx: &mut App) {
    if !super::settings_for(path, cx).biometric_unlock || !keychain::is_available() {
        return;
    }
    let kept = super::read_encryption(path, cx, |encryption, relative| {
        encryption
            .wrapping_key(relative)
            .map(|(id, key)| (id, key.clone()))
    })
    .flatten();
    if let Some((id, key)) = kept {
        keychain::keep(&id, &key, cx);
    }
}

/// Keeps the vault's key behind Touch ID after a password unlocked or
/// set it, when that's on.
pub fn keep_vault_key_for_touch_id(vault: &Path, cx: &mut App) {
    if !super::settings_for(vault, cx).biometric_unlock || !keychain::is_available() {
        return;
    }
    let kept = super::read_encryption(vault, cx, |encryption, _| {
        Some((encryption.vault_key_id()?, encryption.vault_key()?.clone()))
    })
    .flatten();
    if let Some((id, key)) = kept {
        keychain::keep(&id, &key, cx);
    }
}

impl Render for LockScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = ui_theme(cx);
        let prompt = match self.asks_for_own_password(cx) {
            true => "This note has its own password.",
            false => "Type the vault password to read it.",
        };
        let heading = div()
            .flex()
            .items_center()
            .gap(ui.space_sm)
            .text_size(ui.font_size + px(2.))
            .font_weight(ui.strong_weight)
            .text_color(ui.text)
            .child(
                icon(IconName::LockSimple)
                    .size(ui.icon_size)
                    .text_color(ui.text_muted),
            )
            .child("This note is locked");
        let form = self.render_form(&ui, cx);
        div()
            .id("lock-screen")
            .selector(|| "lock-screen".to_owned())
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .px(ui.space_xl)
            .pt(ui.space_xl * 4.)
            .gap(ui.space_lg)
            .children(drawn_whale(WHALE_WIDTH, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(ui.space_xs)
                    .child(heading)
                    .child(div().text_color(ui.text_muted).child(prompt)),
            )
            .child(form)
            .children(self.render_touch_id(cx))
    }
}

/// The cover over an open encrypted note while the window is away: an
/// opaque sheet in the note's own colour, so none of the text shows, with
/// the whale and a word on why.
pub fn away_cover(cx: &mut App) -> AnyElement {
    let ui = ui_theme(cx);
    div()
        .id("away-cover")
        .selector(|| "away-cover".to_owned())
        .absolute()
        .inset_0()
        .occlude()
        .flex()
        .flex_col()
        .items_center()
        .pt(ui.space_xl * 4.)
        .gap(ui.space_lg)
        .bg(ui.note_background)
        .children(drawn_whale(WHALE_WIDTH, cx))
        .child(
            div()
                .flex()
                .items_center()
                .gap(ui.space_sm)
                .text_color(ui.text_muted)
                .child(icon(IconName::LockSimple).size(ui.icon_size))
                .child("Hidden while Gasp is in the background"),
        )
        .into_any_element()
}
