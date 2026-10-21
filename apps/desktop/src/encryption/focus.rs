//! Encrypted notes while the window is away. An open encrypted note is
//! covered as soon as its window loses focus (when
//! `encryption.blur-when-away` is on; the pane draws the cover), and once
//! the window has been away for `encryption.relock-after-seconds`, every
//! encrypted note of the vault locks, so its text leaves memory even if
//! the window never comes back.

use std::collections::HashMap;
use std::time::Duration;

use gpui::{App, Context, EntityId, Global, Task, Window};

use crate::workspace::Workspace;

/// The relock waiting in each away window, by its workspace.
#[derive(Default)]
struct AwayTimers(HashMap<EntityId, Task<()>>);

impl Global for AwayTimers {}

/// Follows the window becoming active or not.
pub fn window_activation_changed(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let id = cx.entity_id();
    window.refresh();
    if window.is_window_active() {
        cx.default_global::<AwayTimers>().0.remove(&id);
        super::lock_file_changed(workspace.vault(), cx);
        return;
    }
    let vault = workspace.vault().to_path_buf();
    let seconds = super::settings_for(&vault, cx).relock_after_seconds;
    let delay = Duration::from_secs(u64::from(seconds));
    let timer = cx.spawn(async move |_, cx| {
        cx.background_executor().timer(delay).await;
        cx.update(|cx| relock_unless_back(&vault, cx)).ok();
    });
    cx.default_global::<AwayTimers>().0.insert(id, timer);
}

/// Locks the vault's notes unless one of the app's windows is in front
/// again, whose own timer starts when it goes away.
fn relock_unless_back(vault: &std::path::Path, cx: &mut App) {
    if cx.active_window().is_some() {
        return;
    }
    super::lock_vault(vault, cx);
}
