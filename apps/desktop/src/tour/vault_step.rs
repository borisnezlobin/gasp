//! Choosing where notes live. Making a new vault is the one big choice;
//! opening a folder that's already there (an Obsidian vault works as it
//! is) and trying the sample vault sit under it, with vaults this Mac
//! opened before after them.
//!
//! A new vault, or a folder that doesn't sync yet, goes on to the sync
//! step. The sample, a recent vault and a folder that already syncs open
//! at once.

use std::path::{Path, PathBuf};

use gpui::{AnyElement, App, Context, SharedString, Window, div, prelude::*, px};

use super::sketch;
use super::{AfterOpening, Step, Tour, choice_icon, explanation, heading, stage};
use crate::icons::IconName;
use crate::theme::UiTheme;
use crate::ui::{Selectable, truncated, ui_theme};
use crate::workspace::files::{display_path, folder_name};
use crate::workspace::state::AppState;
use crate::workspace::window::{build_workspace, folder_prompt};

/// The name the save panel suggests for a new vault.
pub const NEW_VAULT_NAME: &str = "Notes";

/// Recent vaults the step lists at most.
const MAX_RECENT: usize = 4;

/// One way to a vault, in the order the keyboard walks them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VaultChoice {
    NewVault,
    OpenFolder,
    Sample,
    Recent(PathBuf),
}

impl Tour {
    pub fn vault_choices(&self) -> Vec<VaultChoice> {
        let mut choices = vec![
            VaultChoice::NewVault,
            VaultChoice::OpenFolder,
            VaultChoice::Sample,
        ];
        choices.extend(
            self.recent
                .iter()
                .take(MAX_RECENT)
                .cloned()
                .map(VaultChoice::Recent),
        );
        choices
    }

    pub fn take_vault_choice(
        &mut self,
        choice: VaultChoice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match choice {
            VaultChoice::NewVault => self.ask_for_new_vault(window, cx),
            VaultChoice::OpenFolder => self.ask_for_folder(window, cx),
            VaultChoice::Sample => self.open_sample(window, cx),
            VaultChoice::Recent(vault) => self.open_vault(vault, AfterOpening::Nothing, window, cx),
        }
    }

    /// Goes on with `vault`: to the sync step when it doesn't sync yet,
    /// or straight in when it does.
    pub fn chose_vault(&mut self, vault: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let syncs = vault.join(".git").exists();
        let has_sync_step = self.steps.contains(&Step::Sync);
        if syncs || !has_sync_step {
            self.open_vault(vault, AfterOpening::Nothing, window, cx);
            return;
        }
        self.vault = Some(vault);
        self.advance(window, cx);
    }

