//! This computer's own SSH key, for a vault that syncs over SSH: where it
//! lives, copying its public half, and GitHub's page for adding it. The
//! setup form and the Sync page both offer it, from here.

use gasp_sync::ssh::DeviceKey;
use gpui::{App, ClipboardItem};

use crate::notices::{Notice, show};

/// GitHub's page for adding an SSH key to an account.
pub const NEW_SSH_KEY_URL: &str = "https://github.com/settings/ssh/new";

/// Tells sync where this computer's key is kept: an `ssh` folder in the
/// app's config folder (the sandbox's own, in a snapshot run).
pub fn use_key_folder() {
    if let Some(folder) = crate::sandbox::config_folder() {
        gasp_sync::ssh::use_key_folder(folder.join("ssh"));
    }
}

/// This computer's key, made the first time it's asked for. Its comment
/// names the computer, so it reads well in GitHub's list of keys.
pub fn this_computer_key() -> Result<DeviceKey, String> {
    use_key_folder();
    let label = format!(
        "{} on {}",
        gasp_config::APP_NAME,
        crate::edit_time::device_name()
    );
    gasp_sync::ssh::device_key(&label)
        .map_err(|error| format!("This computer’s SSH key couldn’t be made. {error}"))
}

/// Copies the public half of this computer's key, and says so.
pub fn copy_key(cx: &mut App) {
    match this_computer_key() {
        Ok(key) => {
            cx.write_to_clipboard(ClipboardItem::new_string(key.public.clone()));
            show(
                Notice::done("Copied this computer’s SSH key")
                    .with_detail(format!("GitHub lists it as {}.", key.fingerprint)),
                cx,
            );
        }
        Err(message) => {
            show(Notice::problem(message), cx);
        }
    }
}

/// Opens GitHub's page for adding a key.
pub fn open_new_key_page(cx: &mut App) {
    crate::sandbox::open_url(NEW_SSH_KEY_URL, cx);
}
