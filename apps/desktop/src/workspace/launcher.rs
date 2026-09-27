//! What an empty tab shows: the notes opened most recently, and the two
//! ways to something else, a new note and the quick switcher. Arrows move,
//! Enter opens.
//!
//! The list starts with this session's notes and fills up with the
//! vault's most recently changed ones, which are found off the main
//! thread so a new tab opens at once however large the vault.

use std::path::{Path, PathBuf};

use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, KeyDownEvent, MouseButton, SharedString,
    Task, Window, div, prelude::*,
};

use super::files::{note_title, notes_by_recency};
use crate::icons::{IconName, icon};
use crate::keymap::RunCommand;
use crate::theme::UiTheme;
use crate::ui::{keycap, truncated, ui_theme};

/// Notes the launcher lists at most.
pub const MAX_RECENT: usize = 12;

/// Folders the launcher reads at most when looking for recent notes.
pub(crate) const RECENT_SCAN_FOLDERS: usize = 200;

/// The actions under the list: (command, label, icon).
const ACTIONS: [(&str, &str, IconName); 2] = [
    ("note.new", "New note", IconName::NotePencil),
    ("switcher.open", "Find a note", IconName::MagnifyingGlass),
];

/// The launcher asks for a note to open in its tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenRecent(pub PathBuf);

pub struct Launcher {
    focus_handle: FocusHandle,
    vault: PathBuf,
    recent: Vec<PathBuf>,
    selected: usize,
    _scan: Task<()>,
}

impl EventEmitter<OpenRecent> for Launcher {}

impl Focusable for Launcher {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Launcher {
    /// A launcher listing `recent` first, then the vault's most recently
    /// changed notes once they're found.
    pub fn new(vault: &Path, recent: Vec<PathBuf>, cx: &mut Context<Self>) -> Self {
        let root = vault.to_path_buf();
        let scanning =
            cx.background_spawn(async move { notes_by_recency(&root, RECENT_SCAN_FOLDERS) });
        let scan = cx.spawn(async move |launcher, cx| {
            let found = scanning.await;
            launcher
                .update(cx, |launcher, cx| launcher.add_found(found, cx))
                .ok();
        });
        Launcher {
            focus_handle: cx.focus_handle(),
            vault: vault.to_path_buf(),
            recent: recent.into_iter().take(MAX_RECENT).collect(),
            selected: 0,
            _scan: scan,
        }
    }

    pub fn recent(&self) -> &[PathBuf] {
        &self.recent
    }

    /// Shows `recent` in place of the notes listed, keeping the selection
    /// in range.
    pub fn set_recent(&mut self, recent: Vec<PathBuf>, cx: &mut Context<Self>) {
        let recent: Vec<PathBuf> = recent.into_iter().take(MAX_RECENT).collect();
        if recent == self.recent {
            return;
        }
        self.recent = recent;
        self.selected = self.selected.min(self.recent.len().saturating_sub(1));
        cx.notify();
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Appends notes the scan found, after the ones already listed, so the
    /// row the keyboard is on stays put.
    fn add_found(&mut self, found: Vec<PathBuf>, cx: &mut Context<Self>) {
        for path in found {
            if self.recent.len() >= MAX_RECENT {
                break;
            }
            if !self.recent.contains(&path) {
                self.recent.push(path);
            }
        }
        cx.notify();
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
        let folder = relative.parent()?.to_string_lossy().replace('\\', "/");
        (!folder.is_empty()).then(|| folder.into())
    }

    fn render_row(
        &self,
        index: usize,
        path: &Path,
        ui: &UiTheme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = index == self.selected;
        list_row(ui)
            .id(("recent", index))
            .when(selected, |row| row.bg(ui.row_selected))
            .when(!selected, |row| row.hover(|style| style.bg(ui.row_hover)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |launcher, _, _, cx| launcher.open(index, cx)),
            )
            .child(row_icon(IconName::FileText, ui))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .max_w(gpui::relative(0.7))
                    .child(truncated(SharedString::from(note_title(path)))),
            )
            .children(self.folder_label(path).map(|folder| {
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .text_size(ui.small_font_size)
                    .text_color(ui.text_detail)
                    .child(truncated(folder).grow())
            }))
    }
}

/// A row of the launcher: an icon, a label and whatever follows.
fn list_row(ui: &UiTheme) -> gpui::Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(ui.space_md + ui.space_xs)
        .h(ui.row_height)
        .px(ui.row_padding_x)
        .rounded(ui.row_radius)
}

fn row_icon(name: IconName, ui: &UiTheme) -> impl IntoElement {
    icon(name)
        .flex_none()
        .size(ui.icon_size - gpui::px(2.))
        .text_color(ui.icon)
}

/// A way out of the empty tab, with its shortcut.
fn render_action(
    id: &'static str,
    label: &'static str,
    name: IconName,
    cx: &mut App,
) -> impl IntoElement {
    let ui = ui_theme(cx);
    list_row(&ui)
        .id(id)
        .debug_selector(move || format!("launcher-{id}"))
        .text_color(ui.text_muted)
        .hover(|style| style.bg(ui.row_hover))
        .on_click(move |_, window, cx| {
            window.dispatch_action(Box::new(RunCommand { id: id.into() }), cx)
        })
        .child(row_icon(name, &ui))
        .child(div().flex_1().child(label))
        .children(crate::ui::hints::shortcut(id, cx).map(|shortcut| keycap(shortcut, &ui.keycap)))
}

impl Render for Launcher {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = ui_theme(cx);
        let recent = self.recent.clone();
        let rows: Vec<_> = recent
            .iter()
            .enumerate()
            .map(|(index, path)| self.render_row(index, path, &ui, cx).into_any_element())
            .collect();
        let heading = if rows.is_empty() {
            "This vault has no notes yet."
        } else {
            "Recent notes"
        };
        let actions: Vec<_> = ACTIONS
            .iter()
            .map(|&(id, label, name)| render_action(id, label, name, cx).into_any_element())
            .collect();
        div()
            .id("launcher")
            .key_context("Launcher")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .flex()
            .justify_center()
            .px(ui.space_xl)
            .pt(ui.dialog_top_offset)
            .font_family(ui.font_family.clone())
            .text_size(ui.font_size)
            .text_color(ui.text)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(ui.dialog_width)
                    .max_w_full()
                    .gap(ui.space_xs)
                    .child(
                        div()
                            .px(ui.row_padding_x)
                            .pb(ui.space_sm)
                            .text_color(ui.text_detail)
                            .child(heading),
                    )
                    .children(rows)
                    .child(div().h(ui.space_lg))
                    .children(actions),
            )
    }
}
