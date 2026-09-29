//! The MCP side: `rmcp` handles JSON-RPC framing, the handshake and
//! version negotiation over stdio, and this lists and runs the tools.
//!
//! A tool's failure goes back as a result with `isError` set and the
//! reason in words, which the model reads; only an unknown tool is a
//! protocol error. Tools run on tokio's blocking threads, so a long
//! search or render never holds up `ping`.

use std::io;
use std::path::Path;
use std::sync::Arc;

use base64::Engine;
use gasp_config::{COMMAND_NAME, app_name, config_dir};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::context::Context;
use crate::tool::{Output, ToolResult, ToolSpec};

/// What the client is told about the server as a whole.
const INSTRUCTIONS: &str = concat!(
    "Tools for one Markdown vault in the ",
    app_name!(),
    " app. Paths are relative to the vault and use /; a note's .md may be left off. Hidden \
     folders (.git, ",
    config_dir!(),
    ", .trash) are off limits except through the settings, theme, rules, snippets and \
     replacements tools. Prefer patch_note over write_note for small changes. editor_state, \
     run_command and open_note need the app running with this vault open; the other tools \
     work either way."
);

/// The server for one vault.
#[derive(Clone)]
pub struct Server {
    context: Arc<Context>,
    tools: Arc<Vec<ToolSpec>>,
}

impl Server {
    pub fn new(context: Context) -> Server {
        Server {
            context: Arc::new(context),
            tools: Arc::new(crate::tools::all()),
        }
    }

    /// Serves MCP over `reader` and `writer` until the client hangs up.
    pub async fn serve<R, W>(self, reader: R, writer: W) -> io::Result<()>
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        let running = ServiceExt::serve(self, (reader, writer))
            .await
            .map_err(io::Error::other)?;
        running.waiting().await.map_err(io::Error::other)?;
        Ok(())
    }

    async fn run(&self, name: &str, arguments: Value) -> Result<CallToolResult, ErrorData> {
        let index = self
            .tools
            .iter()
            .position(|tool| tool.name == name)
            .ok_or_else(|| ErrorData::invalid_params(format!("there's no tool {name:?}"), None))?;
        let (context, tools) = (self.context.clone(), self.tools.clone());
        let result = tokio::task::spawn_blocking(move || tools[index].call(&context, arguments))
            .await
            .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
        Ok(tool_result(result))
    }
}

/// Runs the server for `vault` on stdin and stdout: `gasp mcp`.
pub fn serve_stdio(vault: &Path) -> io::Result<()> {
    let context = Context::open(vault)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(Server::new(context).serve(tokio::io::stdin(), tokio::io::stdout()))
}

impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(COMMAND_NAME, env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = self.tools.iter().map(ToolSpec::definition).collect();
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let arguments = request.arguments.map_or(Value::Null, Value::Object);
        self.run(&request.name, arguments).await.map(Into::into)
    }
}

/// A tool's output, or its error in words, as MCP content.
fn tool_result(result: ToolResult) -> CallToolResult {
    match result {
        Ok(Output::Text(text)) => CallToolResult::success(vec![ContentBlock::text(text)]),
        Ok(Output::Json(value)) => {
            let text = serde_json::to_string_pretty(&value).unwrap_or_default();
            CallToolResult::success(vec![ContentBlock::text(text)])
        }
        Ok(Output::Image {
            data,
            mime,
            caption,
        }) => {
            let encoded = base64::engine::general_purpose::STANDARD.encode(data);
            CallToolResult::success(vec![
                ContentBlock::image(encoded, mime),
                ContentBlock::text(caption),
            ])
        }
        Err(error) => CallToolResult::error(vec![ContentBlock::text(error.message())]),
    }
}
