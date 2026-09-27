//! What an empty tab shows: the notes opened most recently. Arrows move,
//! Enter opens.

use std::path::{Path, PathBuf};

use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, KeyDownEvent, MouseButton, SharedString,
    Window, div, prelude::*,
};

use super::files::note_title;
use crate::icons::{IconName, icon};
use crate::theme::Theme;

/// Notes the launcher lists at most.
pub const MAX_RECENT: usize = 12;

/// The launcher asks for a note to open in its tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenRecent(pub PathBuf);

pub struct Launcher {
    focus_handle: FocusHandle,
    vault: PathBuf,
    recent: Vec<PathBuf>,
    selected: usize,
    theme: Theme,
}

impl EventEmitter<OpenRecent> for Launcher {}

impl Focusable for Launcher {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Launcher {
    pub fn new(vault: &Path, recent: Vec<PathBuf>, cx: &mut Context<Self>) -> Self {
        Launcher {
            focus_handle: cx.focus_handle(),
            vault: vault.to_path_buf(),
            recent: recent.into_iter().take(MAX_RECENT).collect(),
            selected: 0,
            theme: Theme::default(),
        }
    }

    pub fn recent(&self) -> &[PathBuf] {
        &self.recent
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.recent.is_empty() {
            return;
        }
        let last = self.recent.len() - 1;
        self.selected = self.selected.saturating_add_signed(delta).min(last);
        cx.notify();
    }

    fn open(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(path) = self.recent.get(index) {
            cx.emit(OpenRecent(path.clone()));
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if keystroke.modifiers.modified() {
            return;
        }
        match keystroke.key.as_str() {
            "up" => self.move_selection(-1, cx),
            "down" => self.move_selection(1, cx),
            "enter" => self.open(self.selected, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn folder_label(&self, path: &Path) -> Option<SharedString> {
        let relative = path.strip_prefix(&self.vault).unwrap_or(path);
        let folder = relative.parent()?.to_string_lossy().into_owned();
        (!folder.is_empty()).then(|| folder.into())
    }

    fn render_row(&self, index: usize, path: &Path, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme.workspace;
        let selected = index == self.selected;
        let mut row = div()
            .id(("recent", index))
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_md)
            .px(theme.space_md)
            .py(theme.space_sm)
            .rounded(theme.radius_md)
            .text_color(theme.text)
            .hover(|style| style.bg(theme.list_hover_background))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |launcher, _, _, cx| launcher.open(index, cx)),
            )
            .child(
                icon(IconName::FileText)
                    .size(theme.icon_size)
                    .text_color(theme.text_muted),
            )
            .child(SharedString::from(note_title(path)));
        if selected {
            row = row.bg(theme.list_hover_background);
        }
        if let Some(folder) = self.folder_label(path) {
            row = row.child(div().text_color(theme.text_faint).child(folder));
        }
        row
    }
}

impl Render for Launcher {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.workspace.clone();
        let rows: Vec<_> = self
            .recent
            .clone()
            .iter()
            .enumerate()
            .map(|(index, path)| self.render_row(index, path, cx).into_any_element())
            .collect();
        let body = if rows.is_empty() {
            div()
                .text_color(theme.text_muted)
                .child("No notes here yet.")
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap(theme.space_xs)
                .children(rows)
                .into_any_element()
        };
        div()
            .id("launcher")
            .key_context("Launcher")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .flex()
            .justify_center()
            .pt(theme.modal_top_offset)
            .text_size(theme.ui_font_size)
            .child(div().w(theme.launcher_width).child(body))
    }
}
