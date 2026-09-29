//! Toolbars from `toolbars.toml`: the status bar, strips along the notes'
//! top or bottom and the window's sides, and bars that float by the
//! selection or the cursor's line.
//!
//! [`render`] draws a bar's items wherever it sits. The workspace draws the
//! docked bars and moves keyboard focus through every bar
//! (`toolbar.focus`); the editor draws the floating ones, where it knows
//! the text's place on screen, in [`floating`].

pub mod floating;
pub mod render;

use gasp_config::toolbars::{Toolbar, ToolbarContext, ToolbarItem};
use gasp_core::syntax::InputContext;
use gpui::{Action, SharedString};

/// Presses item `index` of toolbar `toolbar`, as a click on it does. The
/// workspace runs it, so a command goes where it belongs whatever has
/// focus.
#[derive(Clone, Debug, PartialEq, Eq, Action)]
#[action(namespace = toolbar, no_json)]
pub struct PressToolbarItem {
    pub toolbar: SharedString,
    pub index: usize,
    /// Whether the pointer pressed it, so a menu opens where it is.
    pub by_pointer: bool,
}

/// Opens the Toolbars settings page with toolbar `toolbar`'s picker of
/// things to add.
#[derive(Clone, Debug, PartialEq, Eq, Action)]
#[action(namespace = toolbar, no_json)]
pub struct AddToToolbar {
    pub toolbar: SharedString,
}

/// A place in a bar the keyboard can be on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusStop {
    Item(usize),
    /// The button that adds to the bar.
    Add,
}

/// Which bar has the keyboard, and where in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolbarFocus {
    pub toolbar: String,
    pub stop: FocusStop,
}

/// Where the keyboard can stop in `toolbar`: its commands and menus, then
/// its add button when it has one.
pub fn focus_stops(toolbar: &Toolbar, has_add: bool) -> Vec<FocusStop> {
    let items = toolbar
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| matches!(item, ToolbarItem::Command(_) | ToolbarItem::Menu(_)))
        .map(|(index, _)| FocusStop::Item(index));
    items.chain(has_add.then_some(FocusStop::Add)).collect()
}

/// The stop `step` places after `current` in `stops`, wrapping.
pub fn step_stop(stops: &[FocusStop], current: FocusStop, step: isize) -> Option<FocusStop> {
    let at = stops.iter().position(|stop| *stop == current).unwrap_or(0) as isize;
    let len = stops.len() as isize;
    (len > 0).then(|| stops[(at + step).rem_euclid(len) as usize])
}

/// The element id and test selector of a bar's item.
pub fn item_key(toolbar: &str, index: usize) -> String {
    format!("toolbar-{toolbar}-{index}")
}

/// The element id and test selector of a bar's add button.
pub fn add_key(toolbar: &str) -> String {
    format!("toolbar-{toolbar}-add")
}

/// A command's name on a button with a label: its title without the
/// "Toggle" that every toggle's title starts with, so "Toggle bold"
/// reads "Bold".
pub fn button_label(title: &str) -> String {
    let Some(rest) = title.strip_prefix("Toggle ") else {
        return title.to_owned();
    };
    let mut chars = rest.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// The toolbar context of the text at the cursor. Frontmatter is none of
/// them; links, HTML and comments count as text.
pub fn toolbar_context(context: InputContext) -> Option<ToolbarContext> {
    match context {
        InputContext::Math => Some(ToolbarContext::Math),
        InputContext::Code => Some(ToolbarContext::Code),
        InputContext::Table => Some(ToolbarContext::Table),
        InputContext::Frontmatter => None,
        _ => Some(ToolbarContext::Text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gasp_config::Toolbars;

    #[test]
    fn labels_drop_toggle() {
        assert_eq!(button_label("Toggle bold"), "Bold");
        assert_eq!(button_label("Toggle bulleted list"), "Bulleted list");
        assert_eq!(button_label("Export as HTML"), "Export as HTML");
    }

    #[test]
    fn the_keyboard_stops_on_commands_and_menus() {
        let mut status = Toolbars::defaults().get("status").unwrap().clone();
        assert_eq!(focus_stops(&status, true), [FocusStop::Add]);
        status
            .items
            .insert(1, ToolbarItem::Command("export.html".into()));
        status.items.push(ToolbarItem::Menu("insert".into()));
        let stops = focus_stops(&status, true);
        assert_eq!(
            stops,
            [FocusStop::Item(1), FocusStop::Item(8), FocusStop::Add]
        );
        assert_eq!(
            step_stop(&stops, FocusStop::Add, 1),
            Some(FocusStop::Item(1))
        );
        assert_eq!(
            step_stop(&stops, FocusStop::Item(1), -1),
            Some(FocusStop::Add)
        );
    }
}
