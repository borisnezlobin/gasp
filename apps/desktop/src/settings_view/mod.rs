//! The settings screen, generated from the settings schema so it never
//! drifts from the files.
//!
//! [`SettingsView`] writes every change to the vault's
//! `.editor/settings.toml` straight away and emits
//! [`SettingsEvent::Changed`]. It fills the space it's given, so the host
//! can show it as a tab or in a modal; Escape emits `DismissEvent`.

mod keys;
pub mod model;
mod render;
pub mod store;
mod view;

pub use view::{ControlRow, SectionRef, SettingsEvent, SettingsFocus, SettingsView};
