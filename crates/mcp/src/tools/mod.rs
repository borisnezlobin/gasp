//! Every tool the server offers, grouped by what it works on.

mod app;
mod attachments;
mod config;
pub mod files;
mod links;
mod notes;
mod render;

use crate::tool::ToolSpec;

/// Every tool, in the order `tools/list` gives them.
pub fn all() -> Vec<ToolSpec> {
    [
        notes::tools(),
        attachments::tools(),
        links::tools(),
        config::tools(),
        render::tools(),
        app::tools(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

#[cfg(test)]
pub(crate) mod testing {
    //! Tempdir vaults and calling tools by name, for the tools' tests.

    use serde_json::Value;

    use crate::context::Context;
    use crate::tool::{Output, ToolResult};

    /// A vault holding `files` (path, text), with no app.
    pub fn vault(files: &[(&str, &str)]) -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let context = Context::for_tests(dir.path());
        (dir, context)
    }

    pub fn run(context: &Context, name: &str, arguments: Value) -> ToolResult {
        let tools = super::all();
        let tool = tools
            .iter()
            .find(|tool| tool.name == name)
            .unwrap_or_else(|| panic!("no tool {name}"));
        tool.call(context, arguments)
    }

    /// A tool's JSON output; panics on anything else.
    pub fn call(context: &Context, name: &str, arguments: Value) -> Value {
        match run(context, name, arguments) {
            Ok(Output::Json(value)) => value,
            other => panic!("{name} gave {other:?}"),
        }
    }

    /// A tool's text output; panics on anything else.
    pub fn text(context: &Context, name: &str, arguments: Value) -> String {
        match run(context, name, arguments) {
            Ok(Output::Text(text)) => text,
            other => panic!("{name} gave {other:?}"),
        }
    }

    /// A tool's error message; panics if it succeeded.
    pub fn call_err(context: &Context, name: &str, arguments: Value) -> String {
        match run(context, name, arguments) {
            Err(error) => error.message().to_string(),
            Ok(output) => panic!("{name} should have failed, gave {output:?}"),
        }
    }
}
