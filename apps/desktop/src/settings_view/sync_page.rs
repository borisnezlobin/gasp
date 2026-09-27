//! The Sync page's own rows: the repository's address, signing in with a
//! token, and list settings such as the files that stay on each device.
//! The rest of the page is ordinary settings.

use editor_config::schema::SettingKind;
use editor_sync::Token;
use gpui::{AnyElement, ClickEvent, ClipboardItem, Context, Entity, SharedString, div, prelude::*};
use serde_json::Value;

use super::controls::{button, field_box, icon_button};
use super::model::SettingItem;
use super::view::{ControlRow, SettingsView, add_field_key};
use crate::icons::IconName;
use crate::sync::SyncService;
use crate::sync::credentials::store_name;

/// The key the repository field is kept and reports errors under.
pub(super) const REMOTE_FIELD: &str = "sync.remote";
/// The key token errors are reported under.
pub(super) const TOKEN_KEY: &str = "sync.token";
/// Where GitHub makes fine-grained tokens.
pub const NEW_TOKEN_URL: &str = "https://github.com/settings/personal-access-tokens/new";

/// What an empty field shows.
pub(super) fn placeholder(key: &str, adds: bool) -> &'static str {
    match key {
        REMOTE_FIELD => "https://github.com/you/notes.git",
        "sync.device-only+" => "Add a pattern",
        _ if adds => "Add by name",
        _ => "",
    }
}

/// Whether pasted text looks like a token: one word, no spaces.
fn looks_like_token(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty() && !text.chars().any(char::is_whitespace)
}

impl SettingsView {
    /// Lets the Sync page show and change the vault's sync.
    pub fn set_sync(&mut self, sync: Entity<SyncService>, cx: &mut Context<Self>) {
        let observe = cx.observe(&sync, |view, _, cx| {
            view.refresh_sync(cx);
            cx.notify();
        });
        self._subscriptions.push(observe);
        self.sync = Some(sync);
        self.sync_fields(cx);
        cx.notify();
    }

    /// The address of the remote, when the vault is a git clone that has one.
    pub(super) fn sync_remote(&self) -> Option<String> {
        self.remote_cache.clone()
    }

    /// Whether the remote signs in with a token: GitHub over HTTPS does, a
    /// folder or SSH address doesn't.
    pub(super) fn remote_takes_token(&self) -> bool {
        self.remote_cache
            .as_deref()
            .is_some_and(|url| url.starts_with("https://") || url.starts_with("http://"))
    }

    /// Remembers what the page shows about the remote and the account, so
    /// rows can be listed without reaching into the sync service.
    fn refresh_sync(&mut self, cx: &mut Context<Self>) {
        let (remote, signed_in) = self.sync.as_ref().map_or((None, false), |sync| {
            let sync = sync.read(cx);
            (sync.remote_url().map(str::to_string), sync.is_signed_in())
        });
        if remote != self.remote_cache || signed_in != self.signed_in_cache {
            self.remote_cache = remote;
            self.signed_in_cache = signed_in;
            self.invalidate_layouts();
        }
    }

    pub(super) fn remote_description(&self) -> String {
        if self.sync_remote().is_none() {
            return "This vault isn’t a git repository with a remote, so it doesn’t sync. Clone your notes repository into this folder to set it up.".to_string();
        }
        "The HTTPS address of the GitHub repository this vault syncs with.".to_string()
    }

    pub(super) fn account_description(&self) -> String {
        if self.signed_in_cache {
            format!("Signed in. The token is kept in {}.", store_name())
        } else {
            format!(
                "Copy a fine-grained token that can read and write only your notes repository, then paste it here. It’s kept in {}, never in the vault.",
                store_name()
            )
        }
    }

    /// The field that adds a list entry, then one row per entry.
    pub(super) fn list_rows(&self, item: &SettingItem) -> Vec<ControlRow> {
        let entries = self
            .current_value(item)
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .map(|value| ControlRow::ListEntry {
                list: item.clone(),
                value,
            });
        std::iter::once(ControlRow::ListAdd(item.clone()))
            .chain(entries)
            .collect()
    }

    pub(super) fn is_list(&self, key: &str) -> bool {
        self.item_for(key)
            .is_some_and(|item| matches!(item.kind, SettingKind::List(_)))
    }

