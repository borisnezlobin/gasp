//! The vault on disk without a UI: the link index ([`index::LinkIndex`])
//! and how it's built, rewriting links when notes move
//! ([`link_update`]), the file operations behind the file tree ([`ops`])
//! and atomic writes ([`files`]).
//!
//! The desktop app and the MCP server (`editor mcp`) both use it, so a
//! rename, a delete or a backlinks list means the same in either. Nothing
//! here depends on GPUI, so the server starts without loading it.

pub mod build;
pub mod entries;
pub mod files;
pub mod index;
pub mod knowledge;
pub mod link_update;
pub mod mentions;
pub mod ops;
pub mod parse;
pub mod recovery;
