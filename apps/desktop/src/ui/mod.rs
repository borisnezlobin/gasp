//! Shared UI primitives for the workspace chrome: icon and text buttons,
//! tooltips, dropdown and context menus, breadcrumbs, key caps and the
//! surfaces menus and dialogs float on. Every value comes from
//! [`UiTheme`].
//!
//! Commands show their title and current shortcut wherever they appear,
//! from [`hints`], which the workspace fills from the vault's rules.

pub mod breadcrumbs;
pub mod button;
pub mod hints;
pub mod icon_button;
pub mod keycap;
pub mod menu;
pub mod suggestions;
pub mod surface;
pub mod tooltip;
pub mod truncated;

use std::sync::Arc;

use editor_config::Config;
use editor_config::theme::Theme as Tokens;
use gpui::{App, Global, WindowAppearance};

pub use breadcrumbs::{Breadcrumbs, Crumb};
pub use button::{Button, ButtonKind};
pub use icon_button::IconButton;
pub use keycap::keycap;
pub use menu::{DropdownMenu, HasMenuSlot, MenuAnchor, MenuHandler, MenuItem, MenuSlot};
pub use suggestions::{ListHandlers, SuggestionRow, suggestion_list};
pub use surface::{dialog, popover};
pub use tooltip::Tooltip;
pub use truncated::{Truncated, truncated};

use crate::theme::{InputTheme, Palette, SettingsTheme, UiTheme};

/// The theme in effect: whether it's dark, the tokens for that mode and
/// the component tokens built from them. It's built once per change of
/// mode or theme file, so drawing a frame never reads a token.
struct ThemeGlobal {
    dark: bool,
    tokens: Tokens,
    palette: Palette,
    ui: UiTheme,
    input: InputTheme,
    settings: SettingsTheme,
}

impl Global for ThemeGlobal {}

struct FontNames(Arc<[String]>);

impl Global for FontNames {}

/// The installed font families. Listing them walks every font the
/// platform knows, so it happens once and every theme shares the list.
pub fn installed_fonts(cx: &mut App) -> Arc<[String]> {
    if let Some(names) = cx.try_global::<FontNames>() {
        return names.0.clone();
    }
    let _span = crate::trace::span("font-names");
    let names: Arc<[String]> = cx.text_system().all_font_names().into();
    cx.set_global(FontNames(names.clone()));
    names
}

impl ThemeGlobal {
    /// The component tokens for `tokens`, which are already for one mode.
    fn build(tokens: &Tokens, dark: bool, cx: &mut App) -> ThemeGlobal {
        let installed = installed_fonts(cx);
        let palette = Palette::from_tokens(tokens);
        let ui = UiTheme::themed(&palette, &installed);
        let input = InputTheme {
            font_family: ui.font_family.clone(),
            ..InputTheme::from_palette(&palette)
        };
        let settings = SettingsTheme {
            font_family: ui.font_family.clone(),
            ..SettingsTheme::from_tokens(tokens)
        };
        ThemeGlobal {
            dark,
            tokens: tokens.clone(),
            palette,
            ui,
            input,
            settings,
        }
    }
}

fn theme_global(cx: &mut App) -> &ThemeGlobal {
    if !cx.has_global::<ThemeGlobal>() {
        let theme = ThemeGlobal::build(&Config::defaults().theme, false, cx);
        cx.set_global(theme);
    }
    cx.global::<ThemeGlobal>()
}

/// The UI tokens, with the UI font resolved against the installed fonts
/// the first time they're asked for.
pub fn ui_theme(cx: &mut App) -> UiTheme {
    theme_global(cx).ui.clone()
}

/// The tokens every text input starts from.
pub fn input_theme(cx: &mut App) -> InputTheme {
    theme_global(cx).input.clone()
}

/// The settings screen's look, also used by its controls elsewhere.
pub fn settings_theme(cx: &mut App) -> SettingsTheme {
    theme_global(cx).settings.clone()
}

/// Every colour of the theme in effect.
pub fn palette(cx: &mut App) -> Palette {
    theme_global(cx).palette.clone()
}

/// Whether the app draws in its dark palette.
pub fn is_dark(cx: &mut App) -> bool {
    theme_global(cx).dark
}

/// Whether the system's appearance is dark, as the workspace last saw it.
struct SystemDark(bool);

impl Global for SystemDark {}

/// Records whether the system's appearance is dark.
pub fn set_system_dark(dark: bool, cx: &mut App) {
    cx.set_global(SystemDark(dark));
}

/// Whether the system's appearance is dark; light until told otherwise.
pub fn system_dark(cx: &App) -> bool {
    cx.try_global::<SystemDark>().is_some_and(|system| system.0)
}

/// Whether a window's appearance is a dark one.
pub fn is_dark_appearance(appearance: WindowAppearance) -> bool {
    matches!(
        appearance,
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

/// Puts `tokens` in effect, in dark mode when `dark` holds (`tokens` has
/// both modes). Every window redraws when anything changed; the answer
/// says whether it did, so views that keep their own theme can rebuild.
pub fn set_theme(tokens: &Tokens, dark: bool, cx: &mut App) -> bool {
    let tokens = tokens.for_mode(dark);
    let current = theme_global(cx);
    if current.dark == dark && current.tokens == *tokens {
        return false;
    }
    let theme = ThemeGlobal::build(tokens, dark, cx);
    cx.set_global(theme);
    cx.refresh_windows();
    true
}
