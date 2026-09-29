//! Find and replace in the open note: the find bar view. Its fields are
//! `crate::text_input::TextInput`s. Matching lives in `gasp_core::find`.
//!
//! Wiring: call [`bind_keys`] once at startup. The workspace hosts a
//! [`FindBar`] above the editor, creating it on `find.open` or
//! `find.replace` (or calling [`FindBar::show`] when it exists), forwards
//! `find.next` and `find.previous` to [`FindBar::next`] and
//! [`FindBar::previous`], and hides the bar on [`FindBarEvent::Dismissed`].

mod bar;

use gpui::App;

pub use bar::{FIND_BAR_CONTEXT, FindBar, FindBarEvent, REPLACE_CONTEXT, match_label};

/// Binds the find bar's keys. Its fields' editing keys come from the
/// rules, bound by `keymap::bind_rules`.
pub fn bind_keys(cx: &mut App) {
    bar::bind_bar_keys(cx);
}
