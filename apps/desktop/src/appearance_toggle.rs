//! `view.toggle-dark-mode`: switches the app to whichever of light and
//! dark it isn't showing, by writing `appearance.theme`, so the settings
//! screen shows the choice and "Match system" puts it back.

use gpui::{Context, Window};
use serde_json::Value;

use crate::settings_view::store::write_setting;
use crate::workspace::Workspace;

pub const COMMAND: &str = "view.toggle-dark-mode";
const THEME_KEY: &str = "appearance.theme";

pub fn install(workspace: &mut Workspace) {
    workspace.on_command(COMMAND, toggle_dark_mode);
}

/// The `appearance.theme` value that shows the other mode.
pub fn opposite_theme(dark_now: bool) -> &'static str {
    if dark_now { "light" } else { "dark" }
}

fn toggle_dark_mode(workspace: &mut Workspace, _: &mut Window, cx: &mut Context<Workspace>) {
    let next = opposite_theme(crate::ui::is_dark(cx));
    let written = write_setting(
        workspace.vault(),
        THEME_KEY,
        Some(&Value::from(next)),
        &Value::from("match-system"),
    );
    match written {
        Ok(_) => workspace.reload_config(cx),
        Err(error) => {
            crate::notices::problem(format!("Couldn’t switch to {next} mode: {error}"), cx);
        }
    }
}
