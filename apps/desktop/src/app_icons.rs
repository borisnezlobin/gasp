//! Other apps' icons, such as Claude's on the Settings page's AI app
//! tiles. They're read from the apps' own bundles while Gasp runs, so Gasp
//! ships no other company's logo, then kept for as long as Gasp runs.
//! Reading one asks AppKit to draw it, so it happens off the main thread.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{App, AppContext, Context, Global, RenderImage};

#[cfg(target_os = "macos")]
mod macos;

/// How many pixels square an icon is drawn at: a 32-point tile on a
/// Retina screen.
pub const ICON_PIXELS: u32 = 64;

/// Where one bundle's icon has got to.
#[derive(Clone)]
enum Loaded {
    Loading,
    Ready(Arc<RenderImage>),
    Missing,
}

/// Every bundle's icon asked for since Gasp started.
#[derive(Default)]
struct AppIcons(HashMap<PathBuf, Loaded>);

impl Global for AppIcons {}

/// The icon of the app bundle at `bundle`, once it has been read. `None`
/// while it's being read, and `Some(None)` when there's no icon to show.
pub fn app_icon(bundle: &Path, cx: &App) -> Option<Option<Arc<RenderImage>>> {
    match cx.try_global::<AppIcons>()?.0.get(bundle)? {
        Loaded::Loading => None,
        Loaded::Ready(image) => Some(Some(image.clone())),
        Loaded::Missing => Some(None),
    }
}

/// Starts reading the icon of each bundle not read yet, off the main
/// thread, and redraws `view` as each one arrives.
pub fn load_app_icons<V: 'static>(bundles: Vec<PathBuf>, cx: &mut Context<V>) {
    for bundle in bundles {
        let icons = cx.default_global::<AppIcons>();
        if icons.0.contains_key(&bundle) {
            continue;
        }
        icons.0.insert(bundle.clone(), Loaded::Loading);
        let path = bundle.clone();
        let reading = cx.background_spawn(async move { read_app_icon(&path) });
        cx.spawn(async move |view, cx| {
            let image = reading.await;
            view.update(cx, |_, cx| {
                let loaded = image.map_or(Loaded::Missing, |image| Loaded::Ready(Arc::new(image)));
                cx.default_global::<AppIcons>().0.insert(bundle, loaded);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

/// The icon Finder shows for `bundle`, [`ICON_PIXELS`] square. Slow, so
/// call it off the main thread.
pub fn read_app_icon(bundle: &Path) -> Option<RenderImage> {
    let png = icon_png(bundle)?;
    let pixels = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    Some(crate::images::render_image(to_icon_size(pixels)))
}

/// `pixels` at [`ICON_PIXELS`] square, for an icon AppKit drew at
/// another size.
fn to_icon_size(pixels: image::RgbaImage) -> image::RgbaImage {
    if pixels.dimensions() == (ICON_PIXELS, ICON_PIXELS) {
        return pixels;
    }
    image::imageops::resize(
        &pixels,
        ICON_PIXELS,
        ICON_PIXELS,
        image::imageops::FilterType::Lanczos3,
    )
}

#[cfg(target_os = "macos")]
fn icon_png(bundle: &Path) -> Option<Vec<u8>> {
    macos::icon_png(bundle, ICON_PIXELS)
}

#[cfg(not(target_os = "macos"))]
fn icon_png(_: &Path) -> Option<Vec<u8>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_icon_drawn_at_another_size_is_scaled_to_the_tiles() {
        let large = image::RgbaImage::new(ICON_PIXELS * 2, ICON_PIXELS * 2);
        assert_eq!(to_icon_size(large).dimensions(), (ICON_PIXELS, ICON_PIXELS));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_bundle_on_disk_gives_a_square_icon() {
        let folder = tempfile::tempdir().unwrap();
        let bundle = folder.path().join("Example.app");
        std::fs::create_dir_all(&bundle).unwrap();
        let png = icon_png(&bundle).expect("Finder has an icon for any folder");
        assert!(png.starts_with(b"\x89PNG"));
        let image = read_app_icon(&bundle).expect("the icon decodes");
        assert_eq!(image.size(0).width.0, ICON_PIXELS as i32);
    }
}