    /// Asks where the new vault goes. A snapshot run, which can't show
    /// the save panel, makes it in its own Documents folder instead.
    fn ask_for_new_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let start = crate::sandbox::documents_folder();
        if crate::sandbox::is_active() {
            let vault = start.join(NEW_VAULT_NAME);
            if std::fs::create_dir_all(&vault).is_ok() {
                self.chose_vault(vault, window, cx);
            }
            return;
        }
        let chosen = crate::sandbox::prompt_for_new_path(&start, Some(NEW_VAULT_NAME), cx);
        cx.spawn_in(window, async move |tour, cx| {
            let Ok(Ok(Some(vault))) = chosen.await else {
                return;
            };
            tour.update_in(cx, |tour, window, cx| {
                match std::fs::create_dir_all(&vault) {
                    Ok(()) => tour.chose_vault(vault, window, cx),
                    Err(error) => {
                        let message = format!("Couldn’t make the vault’s folder: {error}");
                        crate::notices::problem(message, cx);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    fn ask_for_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chosen = crate::sandbox::prompt_for_paths(folder_prompt(), cx);
        cx.spawn_in(window, async move |tour, cx| {
            let Ok(Ok(Some(paths))) = chosen.await else {
                return;
            };
            let Some(vault) = paths.into_iter().next() else {
                return;
            };
            tour.update_in(cx, |tour, window, cx| tour.chose_vault(vault, window, cx))
                .ok();
        })
        .detach();
    }

    fn open_sample(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match super::write_sample_vault(&crate::sandbox::documents_folder()) {
            Ok(vault) => {
                let first = vault.join(super::FIRST_NOTE);
                self.open_vault(vault, AfterOpening::OpenNote(first), window, cx);
            }
            Err(error) => {
                let message = format!("Couldn’t make the sample vault: {error}");
                crate::notices::problem(message, cx);
            }
        }
    }
}

/// Turns this window into `vault`'s workspace, then does `after`.
pub fn open_here(vault: PathBuf, after: AfterOpening, window: &mut Window, cx: &mut App) {
    let note = match &after {
        AfterOpening::OpenNote(note) => Some(note.clone()),
        _ => None,
    };
    let workspace = window.replace_root(cx, |window, cx| {
        build_workspace(&vault, note.as_deref(), window, cx)
    });
    workspace.update(cx, |workspace, cx| {
        AppState::remember_vault(workspace.vault());
        workspace.focus_active(window, cx);
        if after == AfterOpening::SetUpSync {
            workspace.run_command("sync.set-up", window, cx);
        }
    });
}

pub fn render(tour: &Tour, window: &Window, cx: &mut Context<Tour>) -> AnyElement {
    let ui = ui_theme(cx);
    let focused = tour.focus_handle.is_focused(window);
    let choices: Vec<AnyElement> = tour
        .vault_choices()
        .into_iter()
        .enumerate()
        .map(|(index, choice)| {
            let ringed = crate::ui::focus_visible::ring(focused && tour.selected == index, cx);
            choice_row(index, choice, ringed, cx)
        })
        .collect();
    let mut choices = choices.into_iter();
    let first = choices.next();
    let content = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(ui.space_xl * 3.)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(ui.space_md)
                .child(heading(Step::Vault, &ui))
                .child(explanation(
                    "A vault is a folder of Markdown files. Gasp reads and writes the files in it and nothing else.",
                    &ui,
                ))
                .child(div().h(ui.space_xl))
                .child(div().flex().flex_row().children(first))
                .child(div().h(ui.space_lg))
                .children(choices),
        )
        .child(div().flex_none().child(sketch::folder(px(260.), &ui)));
    stage(px(900.), &ui, content)
}

fn choice_row(
    index: usize,
    choice: VaultChoice,
    ringed: bool,
    cx: &mut Context<Tour>,
) -> AnyElement {
    let ui = ui_theme(cx);
    let (glyph, label, detail) = describe(&choice);
    let big = choice == VaultChoice::NewVault;
    let row = div()
        .id(("tour-vault", index))
        .selector(move || format!("tour-vault-{index}"))
        .flex()
        .flex_row()
        .items_center()
        .gap(ui.space_md + ui.space_xs)
        .px(ui.row_padding_x)
        .rounded(ui.row_radius)
        .when(ringed, |row| row.shadow(vec![ui.focus()]))
        .on_click(cx.listener(move |tour, _, window, cx| {
            tour.selected = index;
            tour.take_choice(index, window, cx);
        }));
    let row = if big {
        row.h(ui.row_height * 1.6)
            .px(ui.space_xl)
            .bg(ui.accent)
            .text_color(ui.on_accent)
            .hover(move |style| style.bg(ui.accent.opacity(0.85)))
            .child(
                crate::icons::icon(glyph)
                    .flex_none()
                    .size(ui.icon_size * 1.25)
                    .text_color(ui.on_accent),
            )
            .child(div().text_size(ui.font_size * 1.1).child(label))
    } else {
        row.h(ui.row_height)
            .hover(move |style| style.bg(ui.row_hover))
            .child(choice_icon(glyph, &ui))
            .child(div().flex_none().child(label))
            .children(detail.map(|detail| detail_text(detail, &ui)))
    };
    row.into_any_element()
}

/// A choice's icon, what it says, and the quieter words after it.
fn describe(choice: &VaultChoice) -> (IconName, SharedString, Option<SharedString>) {
    match choice {
        VaultChoice::NewVault => (IconName::FolderPlus, "Make a new vault".into(), None),
        VaultChoice::OpenFolder => (
            IconName::FolderOpen,
            "Open a folder".into(),
            Some("Obsidian vaults work as they are".into()),
        ),
        VaultChoice::Sample => (
            IconName::BookOpen,
            "Try the sample vault".into(),
            Some("A few notes that show what Gasp does".into()),
        ),
        VaultChoice::Recent(vault) => (
            IconName::Folder,
            folder_name(vault).into(),
            Some(parent_label(vault).into()),
        ),
    }
}

fn detail_text(detail: SharedString, ui: &UiTheme) -> impl IntoElement {
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .text_size(ui.small_font_size)
        .text_color(ui.text_detail)
        .child(truncated(detail).grow())
}

/// The folder a vault is in, with the home folder as `~`.
fn parent_label(vault: &Path) -> String {
    display_path(vault.parent().unwrap_or(vault))
}
