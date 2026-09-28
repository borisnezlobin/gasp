//! The settings screen, generated from the settings schema so it never
//! drifts from the files.
//!
//! [`SettingsView`] is a modal sized to a share of the window. It writes
//! every change straight away (settings to `.editor/settings.toml`, fonts
//! and the accent colour to `.editor/theme.toml`, shortcut changes to
//! `.editor/rules.toml`, snippets to `.editor/snippets.txt` and
//! replacements to `.editor/replacements.toml`) and emits [`SettingsEvent::Changed`]. It asks the
//! host to run commands, such as opening another vault, with
//! [`SettingsRequest`]. Escape and the close button emit `DismissEvent`.

mod capture;
pub mod controls;
mod edit;
mod keys;
mod menu;
pub mod model;
pub mod popover;
mod render;
mod rows;
pub mod snippet_editor;
pub mod snippet_look;
mod snippet_rows;
pub mod snippets_page;
mod sync_page;
mod view;

pub use editor_config::{config_files, store};
pub use menu::MenuTarget;
pub use model::{FontSlot, Page};
pub use render::modal_size;
pub use view::{
    Card, ControlRow, PaneLayout, SettingsEvent, SettingsFocus, SettingsRequest, SettingsView,
};