    fn list_values(&self, item: &SettingItem) -> Vec<Value> {
        self.current_value(item)
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    /// Adds `text` to the list setting `key`, unless it's already there.
    pub(super) fn add_list_entry(&mut self, key: &str, text: &str, cx: &mut Context<Self>) {
        let Some(item) = self.item_for(key).cloned() else {
            return;
        };
        let mut values = self.list_values(&item);
        let value = Value::from(text);
        if text.is_empty() || values.contains(&value) {
            return;
        }
        values.push(value);
        self.write(&item, Some(Value::Array(values)), cx);
        if let Some(field) = self.fields.get(&add_field_key(key)) {
            field.update(cx, |field, cx| field.set_text("", cx));
        }
    }

    /// Removes `text` from the list setting `item`.
    pub fn remove_list_entry(&mut self, item: &SettingItem, text: &str, cx: &mut Context<Self>) {
        let mut values = self.list_values(item);
        values.retain(|value| value.as_str() != Some(text));
        self.write(item, Some(Value::Array(values)), cx);
    }

    // ---- Repository and token ----

    /// Puts the remote's address in its field, and remembers what the
    /// page shows about the account.
    pub(super) fn sync_remote_field(&mut self, cx: &mut Context<Self>) {
        self.refresh_sync(cx);
        if let Some(field) = self.fields.get(REMOTE_FIELD) {
            let text = self.remote_cache.clone().unwrap_or_default();
            field.update(cx, |field, cx| field.set_text(&text, cx));
        }
    }

    pub(super) fn commit_remote(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some(sync) = self.sync.clone() else {
            return;
        };
        let result = sync.update(cx, |sync, cx| sync.set_remote_url(text, cx));
        self.error = result
            .err()
            .map(|message| (REMOTE_FIELD.to_string(), message));
        cx.notify();
    }

    /// Reads a token from the clipboard and signs in with it.
    pub fn paste_token(&mut self, cx: &mut Context<Self>) {
        let Some(sync) = self.sync.clone() else {
            return;
        };
        let text = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default();
        if !looks_like_token(&text) {
            let message = "Copy the token from GitHub first, then press Paste token.";
            self.error = Some((TOKEN_KEY.to_string(), message.to_string()));
            cx.notify();
            return;
        }
        let token = Token::new(text.trim());
        let result = sync.update(cx, |sync, cx| sync.sign_in(token, cx));
        self.error = result.err().map(|message| (TOKEN_KEY.to_string(), message));
        // The token has no business staying on the clipboard.
        cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
        self.sync_remote_field(cx);
        cx.notify();
    }

    pub fn sign_out(&mut self, cx: &mut Context<Self>) {
        let Some(sync) = self.sync.clone() else {
            return;
        };
        let result = sync.update(cx, |sync, cx| sync.sign_out(cx));
        self.error = result.err().map(|message| (TOKEN_KEY.to_string(), message));
        self.sync_remote_field(cx);
        cx.notify();
    }

    // ---- Controls ----

    pub(super) fn remote_control(&self, row: &ControlRow, focused: bool) -> Option<AnyElement> {
        self.sync_remote()?;
        let field = self.field_for(row)?;
        Some(
            field_box(field, None, focused, &self.style)
                .w(self.style.field_width * 1.4)
                .into_any_element(),
        )
    }

    pub(super) fn account_control(&self, focused: bool, cx: &mut Context<Self>) -> AnyElement {
        let style = &self.style;
        if self.signed_in_cache {
            return button("sign-out", "Sign out", false, focused, style)
                .debug_selector(|| "sign-out".to_string())
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| view.sign_out(cx)))
                .into_any_element();
        }
        let create = button("new-token", "Create a token", false, false, style)
            .debug_selector(|| "new-token".to_string())
            .on_click(|_: &ClickEvent, _, cx| cx.open_url(NEW_TOKEN_URL));
        let paste = button("paste-token", "Paste token", true, focused, style)
            .debug_selector(|| "paste-token".to_string())
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| view.paste_token(cx)));
        div()
            .flex()
            .gap(style.control_gap)
            .child(create)
            .child(paste)
            .into_any_element()
    }

    pub(super) fn list_add_control(
        &self,
        item: &SettingItem,
        row: &ControlRow,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let field = self.field_control(row, focused);
        let reset = self.is_changed(item).then(|| {
            let key = item.key.clone();
            self.reset_button(&item.key, cx, move |view, cx| view.reset(&key, cx))
        });
        div()
            .flex()
            .items_center()
            .gap(self.style.gap_sm)
            .children(reset)
            .child(field)
            .into_any_element()
    }

    pub(super) fn list_entry_control(
        &self,
        list: &SettingItem,
        value: &str,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let style = &self.style;
        let selector = format!("remove-entry-{value}");
        let list = list.clone();
        let value = value.to_string();
        icon_button(
            SharedString::from(selector.clone()),
            IconName::X,
            style.text_muted,
            style,
        )
        .debug_selector(|| selector)
        .when(focused, |remove| remove.shadow(vec![style.focus()]))
        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            view.remove_list_entry(&list, &value, cx);
        }))
        .into_any_element()
    }

    // ---- Keys ----

    pub(super) fn account_key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        if !matches!(key, "space" | "enter") {
            return false;
        }
        if self.signed_in_cache {
            self.sign_out(cx);
        } else {
            self.paste_token(cx);
        }
        true
    }

    pub(super) fn list_entry_key(
        &mut self,
        list: &SettingItem,
        value: &str,
        key: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if !matches!(key, "delete" | "backspace" | "space" | "enter") {
            return false;
        }
        self.remove_list_entry(list, value, cx);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_single_words_pass_as_tokens() {
        assert!(looks_like_token("github_pat_11ABCDEF"));
        assert!(looks_like_token("  ghp_abc123\n"));
        assert!(!looks_like_token(""));
        assert!(!looks_like_token("some copied sentence"));
    }
}
