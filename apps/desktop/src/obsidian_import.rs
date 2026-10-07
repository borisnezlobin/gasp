//! `vault.import-obsidian`: reads an Obsidian vault's `.obsidian` folder
//! (Latex Suite snippets, typing replacements, hotkeys, app settings and
//! the look) into this vault's config with `gasp-migrate`, keeping what's
//! already set, and opens the migration report beside the notes.
//!
//! A vault with a `.obsidian` folder and no config of its own is offered
//! the import once, in a notice, when it first opens.

use std::path::{Path, PathBuf};

use gasp_config::CONFIG_DIR;
use gasp_migrate::import::{ImportOutcome, import_into};
use gasp_migrate::{
    REPLACEMENTS_FILE, REPORT_FILE, RULES_FILE, SETTINGS_FILE, SNIPPETS_FILE, THEME_FILE,
};
use gpui::{AppContext, Context, Window};

use crate::notices::{self, Notice};
use crate::workspace::{OpenIn, Workspace};

pub const IMPORT_COMMAND: &str = "vault.import-obsidian";
const OBSIDIAN_DIR: &str = ".obsidian";

/// Wires the command, and offers the import when the vault looks fresh
/// from Obsidian.
pub fn install(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    workspace.on_command(IMPORT_COMMAND, import_command);
    let offered = workspace.config().device.obsidian_import_offered;
    if !offered && should_offer(workspace.vault()) {
        workspace.mark_obsidian_import_offered(cx);
        let notice = Notice::offer("This vault has settings from Obsidian.")
            .with_action("Import them", IMPORT_COMMAND);
        notices::show_in(notice, window, cx);
    }
}

/// Whether `vault` has Obsidian's settings and none of its own yet.
pub fn should_offer(vault: &Path) -> bool {
    let config = vault.join(CONFIG_DIR);
    vault.join(OBSIDIAN_DIR).is_dir()
        && [SNIPPETS_FILE, REPLACEMENTS_FILE, RULES_FILE, SETTINGS_FILE]
            .iter()
            .all(|name| !config.join(name).exists())
}

/// The `.obsidian` folder in or at `chosen`.
fn obsidian_folder(chosen: &Path) -> Option<PathBuf> {
    if chosen.file_name().is_some_and(|name| name == OBSIDIAN_DIR) && chosen.is_dir() {
        return Some(chosen.to_path_buf());
    }
    let inside = chosen.join(OBSIDIAN_DIR);
    inside.is_dir().then_some(inside)
}

fn import_command(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    if let Some(source) = obsidian_folder(workspace.vault()) {
        start_import(workspace, source, window, cx);
        return;
    }
    let chosen = crate::sandbox::prompt_for_paths(crate::workspace::window::folder_prompt(), cx);
    cx.spawn_in(window, async move |workspace, cx| {
        let Ok(Ok(Some(paths))) = chosen.await else {
            return;
        };
        let Some(chosen) = paths.into_iter().next() else {
            return;
        };
        workspace
            .update_in(cx, |workspace, window, cx| {
                match obsidian_folder(&chosen) {
                    Some(source) => start_import(workspace, source, window, cx),
                    None => {
                        let message = "That folder has no Obsidian settings in it. Pick the Obsidian vault itself.";
                        notices::problem(message, cx);
                    }
                }
            })
            .ok();
    })
    .detach();
}

/// Imports `source` off the main thread, then reloads the config.
pub fn start_import(
    workspace: &mut Workspace,
    source: PathBuf,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let config = workspace.vault().join(CONFIG_DIR);
    let importing = cx.background_spawn(async move { import_into(&source, &config) });
    cx.spawn_in(window, async move |workspace, cx| {
        let result = importing.await;
        workspace
            .update_in(cx, |workspace, window, cx| {
                finish_import(workspace, result, window, cx)
            })
            .ok();
    })
    .detach();
}

fn finish_import(
    workspace: &mut Workspace,
    result: Result<ImportOutcome, String>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => {
            notices::failed("Couldn’t import from Obsidian", error, cx);
            return;
        }
    };
    if outcome.changed() {
        crate::features::config_files_changed(workspace, cx);
    }
    let report = workspace.vault().join(CONFIG_DIR).join(REPORT_FILE);
    if let Err(error) = workspace.open_path(&report, OpenIn::NewTab, window, cx) {
        notices::open_failed(&report, error, cx);
    }
    notices::show(Notice::done(summary(&outcome)), cx);
}

/// What the import did, in a sentence.
pub fn summary(outcome: &ImportOutcome) -> String {
    let imported: Vec<&str> = outcome
        .written
        .iter()
        .chain(&outcome.merged)
        .filter_map(|name| describe(name))
        .collect();
    if imported.is_empty() {
        return "Your settings here already cover everything from Obsidian.".to_owned();
    }
    format!(
        "Imported {} from Obsidian. The report lists anything left behind.",
        list(&imported)
    )
}

fn describe(file: &str) -> Option<&'static str> {
    match file {
        SNIPPETS_FILE => Some("snippets"),
        REPLACEMENTS_FILE => Some("replacements"),
        RULES_FILE => Some("hotkeys"),
        SETTINGS_FILE => Some("settings"),
        THEME_FILE => Some("fonts"),
        _ => None,
    }
}

fn list(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_names_what_came_in() {
        let outcome = ImportOutcome {
            written: vec![SNIPPETS_FILE, RULES_FILE, REPORT_FILE],
            merged: vec![SETTINGS_FILE],
            kept: vec![REPLACEMENTS_FILE],
        };
        assert_eq!(
            summary(&outcome),
            "Imported snippets, hotkeys and settings from Obsidian. The report lists anything left behind."
        );
        assert_eq!(
            summary(&ImportOutcome::default()),
            "Your settings here already cover everything from Obsidian."
        );
    }

    #[test]
    fn a_vault_or_its_obsidian_folder_both_lead_to_the_settings() {
        let vault = tempfile::tempdir().unwrap();
        assert_eq!(obsidian_folder(vault.path()), None);
        std::fs::create_dir(vault.path().join(OBSIDIAN_DIR)).unwrap();
        let folder = vault.path().join(OBSIDIAN_DIR);
        assert_eq!(obsidian_folder(vault.path()), Some(folder.clone()));
        assert_eq!(obsidian_folder(&folder), Some(folder));
    }
}
