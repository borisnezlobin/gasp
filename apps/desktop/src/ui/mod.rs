//! Shared UI primitives for the workspace chrome: icon and text buttons,
//! tooltips, dropdown and context menus, breadcrumbs, key caps and the
//! surfaces menus and dialogs float on. Every value comes from
//! [`UiTheme`].
//!
//! Commands show their title and current shortcut wherever they appear,
//! from [`hints`], which the workspace fills from the vault's rules.

pub mod breadcrumbs;
pub mod button;
pub mod drawn_area;
pub mod focus_visible;
pub mod hints;
pub mod icon_button;
pub mod keycap;
pub mod menu;
pub mod selector;
pub mod suggestions;
pub mod surface;
pub mod tooltip;
pub mod truncated;

use std::sync::Arc;

use gasp_config::Config;
use gasp_config::theme::Theme as Tokens;
use gpui::{App, Global, WindowAppearance};

pub use breadcrumbs::{Breadcrumbs, Crumb};
pub use button::{Button, ButtonKind};
pub use drawn_area::DrawnArea;
pub use icon_button::IconButton;
pub use keycap::keycap;
pub use menu::{DropdownMenu, HasMenuSlot, MenuAnchor, MenuEntry, MenuHandler, MenuItem, MenuSlot};
pub use selector::Selectable;
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

/// The installed font families, once they've been listed.
struct FontNames(Arc<[String]>);

impl Global for FontNames {}

/// Set once listing the fonts has started, so it starts only once.
struct FontsLoading;

impl Global for FontsLoading {}

/// The installed font families, or `None` until they've been listed.
/// Listing walks every font the platform knows (a fifth of a second on
/// a Mac with many fonts), so it happens once, off the main thread and
/// after the first frame: see [`load_installed_fonts`]. Until then a
/// font is drawn by the family it names.
pub fn installed_fonts(cx: &App) -> Option<Arc<[String]>> {
    cx.try_global::<FontNames>().map(|names| names.0.clone())
}

/// Lists the installed fonts on a background thread once the first
/// frame is on screen, then puts them in effect with
/// [`set_installed_fonts`]. Asking again does nothing.
pub fn load_installed_fonts(cx: &mut App) {
    if cx.has_global::<FontsLoading>() || cx.has_global::<FontNames>() {
        return;
    }
    cx.set_global(FontsLoading);
    if crate::first_frame::is_waiting() {
        crate::first_frame::defer(list_fonts);
    } else {
        list_fonts(cx);
    }
}

fn list_fonts(cx: &mut App) {
    let text_system = cx.text_system().clone();
    let listing = cx.background_executor().spawn(async move {
        let _span = crate::trace::span("font-names");
        family_names(text_system.all_font_names())
    });
    cx.spawn(async move |cx| {
        let names = listing.await;
        cx.update(|cx| set_installed_fonts(names, cx)).ok();
    })
    .detach();
}

/// Puts the installed font `names` in effect: the theme swaps in a
/// fallback for any font it names that isn't among them, every window
/// redraws, and views following [`observe_installed_fonts`] hear of it.
/// Tests call it to have the fonts arrive late.
pub fn set_installed_fonts(names: Vec<String>, cx: &mut App) {
    let _span = crate::trace::span("font-names-apply");
    cx.set_global(FontNames(family_names(names).into()));
    if let Some(theme) = cx.try_global::<ThemeGlobal>() {
        let (tokens, dark) = (theme.tokens.clone(), theme.dark);
        let theme = ThemeGlobal::build(&tokens, dark, cx);
        cx.set_global(theme);
    }
    cx.refresh_windows();
}

/// Each family once, sorted. The platform names a family once per face,
/// so a Mac's list of tens of thousands of names holds a few hundred
/// families.
fn family_names(mut names: Vec<String>) -> Vec<String> {
    names.sort_unstable();
    names.dedup();
    names.shrink_to_fit();
    names
}

/// Calls `f` when the installed fonts arrive.
pub fn observe_installed_fonts<V: 'static>(
    cx: &mut gpui::Context<V>,
    f: impl FnMut(&mut V, &mut gpui::Context<V>) + 'static,
) -> gpui::Subscription {
    cx.observe_global::<FontNames>(f)
}

impl ThemeGlobal {
    /// The component tokens for `tokens`, which are already for one mode.
    fn build(tokens: &Tokens, dark: bool, cx: &mut App) -> ThemeGlobal {
        let installed = installed_fonts(cx).unwrap_or_default();
        let palette = Palette::from_tokens(tokens);
        let ui = UiTheme {
            toolbar: crate::theme::ToolbarTheme::from_tokens(tokens),
            ..UiTheme::themed(&palette, &installed)
        };
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
