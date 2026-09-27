//! The bar above a pane's note: back and forward, the note's folder path
//! (a click shows that folder in the file tree), the reading-view button
//! and the `⋯` menu.

use std::path::{Path, PathBuf};

use gpui::{AnyElement, Context, IntoElement, SharedString, div, prelude::*};

use super::files::note_title;
use super::pane::{Pane, PaneEvent, PaneMenu};
use crate::icons::IconName;
use crate::ui::{Breadcrumbs, Crumb, IconButton, MenuAnchor, ui_theme};

/// The `⋯` button, which its menu hangs under.
pub const MORE_KEY: &str = "pane-more";

/// The command the reading-view button runs.
pub const READING_COMMAND: &str = "markdown.cycle-symbols";

/// The note's folders from the vault down, then the note: each folder with
/// its absolute path.
pub fn note_trail(vault: &Path, note: &Path) -> (Vec<(String, PathBuf)>, String) {
    let relative = note.strip_prefix(vault).unwrap_or(note);
    let mut folders = Vec::new();
    let mut at = vault.to_path_buf();
    if let Some(parent) = relative.parent() {
        for part in parent.components() {
            at = at.join(part);
            folders.push((part.as_os_str().to_string_lossy().into_owned(), at.clone()));
        }
    }
    (folders, note_title(note))
}

impl Pane {
    pub(super) fn render_note_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = ui_theme(cx);
        let is_note = self.active_tab().is_some_and(|tab| tab.note().is_some());
        let run = |id: &'static str| {
            cx.listener(
                move |_: &mut Pane, _: &gpui::ClickEvent, _, cx: &mut Context<Pane>| {
                    cx.emit(PaneEvent::Run(id.into()))
                },
            )
        };
        let back = IconButton::new("pane-back", IconName::ArrowLeft)
            .command("history.back", cx)
            .disabled(!self.can_go_back())
            .on_click(run("history.back"));
        let forward = IconButton::new("pane-forward", IconName::ArrowRight)
            .command("history.forward", cx)
            .disabled(!self.can_go_forward())
            .on_click(run("history.forward"));
        let side = || {
            div()
                .flex()
                .flex_row()
                .flex_1()
                .flex_basis(gpui::px(0.))
                .items_center()
        };
        let actions = if is_note {
            self.render_note_actions(cx)
        } else {
            Vec::new()
        };
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(ui.space_md)
            .h(ui.note_header_height)
            .px(ui.space_md)
            .child(side().gap(ui.space_xs).child(back).child(forward))
            .children(self.render_trail(cx))
            .child(side().justify_end().gap(ui.space_xs).children(actions))
    }

    fn render_trail(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let path = self.active_tab()?.path(cx)?.to_path_buf();
        let (folders, title) = note_trail(&self.vault, &path);
        let pane = cx.entity().downgrade();
        let mut crumbs: Vec<Crumb> = folders
            .into_iter()
            .map(|(name, folder)| {
                let pane = pane.clone();
                Crumb::new(name).on_click(move |_, cx| {
                    let folder = folder.clone();
                    pane.update(cx, |_, cx| cx.emit(PaneEvent::Reveal(folder)))
                        .ok();
                })
            })
            .collect();
        crumbs.push(Crumb::new(SharedString::from(title)));
        Some(
            div()
                .flex()
                .flex_none()
                .max_w(gpui::relative(0.6))
                .min_w_0()
                .child(Breadcrumbs::new("pane-trail", crumbs))
                .into_any_element(),
        )
    }

    fn render_note_actions(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let ui = ui_theme(cx);
        let reading = self.is_reading(cx);
        let (reading_icon, reading_label) = if reading {
            (IconName::PencilSimple, "Editing view")
        } else {
            (IconName::BookOpen, "Reading view")
        };
        let more_menu = self.menu.render_attached(MORE_KEY, ui.space_xs);
        vec![
            IconButton::new("pane-reading", reading_icon)
                .command(READING_COMMAND, cx)
                .label(reading_label)
                .active(reading)
                .on_click(
                    cx.listener(|_, _, _, cx| cx.emit(PaneEvent::Run(READING_COMMAND.into()))),
                )
                .into_any_element(),
            IconButton::new(MORE_KEY, IconName::DotsThree)
                .tooltip("More options")
                .active(more_menu.is_some())
                .on_click(cx.listener(|_, _, _, cx| {
                    cx.emit(PaneEvent::OpenMenu(
                        PaneMenu::More,
                        MenuAnchor::Below {
                            key: MORE_KEY.into(),
                            align_right: true,
                        },
                    ))
                }))
                .attach(more_menu)
                .into_any_element(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_trail_lists_folders_from_the_vault_down() {
        let vault = Path::new("/vault");
        let (folders, title) = note_trail(vault, Path::new("/vault/Essays/Drafts/Idea.md"));
        let names: Vec<&str> = folders.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["Essays", "Drafts"]);
        assert_eq!(folders[1].1, Path::new("/vault/Essays/Drafts"));
        assert_eq!(title, "Idea");
        let (folders, _) = note_trail(vault, Path::new("/vault/Top.md"));
        assert!(folders.is_empty());
    }
}
