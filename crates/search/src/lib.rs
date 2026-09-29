//! Search index, queries and excerpts.
//!
//! [`engine`] searches notes held in memory with the owner's Omnisearch
//! weights; [`tags`] answers `tag:name` queries from the link index's
//! tagged notes. The app's search panel and the MCP server's `search`
//! tool both run these, so they rank the same way. [`ocr`] keeps the
//! text recognised in images and PDFs and searches it.

pub mod engine;
mod fold;
mod hits;
pub mod ocr;
pub mod tags;
