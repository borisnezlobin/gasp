//! The desktop app: a GPUI live-preview Markdown editor, plus headless CLI
//! modes for tests and agents.

pub mod actions;
#[cfg(target_os = "macos")]
pub mod allocator;
pub mod app;
pub mod app_icons;
pub mod attachment_cleanup;
pub mod appearance_toggle;
pub mod dock_icon;
pub mod atlas;
pub mod bench;
pub mod cli;
pub mod code_copy;
pub mod commands;
pub mod demo;
pub mod edit_time;
pub mod editor;
pub mod element;
pub mod embeds;
pub mod export_ui;
pub mod features;
pub mod file_tree;
pub mod find;
pub mod first_frame;
pub mod folding;
pub mod footnotes;
pub mod frame;
pub mod hover;
pub mod icons;
pub mod images;
pub mod input;
pub mod keymap;
pub mod keytrace;
pub mod knowledge;
pub mod line_cache;
pub mod line_layout;
pub mod link_cards;
pub use gasp_vault::link_update;
pub mod look_up;
pub mod memory;
pub mod metrics;
pub mod move_picker;
pub mod navigation;
pub mod note;
pub mod note_texts;
pub mod notices;
pub mod obsidian_import;
pub mod open_bench;
pub mod outline;
pub mod palette;
pub mod paste;
pub mod plain_errors;
pub mod pending_renders;
pub mod picker;
pub mod preview;
pub mod print;
pub mod prose;
pub mod recovery;
pub mod reduce_motion;
#[cfg(target_os = "macos")]
pub mod rich_copy;
pub mod sandbox;
pub mod settings_view;
pub mod snapshot;
pub mod stats;
pub mod styling;
pub mod suggest;
pub mod switcher;
pub mod sync;
pub mod table_edit;
pub mod telemetry;
pub mod text_input;
pub mod text_offsets;
pub mod theme;
pub mod toolbar;
pub mod tour;
pub mod trace;
pub mod trashing;
pub mod typing;
pub mod ui;
#[cfg(target_os = "macos")]
pub mod update;
pub mod vault_index;
pub mod vault_search;
pub mod vault_watch;
pub mod window_controls;
pub mod window_drag;
pub mod workspace;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod x11_wake;

pub use editor::{EditorEvent, EditorView, HighlightKind};

/// Unit tests keep the app's own folders out of the person's, as the
/// integration tests do.
#[cfg(test)]
#[ctor::ctor]
fn keep_app_folders_out_of_the_way() {
    sandbox::keep_app_folders_for_tests();
}
