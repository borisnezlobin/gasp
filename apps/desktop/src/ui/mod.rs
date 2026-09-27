//! Shared UI primitives for the workspace chrome: icon buttons with
//! tooltips, dropdown and context menus, and breadcrumbs. Every value
//! comes from [`UiTheme`].
//!
//! Commands show their title and current shortcut wherever they appear,
//! from [`hints`], which the workspace fills from the vault's rules.

pub mod breadcrumbs;
pub mod hints;
pub mod icon_button;
pub mod keycap;
pub mod menu;
pub mod tooltip;

use gpui::{App, Global};

pub use breadcrumbs::{Breadcrumbs, Crumb};
pub use icon_button::IconButton;
pub use keycap::keycap;
pub use menu::{DropdownMenu, HasMenuSlot, MenuAnchor, MenuHandler, MenuItem, MenuSlot};
pub use tooltip::Tooltip;

use crate::theme::UiTheme;

struct ThemeGlobal(UiTheme);

impl Global for ThemeGlobal {}

/// The UI tokens, with the UI font resolved against the installed fonts
/// the first time they're asked for.
pub fn ui_theme(cx: &mut App) -> UiTheme {
    if let Some(theme) = cx.try_global::<ThemeGlobal>() {
        return theme.0.clone();
    }
    let installed = cx.text_system().all_font_names();
    let theme = UiTheme::with_installed_fonts(&installed);
    cx.set_global(ThemeGlobal(theme.clone()));
    theme
}
