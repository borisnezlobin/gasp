//! What a tool is to the server: a name, a description, the schema of its
//! arguments and a function from those arguments to an [`Output`].
//!
//! Arguments are plain structs that derive `Deserialize` and `JsonSchema`,
//! so the schema a client sees and the parsing that checks it can't
//! drift apart. Tools run synchronously on a blocking thread.

use std::fmt;
use std::sync::Arc;

use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::context::Context;

/// Why a tool couldn't do what it was asked, in words for the agent. It
/// becomes a tool result with `isError` set, which the model reads,
/// rather than a protocol error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolError(String);

impl ToolError {
    pub fn new(message: impl Into<String>) -> ToolError {
        ToolError(message.into())
    }

    /// A file system error on `what`, such as a note's path.
    pub fn io(what: &str, error: &std::io::Error) -> ToolError {
        let reason = match error.kind() {
            std::io::ErrorKind::NotFound => "it doesn't exist".to_string(),
            std::io::ErrorKind::PermissionDenied => "permission denied".to_string(),
            _ => error.to_string(),
        };
        ToolError(format!("couldn't use {what}: {reason}"))
    }

    pub fn message(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for ToolError {
    fn from(message: String) -> ToolError {
        ToolError(message)
    }
}

/// What a tool gives back.
#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    /// A sentence, such as what was written where.
    Text(String),
    /// Data, sent as pretty-printed JSON text.
    Json(Value),
    /// An image, such as a rendered page, with a line saying what it
    /// shows.
    Image {
        data: Vec<u8>,
        mime: &'static str,
        caption: String,
    },
}

pub type ToolResult = Result<Output, ToolError>;

type Run = dyn Fn(&Context, Value) -> ToolResult + Send + Sync;

/// One tool.
pub struct ToolSpec {
    pub name: &'static str,
    description: &'static str,
    read_only: bool,
    schema: Arc<JsonObject>,
    run: Box<Run>,
}

impl ToolSpec {
    /// A tool that only reads.
    pub fn reads<A>(
        name: &'static str,
        description: &'static str,
        run: fn(&Context, A) -> ToolResult,
    ) -> ToolSpec
    where
        A: DeserializeOwned + JsonSchema + 'static,
    {
        ToolSpec::new(name, description, true, run)
    }

    /// A tool that changes the vault, its config or the app.
    pub fn writes<A>(
        name: &'static str,
        description: &'static str,
        run: fn(&Context, A) -> ToolResult,
    ) -> ToolSpec
    where
        A: DeserializeOwned + JsonSchema + 'static,
    {
        ToolSpec::new(name, description, false, run)
    }

    fn new<A>(
        name: &'static str,
        description: &'static str,
        read_only: bool,
        run: fn(&Context, A) -> ToolResult,
    ) -> ToolSpec
    where
        A: DeserializeOwned + JsonSchema + 'static,
    {
        let schema = rmcp::handler::server::common::schema_for_input::<A>()
            .unwrap_or_else(|_| rmcp::handler::server::common::schema_for_empty_input());
        ToolSpec {
            name,
            description,
            read_only,
            schema,
            run: Box::new(move |context, arguments| {
                let arguments = parse_arguments::<A>(arguments)?;
                run(context, arguments)
            }),
        }
    }

    /// The tool as `tools/list` describes it.
    pub fn definition(&self) -> Tool {
        let annotations = ToolAnnotations::from_raw(
            None,
            Some(self.read_only),
            Some(!self.read_only),
            None,
            Some(false),
        );
        Tool::new(self.name, self.description, self.schema.clone()).with_annotations(annotations)
    }

    pub fn call(&self, context: &Context, arguments: Value) -> ToolResult {
        (self.run)(context, arguments)
    }
}

/// Reads a tool's arguments; a missing object reads as `{}`.
fn parse_arguments<A: DeserializeOwned>(arguments: Value) -> Result<A, ToolError> {
    let arguments = match arguments {
        Value::Null => Value::Object(Default::default()),
        other => other,
    };
    serde_json::from_value(arguments)
        .map_err(|error| ToolError::new(format!("the arguments don't fit this tool: {error}")))
}

/// Arguments for a tool that takes none.
#[derive(Debug, Default, serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoArguments {}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, serde::Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct Example {
        /// Which note.
        path: String,
        #[serde(default)]
        limit: Option<usize>,
    }

    fn echo(_: &Context, example: Example) -> ToolResult {
        Ok(Output::Text(format!(
            "{} {:?}",
            example.path, example.limit
        )))
    }

    #[test]
    fn the_schema_comes_from_the_arguments() {
        let spec = ToolSpec::reads("echo", "Echoes.", echo);
        let tool = spec.definition();
        let schema = Value::Object((*tool.input_schema).clone());
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["path"]["description"], "Which note.");
        assert_eq!(schema["required"], serde_json::json!(["path"]));
        assert_eq!(tool.annotations.unwrap().read_only_hint, Some(true));
    }

    #[test]
    fn bad_arguments_are_a_readable_tool_error() {
        let spec = ToolSpec::reads("echo", "Echoes.", echo);
        let context = Context::for_tests(std::path::Path::new("/"));
        let error = spec.call(&context, serde_json::json!({})).unwrap_err();
        assert!(error.message().contains("missing field `path`"), "{error}");
        let error = spec
            .call(&context, serde_json::json!({"path": "a", "colour": 1}))
            .unwrap_err();
        assert!(
            error.message().contains("unknown field `colour`"),
            "{error}"
        );
        let output = spec
            .call(&context, serde_json::json!({"path": "a", "limit": 3}))
            .unwrap();
        assert_eq!(output, Output::Text("a Some(3)".into()));
    }
}
