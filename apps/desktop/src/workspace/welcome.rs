//! The empty state before any vault is open: open a folder of notes, make
//! a new one, or reopen a vault this device opened before. The keyboard
//! walks the choices with the arrows or Tab, and Enter or Space takes one;
//! "Open a folder" has it first.

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
use crate::ui::{Selectable, truncated, ui_theme};

/// The name the save panel suggests for a new vault.
pub const NEW_VAULT_NAME: &str = "Notes";

/// One thing the welcome screen offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WelcomeChoice {
    OpenFolder,
    NewVault,
    Recent(PathBuf),
}

pub struct Welcome {
    focus_handle: FocusHandle,
    recent: Vec<PathBuf>,
    selected: usize,
    _notices: gpui::Subscription,
}

impl Focusable for Welcome {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Welcome {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_recent(AppState::recent_vaults(), window, cx)
    }

    /// The welcome screen listing `recent` as the vaults opened before.
    pub fn with_recent(recent: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle);
        Welcome {
            focus_handle,
            recent,
            selected: 0,
            _notices: crate::notices::observe(cx),
        }
    }

    /// Every choice, in the order the keyboard walks them.
    pub fn choices(&self) -> Vec<WelcomeChoice> {
        let mut choices = vec![WelcomeChoice::OpenFolder, WelcomeChoice::NewVault];
        choices.extend(self.recent.iter().cloned().map(WelcomeChoice::Recent));
        choices
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Asks for a folder, then turns this window into its workspace.
    pub fn open_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chosen = crate::sandbox::prompt_for_paths(folder_prompt(), cx);
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

    /// Asks where a new vault goes and what it's called, makes the
    /// folder and opens it.
    pub fn new_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let start = dirs::document_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_default();
        let chosen = crate::sandbox::prompt_for_new_path(&start, Some(NEW_VAULT_NAME), cx);
        cx.spawn_in(window, async move |_, cx| {
            let Ok(Ok(Some(vault))) = chosen.await else {
                return;
            };
            cx.update(|window, cx| match std::fs::create_dir_all(&vault) {
                Ok(()) => open_here(vault, window, cx),
                Err(error) => {
                    let message = format!("Couldn’t make the vault’s folder: {error}");
                    crate::notices::problem(message, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn take(&mut self, choice: WelcomeChoice, window: &mut Window, cx: &mut Context<Self>) {
        match choice {
            WelcomeChoice::OpenFolder => self.open_folder(window, cx),
            WelcomeChoice::NewVault => self.new_vault(window, cx),
            WelcomeChoice::Recent(vault) => open_here(vault, window, cx),
        }
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.choices().len() as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(count) as usize;
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let back = keystroke.modifiers.shift;
        match keystroke.key.as_str() {
            "enter" | "space" => {
                let choice = self.choices()[self.selected].clone();
                self.take(choice, window, cx);
            }
            "tab" => self.step(if back { -1 } else { 1 }, cx),
            "down" | "right" => self.step(1, cx),
            "up" | "left" => self.step(-1, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn is_ringed(&self, index: usize, window: &Window, cx: &App) -> bool {
        let focused = self.focus_handle.is_focused(window) && self.selected == index;
        crate::ui::focus_visible::ring(focused, cx)
    }

    fn render_actions(&self, ui: &UiTheme, window: &Window, cx: &mut Context<Self>) -> gpui::Div {
        let open = action_button("open-folder", IconName::FolderOpen, "Open a folder", ui)
            .bg(ui.accent)
            .text_color(ui.on_accent)
            .hover(|style| style.bg(ui.accent.opacity(0.85)))
            .when(self.is_ringed(0, window, cx), |button| {
                button.shadow(vec![ui.focus()])
            })
            .on_click(cx.listener(|welcome, _, window, cx| welcome.open_folder(window, cx)));
        let new = action_button("new-vault", IconName::FolderPlus, "Make a new vault", ui)
            .bg(ui.button_background)
            .hover(|style| style.bg(ui.control_pressed))
            .when(self.is_ringed(1, window, cx), |button| {
                button.shadow(vec![ui.focus()])
            })
            .on_click(cx.listener(|welcome, _, window, cx| welcome.new_vault(window, cx)));
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .justify_center()
            .gap(ui.space_md)
            .child(open)
            .child(new)
    }

    fn render_recent(&self, ui: &UiTheme, window: &Window) -> Option<impl IntoElement> {
        if self.recent.is_empty() {
            return None;
        }
        let rows: Vec<_> = self
            .recent
            .iter()
            .enumerate()
            .map(|(index, vault)| self.render_recent_row(index, vault, ui, window))
            .collect();
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

    fn render_recent_row(
        &self,
        index: usize,
        vault: &Path,
        ui: &UiTheme,
        window: &Window,
    ) -> impl IntoElement {
        let target = vault.to_path_buf();
        let choice = index + 2;
        let selected = self.focus_handle.is_focused(window) && self.selected == choice;
        div()
            .id(("recent-vault", index))
            .selector(move || format!("recent-vault-{index}"))
            .flex()
            .flex_row()
            .items_center()
            .gap(ui.space_md + ui.space_xs)
            .h(ui.row_height)
            .px(ui.row_padding_x)
            .rounded(ui.row_radius)
            .when(selected, |row| row.bg(ui.row_selected))
            .when(!selected, |row| row.hover(|style| style.bg(ui.row_hover)))
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
    }
}

/// A welcome button: an icon and a verb, the height of a list row.
fn action_button(
    id: &'static str,
    glyph: IconName,
    label: &'static str,
    ui: &UiTheme,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .selector(move || id.to_string())
        .flex()
        .flex_row()
        .items_center()
        .gap(ui.space_md)
        .h(ui.row_height)
        .px(ui.space_xl)
        .rounded(ui.icon_button_radius)
        .child(icon(glyph).size(ui.icon_size - gpui::px(2.)))
        .child(label)
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
    super::files::display_path(vault.parent().unwrap_or(vault))
}

impl Render for Welcome {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = ui_theme(cx);
        let actions = self.render_actions(&ui, window, cx);
        let recent = self
            .render_recent(&ui, window)
            .map(|recent| div().pt(ui.space_xl).w_full().child(recent));
        div()
            .key_context(WORKSPACE_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
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
                    .child(actions)
                    .children(recent),
            )
            .children(crate::notices::render(window, ui.space_xl, cx))
    }
}
