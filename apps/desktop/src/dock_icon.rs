//! The Dock's icon, which `appearance.app-icon` picks: the breaching
//! whale or its head up close, each in a dark version while the system is
//! in dark mode. Only the Dock changes, and only while Gasp runs; Finder
//! and Launchpad keep the bundle's icon. Writing the icon into the bundle
//! would break its code signature, which the updater checks.

use std::collections::HashMap;
use std::sync::Arc;

use gasp_config::settings::AppIconChoice;
use gpui::{App, Global, RenderImage};

#[cfg(target_os = "macos")]
mod macos;

/// How many pixels square a settings tile's icon is drawn at: a
/// 48-point picture on a Retina screen.
pub const PREVIEW_PIXELS: u32 = 96;

const BREACHING_LIGHT: &[u8] = include_bytes!("../assets/icon/dock/breaching-light.png");
const BREACHING_DARK: &[u8] = include_bytes!("../assets/icon/dock/breaching-dark.png");
const UP_CLOSE_LIGHT: &[u8] = include_bytes!("../assets/icon/dock/up-close-light.png");
const UP_CLOSE_DARK: &[u8] = include_bytes!("../assets/icon/dock/up-close-dark.png");

/// One of the four pictures of the icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IconArtwork {
    pub choice: AppIconChoice,
    pub dark: bool,
}

impl IconArtwork {
    pub fn png(self) -> &'static [u8] {
        match (self.choice, self.dark) {
            (AppIconChoice::Breaching, false) => BREACHING_LIGHT,
            (AppIconChoice::Breaching, true) => BREACHING_DARK,
            (AppIconChoice::UpClose, false) => UP_CLOSE_LIGHT,
            (AppIconChoice::UpClose, true) => UP_CLOSE_DARK,
        }
    }

    /// Whether this is the bundle's own icon, which the Dock shows
    /// without being told.
    fn is_bundle_icon(self) -> bool {
        self == IconArtwork {
            choice: AppIconChoice::Breaching,
            dark: false,
        }
    }
}

/// What the Dock should show for `choice` while the system is `dark`:
/// `None` for the bundle's own icon.
pub fn dock_artwork(choice: AppIconChoice, dark: bool) -> Option<IconArtwork> {
    let artwork = IconArtwork { choice, dark };
    (!artwork.is_bundle_icon()).then_some(artwork)
}

/// What the Dock was last told to show. Until the first change it shows
/// the bundle's icon.
#[derive(Default)]
struct ShownDockIcon(Option<IconArtwork>);

impl Global for ShownDockIcon {}

/// Gives the Dock the icon `choice` asks for in the system's current
/// appearance, if it isn't showing it already. A snapshot run has no Dock
/// icon and leaves it alone.
pub fn show_in_dock(choice: AppIconChoice, cx: &mut App) {
    if !crate::sandbox::reaches_outside() {
        return;
    }
    let wanted = dock_artwork(choice, crate::ui::system_dark(cx));
    let shown = cx.default_global::<ShownDockIcon>();
    if shown.0 == wanted {
        return;
    }
    shown.0 = wanted;
    set_dock_png(wanted.map(IconArtwork::png));
}

#[cfg(target_os = "macos")]
fn set_dock_png(png: Option<&[u8]>) {
    macos::set_dock_png(png);
}

#[cfg(not(target_os = "macos"))]
fn set_dock_png(_: Option<&[u8]>) {}

/// Every tile picture decoded so far.
#[derive(Default)]
struct Previews(HashMap<IconArtwork, Arc<RenderImage>>);

impl Global for Previews {}

/// `artwork` at [`PREVIEW_PIXELS`] square, for a settings tile. Each is
/// decoded once, the first time it's shown.
pub fn preview(artwork: IconArtwork, cx: &mut App) -> Arc<RenderImage> {
    cx.default_global::<Previews>()
        .0
        .entry(artwork)
        .or_insert_with(|| Arc::new(crate::images::render_image(preview_pixels(artwork))))
        .clone()
}

fn preview_pixels(artwork: IconArtwork) -> image::RgbaImage {
    let pixels = image::load_from_memory_with_format(artwork.png(), image::ImageFormat::Png)
        .map(|picture| picture.to_rgba8())
        .unwrap_or_else(|_| image::RgbaImage::new(1, 1));
    image::imageops::resize(
        &pixels,
        PREVIEW_PIXELS,
        PREVIEW_PIXELS,
        image::imageops::FilterType::Lanczos3,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_breaching_whale_in_light_mode_leaves_the_bundle_icon() {
        assert_eq!(dock_artwork(AppIconChoice::Breaching, false), None);
    }

    #[test]
    fn every_other_choice_and_appearance_sets_its_own_picture() {
        let cases = [
            (AppIconChoice::Breaching, true, BREACHING_DARK),
            (AppIconChoice::UpClose, false, UP_CLOSE_LIGHT),
            (AppIconChoice::UpClose, true, UP_CLOSE_DARK),
        ];
        for (choice, dark, png) in cases {
            let artwork = dock_artwork(choice, dark).expect("not the bundle's icon");
            assert_eq!(artwork, IconArtwork { choice, dark });
            assert_eq!(artwork.png(), png, "{choice:?} dark={dark}");
        }
    }

    #[test]
    fn every_picture_decodes_to_a_square_preview() {
        for choice in AppIconChoice::ALL {
            for dark in [false, true] {
                let pixels = preview_pixels(IconArtwork { choice, dark });
                assert_eq!(pixels.dimensions(), (PREVIEW_PIXELS, PREVIEW_PIXELS));
            }
        }
    }
}
