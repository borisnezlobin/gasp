//! Tools that need the running app: what it shows, running its commands
//! and opening notes. Each fails with a clear message when no app has
//! the vault open.

use gasp_config::config_files::known_commands;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use super::files::app_error;
use crate::bridge::{EditorState, Request};
use crate::context::Context;
use crate::tool::{NoArguments, Output, ToolError, ToolResult, ToolSpec};

pub fn tools() -> Vec<ToolSpec> {
    vec![
        ToolSpec::reads(
            "editor_state",
            "What the running app shows: its panes and their tabs (path, title, unsaved \
             edits), the active note, and the cursor there (byte offset, 1-based line and \
             column) with the selected text. Needs the app open on this vault.",
            editor_state,
        ),
        ToolSpec::writes(
            "run_command",
            "Run one of the app's commands by id, as the palette does, in the active pane: \
             such as `tab.new`, `pane.split-right` or `format.bold` (which acts on the \
             selection). list_commands gives the ids. Needs the app open on this vault.",
            run_command,
        ),
        ToolSpec::writes(
            "open_note",
            "Open a note in the running app's active pane, optionally with the cursor at a \
             line (1-based). Needs the app open on this vault.",
            open_note,
        ),
    ]
}

fn editor_state(context: &Context, _: NoArguments) -> ToolResult {
    let state: EditorState = context.app().call_as(Request::State).map_err(app_error)?;
    let value = serde_json::to_value(state).map_err(|error| ToolError::new(error.to_string()))?;
    Ok(Output::Json(value))
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RunCommand {
    /// The command's id, such as `tab.new`.
    id: String,
}

fn run_command(context: &Context, args: RunCommand) -> ToolResult {
    let id = args.id.trim();
    let known = known_commands().contains(&id) || id.starts_with("tab.go-");
    if !known {
        return Err(ToolError::new(format!(
            "there's no command {id:?}; list_commands gives the ids"
        )));
    }
    let request = Request::RunCommand { id: id.to_string() };
    context.app().call(request).map_err(app_error)?;
    Ok(Output::Text(format!("Ran {id}.")))
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OpenNote {
    /// The note, relative to the vault; `.md` may be left off.
    path: String,
    /// The line to put the cursor on, counting from 1.
    #[serde(default)]
    line: Option<usize>,
}

fn open_note(context: &Context, args: OpenNote) -> ToolResult {
    let note = context.resolve_note(&args.path)?;
    if !note.absolute.is_file() {
        return Err(ToolError::new(format!("{} doesn't exist", note.relative)));
    }
    let request = Request::OpenNote {
        path: note.relative.clone(),
        line: args.line,
    };
    let result = context.app().call(request).map_err(app_error)?;
    Ok(Output::Json(
        json!({ "opened": note.relative, "result": result }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::testing::{call_err, vault};

    #[test]
    fn without_the_app_the_tools_say_so() {
        let (_dir, context) = vault(&[("A.md", "a")]);
        for (tool, args) in [
            ("editor_state", json!({})),
            ("run_command", json!({"id": "tab.new"})),
            ("open_note", json!({"path": "A"})),
        ] {
            let error = call_err(&context, tool, args);
            assert!(error.contains("isn't running"), "{tool}: {error}");
        }
        let error = call_err(&context, "run_command", json!({"id": "no.such"}));
        assert!(error.contains("no command"));
        let error = call_err(&context, "open_note", json!({"path": "Gone"}));
        assert!(error.contains("doesn't exist"));
    }
}
