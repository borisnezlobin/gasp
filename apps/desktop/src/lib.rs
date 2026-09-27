//! The desktop app: a GPUI live-preview Markdown editor, plus headless CLI
//! modes for tests and agents.

pub mod actions;
pub mod app;
pub mod bench;
pub mod cli;
pub mod commands;
pub mod demo;
pub mod editor;
pub mod element;
pub mod export_ui;
pub mod features;
pub mod file_tree;
pub mod find;
pub mod frame;
pub mod icons;
pub mod images;
pub mod input;
pub mod keymap;
pub mod line_layout;
pub mod link_update;
pub mod metrics;
pub mod navigation;
pub mod note;
pub mod outline;
pub mod palette;
pub mod paste;
pub mod picker;
pub mod preview;
pub mod settings_view;
pub mod stats;
pub mod styling;
pub mod switcher;
pub mod sync;
pub mod text_input;
pub mod text_offsets;
pub mod theme;
pub mod ui;
pub mod vault_search;
pub mod workspace;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod x11_wake;

pub use editor::{EditorEvent, EditorView, HighlightKind};
