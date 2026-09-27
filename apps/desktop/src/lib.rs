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
pub mod frame;
pub mod icons;
pub mod images;
pub mod input;
pub mod keymap;
pub mod line_layout;
pub mod metrics;
pub mod navigation;
pub mod note;
pub mod outline;
pub mod palette;
pub mod picker;
pub mod stats;
pub mod styling;
pub mod switcher;
pub mod text_offsets;
pub mod theme;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod x11_wake;

pub use editor::{EditorEvent, EditorView, HighlightKind};
