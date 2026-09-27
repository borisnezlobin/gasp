//! Whether the keyboard is driving, so focus rings show only when someone
//! is finding their way with it: after Tab, an arrow, Enter or Escape, not
//! after a click and not when a screen first opens. A click hands the
//! pointer back; opening a screen starts it with no ring.
//!
//! Every focus ring asks [`ring`]. Text fields keep their caret and fill
//! either way, since those say where typing goes.

use gpui::{
    App, DispatchPhase, Global, IntoElement, Keystroke, KeystrokeEvent, MouseDownEvent, Styled,
    canvas,
};

use crate::keymap::KEY_CONTEXT;

#[derive(Default)]
struct KeyboardDriving(bool);

impl Global for KeyboardDriving {}

/// Keys that move focus or act on what has it.
const NAVIGATION_KEYS: [&str; 7] = ["tab", "up", "down", "left", "right", "enter", "escape"];

/// Watches every keystroke in every window from now on.
pub fn install(cx: &mut App) {
    cx.intercept_keystrokes(|event, _, cx| note_keystroke(event, cx))
        .detach();
}

fn note_keystroke(event: &KeystrokeEvent, cx: &mut App) {
    // In the editor the arrows and Enter move the caret and break lines,
    // which is typing, not finding a control.
    let in_editor = event
        .context_stack
        .iter()
        .any(|context| context.contains(KEY_CONTEXT));
    if !in_editor && navigates(&event.keystroke) {
        set_keyboard_driving(true, cx);
    }
}

/// Whether `keystroke` is a navigation key, alone or with Shift.
pub fn navigates(keystroke: &Keystroke) -> bool {
    let modifiers = keystroke.modifiers;
    let plain = !(modifiers.control || modifiers.alt || modifiers.platform || modifiers.function);
    plain && NAVIGATION_KEYS.contains(&keystroke.key.as_str())
}

/// Whether the keyboard is driving.
pub fn keyboard_driving(cx: &App) -> bool {
    cx.try_global::<KeyboardDriving>()
        .is_some_and(|driving| driving.0)
}

/// Whether something that has focus shows its ring.
pub fn ring(focused: bool, cx: &App) -> bool {
    focused && keyboard_driving(cx)
}

/// Says whether the keyboard is driving; windows redraw when that
/// changes, which is at most once per switch between keyboard and mouse.
pub fn set_keyboard_driving(driving: bool, cx: &mut App) {
    if keyboard_driving(cx) == driving {
        return;
    }
    cx.set_global(KeyboardDriving(driving));
    cx.refresh_windows();
}

/// An invisible element that hands the pointer back on any mouse press in
/// its window, however deep the press lands. Each window's root has one.
pub fn pointer_watch() -> impl IntoElement {
    canvas(
        |_, _, _| {},
        |_, _, window, _| {
            window.on_mouse_event(|_: &MouseDownEvent, phase, _, cx| {
                if phase == DispatchPhase::Capture {
                    set_keyboard_driving(false, cx);
                }
            });
        },
    )
    .absolute()
    .size_0()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_keys_drive_and_shortcuts_do_not() {
        let key = |text: &str| Keystroke::parse(text).unwrap();
        assert!(navigates(&key("tab")));
        assert!(navigates(&key("shift-tab")));
        assert!(navigates(&key("down")));
        assert!(navigates(&key("escape")));
        assert!(!navigates(&key("a")));
        assert!(!navigates(&key("secondary-,")));
        assert!(!navigates(&key("ctrl-tab")));
    }
}
