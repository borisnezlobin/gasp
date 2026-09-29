//! The workspace's prose commands, and starting the grammar checker on a
//! vault.

use gasp_config::settings::SentenceLengthSettings;
use gpui::{Context, Window};
use serde_json::Value;

use crate::settings_view::store::write_setting;
use crate::workspace::Workspace;

/// The setting `prose.toggle-sentence-highlighting` flips.
pub const SENTENCE_LENGTH_KEY: &str = "prose.sentence-length.enabled";

/// Wires the prose commands into `workspace` and has the checker learn
/// its vault.
pub fn install(workspace: &mut Workspace, cx: &mut Context<Workspace>) {
    workspace.on_command("prose.toggle-sentence-highlighting", toggle_sentence_length);
    let settings = workspace.config().settings.prose.grammar.clone();
    super::checker::configure(workspace.vault(), &settings, cx);
    if settings.enabled {
        let texts = workspace.note_texts().clone();
        super::checker::open_vault(workspace.vault(), texts, cx);
    }
}

/// Turns sentence-length highlighting on or off by writing the setting,
/// so the settings screen, every open note and other devices agree.
fn toggle_sentence_length(workspace: &mut Workspace, _: &mut Window, cx: &mut Context<Workspace>) {
    let on = workspace.config().settings.prose.sentence_length.enabled;
    let default = Value::from(SentenceLengthSettings::default().enabled);
    match write_setting(
        workspace.vault(),
        SENTENCE_LENGTH_KEY,
        Some(&Value::from(!on)),
        &default,
    ) {
        Ok(_) => workspace.reload_config(cx),
        Err(error) => {
            let message = format!("Couldn’t change sentence-length highlighting: {error}");
            crate::notices::problem(message, cx);
        }
    }
}
