//! Moving the window from the tab bar's empty space. On macOS the app
//! draws under a hidden title bar, so the tab bar is the window's top
//! edge: its empty space moves the window as a title bar would, and a
//! double click zooms or minimizes as the system setting says. The tabs
//! themselves don't move it, since they drag to reorder and split.
//!
//! AppKit would otherwise move the window from any press on GPUI's view,
//! tabs included, and GPUI 0.2.2's `start_window_move` does nothing on
//! macOS, so both halves go through AppKit directly.
//!
//! On Linux the system's title bar usually stays above the tab bar, but
//! GNOME on Wayland leaves the title bar to the app
//! (`crate::window_controls`), so there the tab bar moves the window too,
//! a double click maximizes it and a right click shows the desktop's
//! window menu. Where a title bar is drawn the compositor ignores these.

use std::cell::Cell;

use gpui::Window;

#[cfg(target_os = "macos")]
mod macos;

thread_local! {
    /// How many window moves the tab bar has started on this thread, for
    /// tests, which each run on their own.
    static STARTED: Cell<usize> = const { Cell::new(0) };
    /// Whether moves only count, for GPUI's test windows, which have no
    /// AppKit window to move.
    static ONLY_COUNT: Cell<bool> = const { Cell::new(false) };
}

/// How many window moves have started so far on this thread.
pub fn moves_started() -> usize {
    STARTED.get()
}

/// From now on, on this thread, moves are counted and nothing is moved.
pub fn only_count_moves() {
    ONLY_COUNT.set(true);
}

/// Stops a press on the app's own view from moving the window, from now
/// on, so only [`start`] moves it.
pub fn install() {
    #[cfg(target_os = "macos")]
    if !macos::keep_presses_in_the_view() {
        eprintln!("could not stop presses on tabs from moving the window");
    }
}

/// Moves the window with the pointer, from a press that's being handled
/// now.
pub fn start(window: &mut Window) {
    if crate::sandbox::blocks("moving the window") {
        return;
    }
    STARTED.set(STARTED.get() + 1);
    if ONLY_COUNT.get() {
        return;
    }
    #[cfg(target_os = "macos")]
    if let Some(view) = crate::look_up::native_view(window) {
        macos::drag_window(view);
    }
    #[cfg(not(target_os = "macos"))]
    window.start_window_move();
}

/// Zooms or minimizes the window, as a title bar's double click does.
pub fn double_click(window: &mut Window) {
    if crate::sandbox::blocks("zooming the window") {
        return;
    }
    #[cfg(target_os = "macos")]
    window.titlebar_double_click();
    #[cfg(not(target_os = "macos"))]
    window.zoom_window();
}

/// Shows the desktop's window menu at `position`, as a right click on a
/// title bar does, where the app draws the title bar.
pub fn window_menu(window: &mut Window, position: gpui::Point<gpui::Pixels>) {
    if ONLY_COUNT.get() || !crate::window_controls::drawn_by_app(window) {
        return;
    }
    window.show_window_menu(position);
}
