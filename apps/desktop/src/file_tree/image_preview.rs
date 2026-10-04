//! A preview of an image file while the pointer rests on its row. After
//! the same pause as a link's preview, the file is decoded off the main
//! thread, shrunk to about the size it's drawn, and shown beside the row.
//! Previews are kept by path and modification time, so hovering the same
//! image again costs nothing.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use gpui::{
    AnyElement, Context, Pixels, RenderImage, Size, Task, anchored, deferred, div, img, prelude::*,
    px, size,
};

use super::view::FileTree;
use crate::images::{Decode, decode_file};
use crate::theme::UiTheme;
use crate::ui::Selectable;
use crate::ui::{popover, ui_theme};

/// Decoded previews kept for hovering again.
const KEPT_PREVIEWS: usize = 8;

#[derive(Default)]
pub(super) struct ImagePreview {
    hovered: Option<PathBuf>,
    shown: Option<Preview>,
    kept: Vec<Preview>,
    _loading: Option<Task<()>>,
}

#[derive(Clone)]
struct Preview {
    path: PathBuf,
    modified: Option<SystemTime>,
    image: Arc<RenderImage>,
    /// The file's own size in pixels.
    natural: (u32, u32),
}

impl ImagePreview {
    fn kept(&self, path: &Path, modified: Option<SystemTime>) -> Option<Preview> {
        self.kept
            .iter()
            .find(|preview| preview.path == path && preview.modified == modified)
            .cloned()
    }

    fn keep(&mut self, preview: Preview) {
        self.kept.retain(|kept| kept.path != preview.path);
        self.kept.push(preview);
        if self.kept.len() > KEPT_PREVIEWS {
            self.kept.remove(0);
        }
    }

    fn hide(&mut self) {
        self.hovered = None;
        self.shown = None;
        self._loading = None;
    }
}

impl FileTree {
    /// Starts or stops the preview of the image at `path` (vault-relative)
    /// as the pointer reaches or leaves its row. `pixel_width` is how many
    /// device pixels wide the preview may be drawn.
    pub(super) fn image_row_hovered(
        &mut self,
        path: PathBuf,
        hovered: bool,
        pixel_width: u32,
        cx: &mut Context<Self>,
    ) {
        if !hovered {
            if self.image_preview.hovered.as_ref() == Some(&path) {
                self.image_preview.hide();
                cx.notify();
            }
            return;
        }
        let delay = ui_theme(cx).hover_preview_delay;
        let file = self.absolute(&path);
        self.image_preview.hovered = Some(path.clone());
        self.image_preview._loading = Some(cx.spawn(async move |tree, cx| {
            cx.background_executor().timer(delay).await;
            let modified = cx
                .background_executor()
                .spawn({
                    let file = file.clone();
                    async move {
                        std::fs::metadata(file)
                            .and_then(|meta| meta.modified())
                            .ok()
                    }
                })
                .await;
            let kept = tree
                .read_with(cx, |tree, _| tree.image_preview.kept(&path, modified))
                .ok()
                .flatten();
            let preview = match kept {
                Some(preview) => Some(preview),
                None => {
                    cx.background_executor()
                        .spawn(decode_preview(path.clone(), file, modified, pixel_width))
                        .await
                }
            };
            tree.update(cx, |tree, cx| tree.show_image_preview(&path, preview, cx))
                .ok();
        }));
    }

    fn show_image_preview(
        &mut self,
        path: &Path,
        preview: Option<Preview>,
        cx: &mut Context<Self>,
    ) {
        if self.image_preview.hovered.as_deref() != Some(path) {
            return;
        }
        if let Some(preview) = &preview {
            self.image_preview.keep(preview.clone());
        }
        self.image_preview.shown = preview;
        cx.notify();
    }

    /// The preview beside the row for `path`, while it shows.
    pub(super) fn render_image_preview(&self, path: &Path, ui: &UiTheme) -> Option<AnyElement> {
        let preview = self
            .image_preview
            .shown
            .as_ref()
            .filter(|preview| preview.path == path)?;
        let drawn = fit_within(preview.natural, ui.image_preview_size);
        let inner_radius = (ui.menu_radius - ui.menu_padding).max(px(2.));
        let (width, height) = preview.natural;
        let panel = popover(ui)
            .id("file-tree-image-preview")
            .selector(|| "file-tree-image-preview".to_owned())
            .ml(ui.space_lg)
            .gap(ui.space_sm)
            .child(
                img(preview.image.clone())
                    .w(drawn.width)
                    .h(drawn.height)
                    .rounded(inner_radius),
            )
            .child(
                div()
                    .px(ui.space_xs)
                    .text_size(ui.small_font_size)
                    .text_color(ui.text_muted)
                    .child(format!("{width} × {height}")),
            );
        let beside_row = div()
            .absolute()
            .top_0()
            .left_full()
            .child(deferred(anchored().snap_to_window().child(panel)).with_priority(1));
        Some(beside_row.into_any_element())
    }
}

async fn decode_preview(
    path: PathBuf,
    file: PathBuf,
    modified: Option<SystemTime>,
    pixel_width: u32,
) -> Option<Preview> {
    let natural = image::image_dimensions(&file).ok()?;
    let decode = Decode {
        target: path.to_string_lossy().into_owned(),
        path: file,
        width: pixel_width,
    };
    let image = decode_file(&decode)?;
    Some(Preview {
        path,
        modified,
        image: Arc::new(image),
        natural,
    })
}

/// `natural` scaled down, keeping its shape, until neither side is longer
/// than `longest`. Small images keep their own size.
fn fit_within(natural: (u32, u32), longest: Pixels) -> Size<Pixels> {
    let (width, height) = (natural.0.max(1) as f32, natural.1.max(1) as f32);
    let scale = (f32::from(longest) / width.max(height)).min(1.);
    size(px(width * scale), px(height * scale))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_fit_their_longest_side() {
        assert_eq!(fit_within((1200, 600), px(300.)), size(px(300.), px(150.)));
        assert_eq!(fit_within((400, 1600), px(300.)), size(px(75.), px(300.)));
        assert_eq!(fit_within((64, 32), px(300.)), size(px(64.), px(32.)));
    }
}
