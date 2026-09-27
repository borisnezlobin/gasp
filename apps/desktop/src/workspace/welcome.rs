//! The empty state before any vault is open: one action, "Open a folder",
//! which has focus so Enter or Space runs it, and the vaults this device
//! opened before, when any still exist.

use std::path::{Path, PathBuf};

use gpui::{
    App, Context, FocusHandle, Focusable, KeyDownEvent, SharedString, Window, div, prelude::*,
};

use super::files::folder_name;
use super::state::AppState;
use super::window::{build_workspace, folder_prompt};
use crate::icons::{IconName, icon};
use crate::keymap::WORKSPACE_CONTEXT;
use crate::theme::UiTheme;
use crate::ui::{truncated, ui_theme};

pub struct Welcome {
    focus_handle: FocusHandle,
    recent: Vec<PathBuf>,
}

impl Focusable for Welcome {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Welcome {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle);
        Welcome {
            focus_handle,
            recent: AppState::recent_vaults(),
        }
    }

    /// Asks for a folder, then turns this window into its workspace.
    pub fn open_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(folder_prompt());
        cx.spawn_in(window, async move |_, cx| {
            let Ok(Ok(Some(paths))) = chosen.await else {
                return;
            };
            let Some(vault) = paths.into_iter().next() else {
                return;
            };
            cx.update(|window, cx| open_here(vault, window, cx)).ok();
        })
        .detach();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
            self.open_folder(window, cx);
            cx.stop_propagation();
        }
    }

    fn render_recent(&self, ui: &UiTheme) -> Option<impl IntoElement> {
        if self.recent.is_empty() {
            return None;
        }
        let rows = self.recent.iter().enumerate().map(|(index, vault)| {
            let target = vault.clone();
            div()
                .id(("recent-vault", index))
                .flex()
                .flex_row()
                .items_center()
                .gap(ui.space_md + ui.space_xs)
                .h(ui.row_height)
                .px(ui.row_padding_x)
                .rounded(ui.row_radius)
                .hover(|style| style.bg(ui.row_hover))
                .on_click(move |_, window, cx| open_here(target.clone(), window, cx))
                .child(
                    icon(IconName::Folder)
                        .flex_none()
                        .size(ui.icon_size - gpui::px(2.))
                        .text_color(ui.icon),
                )
                .child(
                    div()
                        .flex_none()
                        .text_color(ui.text)
                        .child(folder_name(vault)),
                )
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .text_size(ui.small_font_size)
                        .text_color(ui.text_detail)
                        .child(truncated(SharedString::from(parent_label(vault))).grow()),
                )
        });
        Some(
            div()
                .flex()
                .flex_col()
                .w_full()
                .gap(ui.space_xs)
                .child(
                    div()
                        .px(ui.row_padding_x)
                        .pb(ui.space_sm)
                        .text_color(ui.text_detail)
                        .child("Recent vaults"),
                )
                .children(rows),
        )
    }
}

/// Turns this window into `vault`'s workspace.
fn open_here(vault: PathBuf, window: &mut Window, cx: &mut App) {
    let workspace = window.replace_root(cx, |window, cx| build_workspace(&vault, None, window, cx));
    workspace.update(cx, |workspace, cx| {
        AppState::remember_vault(workspace.vault());
        workspace.focus_active(window, cx);
    });
}

/// The folder a vault is in, with the home folder as `~`.
fn parent_label(vault: &Path) -> String {
    let parent = vault.parent().unwrap_or(vault);
    match dirs::home_dir().and_then(|home| parent.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => parent.display().to_string(),
    }
}

impl Render for Welcome {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = ui_theme(cx);
        let focused = self.focus_handle.is_focused(window);
        div()
            .key_context(WORKSPACE_CONTEXT)
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .px(ui.space_xl)
            .bg(ui.app_background)
            .font_family(ui.font_family.clone())
            .text_size(ui.font_size)
            .text_color(ui.text)
            .relative()
            .child(crate::ui::focus_visible::pointer_watch())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(ui.space_xl)
                    .w(ui.small_dialog_width)
                    .max_w_full()
                    .child(
                        div()
                            .text_size(ui.font_size + gpui::px(4.))
                            .child("Pick a folder of notes to start."),
                    )
                    .child(
                        div()
                            .id("open-folder")
                            .track_focus(&self.focus_handle)
                            .on_key_down(cx.listener(Self::on_key_down))
                            .on_click(
                                cx.listener(|welcome, _, window, cx| {
                                    welcome.open_folder(window, cx)
                                }),
                            )
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(ui.space_md)
                            .h(ui.row_height)
                            .px(ui.space_xl)
                            .rounded(ui.icon_button_radius)
                            .bg(ui.accent)
                            .text_color(ui.on_accent)
                            .hover(|style| style.bg(ui.accent.opacity(0.85)))
                            .when(crate::ui::focus_visible::ring(focused, cx), |button| {
                                button.shadow(vec![ui.focus()])
                            })
                            .child(
                                icon(IconName::FolderOpen)
                                    .size(ui.icon_size - gpui::px(2.))
                                    .text_color(ui.on_accent),
                            )
                            .child("Open a folder"),
                    )
                    .children(
                        self.render_recent(&ui)
                            .map(|recent| div().pt(ui.space_xl).w_full().child(recent)),
                    ),
            )
    }
}
