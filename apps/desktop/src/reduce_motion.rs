//! Whether the system asks apps to hold still: Reduce Motion in macOS's
//! Accessibility settings. Read when it matters rather than watched,
//! since a window that cares reads it again when it's brought forward.

use std::cell::Cell;

#[cfg(target_os = "macos")]
mod macos;

thread_local! {
    /// What tests on this thread want the setting to be.
    static PRETENDED: Cell<Option<bool>> = const { Cell::new(None) };
}

/// Whether Reduce Motion is on.
pub fn is_on() -> bool {
    if let Some(pretended) = PRETENDED.get() {
        return pretended;
    }
    #[cfg(target_os = "macos")]
    {
        macos::is_on()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// From now on, on this thread, answers `on` whatever the system says,
/// for tests.
pub fn pretend(on: bool) {
    PRETENDED.set(Some(on));
}
