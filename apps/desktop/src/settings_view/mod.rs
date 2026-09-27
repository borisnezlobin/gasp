//! The settings screen, generated from the settings schema so it never
//! drifts from the files, plus the text field it shares with the file tree.
//!
//! [`SettingsView`] writes every change to the vault's
//! `.editor/settings.toml` straight away and emits
//! [`SettingsEvent::Changed`]. It fills the space it's given, so the host
//! can show it as a tab or in a modal; Escape emits `DismissEvent`.

mod keys;
pub mod model;
mod render;
pub mod store;
mod text_field;
mod view;

pub use text_field::{TextField, TextFieldEvent};
pub use view::{ControlRow, SectionRef, SettingsEvent, SettingsFocus, SettingsView};
