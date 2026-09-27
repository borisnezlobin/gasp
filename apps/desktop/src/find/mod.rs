//! Find and replace in the open note: the find bar view and the query
//! field it shares with the vault search panel. Matching lives in
//! `editor_core::find`.
//!
//! Wiring: call [`bind_keys`] once at startup. The workspace hosts a
//! [`FindBar`] above the editor, creating it on `find.open` or
//! `find.replace` (or calling [`FindBar::show`] when it exists), forwards
//! `find.next` and `find.previous` to [`FindBar::next`] and
//! [`FindBar::previous`], and hides the bar on [`FindBarEvent::Dismissed`].

mod bar;
pub mod input;

use editor_config::RuleSet;
use gpui::App;

pub use bar::{FIND_BAR_CONTEXT, FindBar, FindBarEvent, REPLACE_CONTEXT, match_label};
pub use input::{QueryInput, QueryInputEvent};

/// Binds the find bar's keys and the query field's editing keys from the
/// default rules.
pub fn bind_keys(cx: &mut App) {
    bind_keys_from(&RuleSet::defaults(), cx);
}

/// Binds the find bar's keys, and the query field's editing keys from
/// `rules`.
pub fn bind_keys_from(rules: &RuleSet, cx: &mut App) {
    input::bind_input_keys(rules, cx);
    bar::bind_bar_keys(cx);
}
