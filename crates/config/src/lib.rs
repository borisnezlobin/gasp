//! Config loading, settings schema, theme tokens, rules engine and command registry.
//!
//! A vault's `.gasp/` folder holds `settings.toml`, `theme.toml`, `layout.toml`,
//! `rules.toml` and `toolbars.toml`, which layer over the built-in files in `defaults/`, a
//! device-local `device.toml` that never syncs, and `snippets.txt` and
//! `replacements.toml`, which replace the built-in snippets and replacements.

pub mod commands;
pub mod config_files;
pub mod device;
pub mod diagnostics;
pub mod keymap;
pub mod keys;
pub mod layout;
pub mod loader;
pub mod merge;
pub mod migration;
pub mod names;
pub mod platform;
pub mod rules;
pub mod schema;
pub mod setting_texts;
pub mod settings;
pub mod store;
pub mod theme;
pub mod toolbar_files;
pub mod toolbars;
pub mod typing;
pub mod watcher;

pub use commands::{Args, CommandError, CommandInfo, CommandRegistry, CommandSpec};
pub use diagnostics::{Diagnostic, Severity};
pub use keys::KeyChord;
pub use loader::{Config, ConfigFile, ConfigLoader};
pub use names::{APP_FOLDER, APP_NAME, COMMAND_NAME, CONFIG_DIR};
pub use platform::{InputContext, Platform, PlatformFilter};
pub use rules::{
    Clock, Dispatch, Event, EventKind, ManualClock, MatchContext, Rule, RuleEngine, RuleSet,
};
pub use settings::Settings;
pub use theme::Theme;
pub use toolbars::{Toolbar, ToolbarItem, Toolbars};
pub use typing::{ReplacementTable, SnippetTable, TypingTables};
pub use watcher::{ConfigUpdate, ConfigWatcher};
