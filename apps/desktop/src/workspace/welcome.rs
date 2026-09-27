//! The empty state before any vault is open: one action, "Open a folder",
//! which has focus so Enter or Space runs it.

use gpui::{App, Context, FocusHandle, Focusable, KeyDownEvent, Window, div, prelude::*};

use super::state::AppState;
use super::window::{build_workspace, folder_prompt};
use crate::icons::{IconName, icon};
use crate::keymap::WORKSPACE_CONTEXT;
use crate::theme::Theme;

pub struct Welcome {
    focus_handle: FocusHandle,
    theme: Theme,
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
            theme: Theme::default(),
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
            cx.update(|window, cx| {
                let workspace =
                    window.replace_root(cx, |window, cx| build_workspace(&vault, None, window, cx));
                workspace.update(cx, |workspace, cx| {
                    AppState::remember_vault(workspace.vault());
                    workspace.focus_active(window, cx);
                });
            })
            .ok();
        })
        .detach();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
            self.open_folder(window, cx);
            cx.stop_propagation();
        }
    }
}

impl Render for Welcome {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme.workspace;
        let focused = self.focus_handle.is_focused(window);
        div()
            .key_context(WORKSPACE_CONTEXT)
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(theme.space_xl)
            .bg(self.theme.background)
            .font_family(self.theme.body_font_family)
            .text_size(theme.ui_font_size)
            .text_color(theme.text_muted)
            .child("Pick a folder of notes to start.")
            .child(
                div()
                    .id("open-folder")
                    .track_focus(&self.focus_handle)
                    .on_key_down(cx.listener(Self::on_key_down))
                    .on_click(cx.listener(|welcome, _, window, cx| welcome.open_folder(window, cx)))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(theme.space_md)
                    .px(theme.space_xl)
                    .py(theme.space_md)
                    .rounded(theme.radius_md)
                    .bg(theme.accent)
                    .text_color(theme.on_accent)
                    .when(focused, |button| {
                        button.shadow(vec![gpui::BoxShadow {
                            color: theme.text_faint,
                            offset: gpui::point(gpui::px(0.), gpui::px(0.)),
                            blur_radius: gpui::px(0.),
                            spread_radius: theme.focus_line_width,
                        }])
                    })
                    .child(
                        icon(IconName::FolderOpen)
                            .size(theme.icon_size)
                            .text_color(theme.on_accent),
                    )
                    .child("Open a folder"),
            )
    }
}
