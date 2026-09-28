//! The MCP server behind `editor mcp`: tools over one vault, its config
//! and, through [`bridge`], the desktop app when it has the vault open.
//!
//! The headless tools use the vault crates directly and never load GPUI,
//! so the server starts in a few milliseconds and works whether or not
//! the app is running. The app side of the bridge lives here too, so the
//! desktop app only supplies a handler.

pub mod bridge;
pub mod context;
pub mod frontmatter;
pub mod patch;
pub mod paths;
pub mod server;
pub mod tool;
pub mod tools;

pub use context::Context;
pub use server::serve_stdio;
