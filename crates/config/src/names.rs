//! The app's names: what people call it, the command that runs it, and
//! the folders it keeps in a vault and on the device. Each is a macro as
//! well as a constant so other text can be built from it with `concat!`.
//!
//! The app was called Editor before it was Gasp; the legacy names are what
//! [`crate::migration`] moves from.

/// What people call the app, as in its menus and window titles.
#[macro_export]
macro_rules! app_name {
    () => {
        "Gasp"
    };
}

/// The command that runs the app, and the name its MCP server gives.
#[macro_export]
macro_rules! command_name {
    () => {
        "gasp"
    };
}

/// The config folder inside a vault.
#[macro_export]
macro_rules! config_dir {
    () => {
        ".gasp"
    };
}

macro_rules! legacy_config_dir {
    () => {
        ".editor"
    };
}
pub(crate) use legacy_config_dir;

pub const APP_NAME: &str = app_name!();
pub const COMMAND_NAME: &str = command_name!();
pub const CONFIG_DIR: &str = config_dir!();
pub const LEGACY_CONFIG_DIR: &str = legacy_config_dir!();

/// The app's own folder inside the platform's config, data and cache
/// folders, cased the way each platform names its apps' folders.
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "windows"))]
pub const APP_FOLDER: &str = app_name!();
#[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "windows")))]
pub const APP_FOLDER: &str = command_name!();

pub const LEGACY_APP_FOLDER: &str = "editor";
