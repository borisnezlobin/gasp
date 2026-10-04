//! A tab showing one image file. The image fits the pane, never drawn
//! larger than its own pixels, and a click flips between that and actual
//! size, where one larger than the pane scrolls. The file is decoded off
//! the main thread, and again whenever its modification time changes.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::SystemTime;

use gpui::{
    AnyElement, App, Context, CursorStyle, FocusHandle, Focusable, Pixels, RenderImage,
    ScrollHandle, Size, Subscription, Task, Window, canvas, div, img, prelude::*, px, size,
};

use crate::images::{Decode, decode_file};
use crate::theme::UiTheme;
use crate::ui::{Selectable, ui_theme};

const MISSING_MESSAGE: &str = "This image isn’t there anymore. It may have been moved or deleted.";
const UNDECODABLE_MESSAGE: &str = "Gasp can’t show this image.";

/// What the tab has to show.
#[derive(Clone)]
enum Picture {
    Loading,
    Shown {
        image: Arc<RenderImage>,
        /// The file's own size in pixels.
        natural: (u32, u32),
    },
    Undecodable,
    Missing,
}

/// The file as it was when last read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stamp {
    Unread,
    Gone,
    Modified(Option<SystemTime>),
}

pub struct ImageView {
    path: PathBuf,
    focus_handle: FocusHandle,
    picture: Picture,
    stamp: Stamp,
    actual_size: bool,
    scroll: ScrollHandle,
    /// The room the image has, measured as it's drawn.
    room: Rc<Cell<Size<Pixels>>>,
    _loading: Option<Task<()>>,
    _activation: Subscription,
}

impl Focusable for ImageView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ImageView {
    /// A view of the image at the absolute `path`. It decodes the file the
    /// first time it's refreshed, which its pane does when it shows it.
    pub fn new(path: &Path, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let activation = cx.observe_window_activation(window, |view: &mut Self, window, cx| {
            if window.is_window_active() {
                view.refresh(cx);
            }
        });
        ImageView {
            path: path.to_path_buf(),
            focus_handle: cx.focus_handle(),
            picture: Picture::Loading,
            stamp: Stamp::Unread,
            actual_size: false,
            scroll: ScrollHandle::new(),
            room: Rc::default(),
            _loading: None,
            _activation: activation,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The file's own size in pixels, once it has decoded.
    pub fn natural_size(&self) -> Option<(u32, u32)> {
        match self.picture {
            Picture::Shown { natural, .. } => Some(natural),
            _ => None,
        }
    }

    /// Whether the file is gone.
    pub fn is_missing(&self) -> bool {
        matches!(self.picture, Picture::Missing)
    }

    /// Follows the file to where it moved.
    pub fn set_path(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.path = path.to_path_buf();
        self.refresh(cx);
    }

    /// Reads the file again, off the main thread, if its modification time
    /// changed or it came or went since it was last read.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let (path, stamp) = (self.path.clone(), self.stamp);
        self._loading = Some(cx.spawn(async move |view, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { read_if_changed(&path, stamp) })
                .await;
            let Some((picture, stamp)) = read else {
                return;
            };
            view.update(cx, |view, cx| view.show(picture, stamp, cx))
                .ok();
        }));
    }

    fn show(&mut self, picture: Picture, stamp: Stamp, cx: &mut Context<Self>) {
        self.picture = picture;
        self.stamp = stamp;
        cx.notify();
    }

    fn toggle_actual_size(&mut self, cx: &mut Context<Self>) {
        self.actual_size = !self.actual_size;
        self.scroll.set_offset(gpui::point(px(0.), px(0.)));
        cx.notify();
    }

    fn render_picture(
        &self,
        image: Arc<RenderImage>,
        natural: (u32, u32),
        window: &Window,
        ui: &UiTheme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let actual = actual_size(natural, window.scale_factor());
        let room = self.room.get();
        let picture = if self.actual_size {
            self.render_actual_size(image, actual, room, ui)
        } else {
            render_fitted(image, fitted_size(actual, room, ui.image_tab_margin))
        };
        div()
            .id("image-tab-picture")
            .selector(|| "image-tab-picture".to_owned())
            .size_full()
            .cursor(CursorStyle::PointingHand)
            .on_click(cx.listener(|view, _, _, cx| view.toggle_actual_size(cx)))
            .child(picture)
            .into_any_element()
    }

    /// The image at one image pixel to one screen pixel, centred, and
    /// scrolling when it's larger than the pane.
    fn render_actual_size(
        &self,
        image: Arc<RenderImage>,
        actual: Size<Pixels>,
        room: Size<Pixels>,
        ui: &UiTheme,
    ) -> AnyElement {
        let margin = ui.image_tab_margin * 2.;
        let content = div()
            .flex_none()
            .w((actual.width + margin).max(room.width))
            .h((actual.height + margin).max(room.height))
            .flex()
            .items_center()
            .justify_center()
            .child(img(image).flex_none().w(actual.width).h(actual.height));
        div()
            .id("image-tab-scroll")
            .size_full()
            .overflow_scroll()
            .track_scroll(&self.scroll)
            .child(content)
            .into_any_element()
    }

    /// Keeps the room the image has up to date, drawing again when it
    /// changes, as when the pane is resized.
    fn measure_room(&self) -> impl IntoElement {
        let room = self.room.clone();
        canvas(
            move |bounds, window, _| {
                if room.get() != bounds.size {
                    room.set(bounds.size);
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
    }

    fn render_size_line(&self, ui: &UiTheme) -> impl IntoElement {
        let label = self
            .natural_size()
            .map(|(width, height)| format!("{width} × {height}"));
        div()
            .id("image-tab-size")
            .selector(|| "image-tab-size".to_owned())
            .flex_none()
            .flex()
            .justify_center()
            .h(ui.text_line_height)
            .mb(ui.space_md)
            .text_size(ui.small_font_size)
            .line_height(ui.text_line_height)
            .text_color(ui.text_faint)
            .children(label)
    }
}

/// `actual` shrunk to fit `room` less a margin on each side, keeping its
/// shape. One that fits keeps its size.
fn fitted_size(actual: Size<Pixels>, room: Size<Pixels>, margin: Pixels) -> Size<Pixels> {
    let free = |room: Pixels| f32::from((room - margin * 2.).max(px(0.)));
    let scale = (free(room.width) / f32::from(actual.width))
        .min(free(room.height) / f32::from(actual.height))
        .min(1.);
    size(actual.width * scale, actual.height * scale)
}

fn render_fitted(image: Arc<RenderImage>, fitted: Size<Pixels>) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(img(image).flex_none().w(fitted.width).h(fitted.height))
        .into_any_element()
}

fn render_message(message: &'static str, ui: &UiTheme) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .p(ui.image_tab_margin)
        .text_size(ui.font_size)
        .text_color(ui.text_muted)
        .child(message)
        .into_any_element()
}

/// How large `natural` pixels are on a screen of `scale_factor`.
fn actual_size(natural: (u32, u32), scale_factor: f32) -> Size<Pixels> {
    let scale = scale_factor.max(1.);
    size(
        px(natural.0.max(1) as f32 / scale),
        px(natural.1.max(1) as f32 / scale),
    )
}

fn stamp_of(path: &Path) -> Stamp {
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() => Stamp::Modified(meta.modified().ok()),
        _ => Stamp::Gone,
    }
}

/// The file's picture and stamp, unless it's as it was at `known`.
fn read_if_changed(path: &Path, known: Stamp) -> Option<(Picture, Stamp)> {
    let stamp = stamp_of(path);
    if stamp == known {
        return None;
    }
    let picture = match stamp {
        Stamp::Modified(_) => decode_picture(path),
        Stamp::Gone | Stamp::Unread => Picture::Missing,
    };
    Some((picture, stamp))
}

fn decode_picture(path: &Path) -> Picture {
    let Ok(natural) = image::image_dimensions(path) else {
        return Picture::Undecodable;
    };
    let decode = Decode {
        target: path.to_string_lossy().into_owned(),
        path: path.to_path_buf(),
        width: natural.0,
    };
    match decode_file(&decode) {
        Some(image) => Picture::Shown {
            image: Arc::new(image),
            natural,
        },
        None => Picture::Undecodable,
    }
}

impl Render for ImageView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = ui_theme(cx);
        let body = match self.picture.clone() {
            Picture::Shown { image, natural } => {
                self.render_picture(image, natural, window, &ui, cx)
            }
            Picture::Loading => div().into_any_element(),
            Picture::Missing => render_message(MISSING_MESSAGE, &ui),
            Picture::Undecodable => render_message(UNDECODABLE_MESSAGE, &ui),
        };
        div()
            .id("image-tab")
            .selector(|| "image-tab".to_owned())
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(self.measure_room())
                    .child(div().absolute().inset_0().child(body)),
            )
            .child(self.render_size_line(&ui))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_size_is_one_image_pixel_to_one_screen_pixel() {
        assert_eq!(actual_size((2560, 1640), 2.), size(px(1280.), px(820.)));
        assert_eq!(actual_size((300, 200), 1.), size(px(300.), px(200.)));
    }

    #[test]
    fn large_images_shrink_to_fit_and_small_ones_keep_their_size() {
        let room = size(px(564.), px(364.));
        let fitted = fitted_size(size(px(2000.), px(1000.)), room, px(32.));
        assert_eq!(fitted, size(px(500.), px(250.)));
        let small = size(px(60.), px(40.));
        assert_eq!(fitted_size(small, room, px(32.)), small);
    }

    #[test]
    fn an_unchanged_file_is_not_read_again() {
        let dir = std::env::temp_dir().join(format!("gasp-image-tab-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pic.png");
        image::RgbaImage::new(4, 3).save(&path).unwrap();
        let (picture, stamp) = read_if_changed(&path, Stamp::Unread).unwrap();
        assert!(matches!(
            picture,
            Picture::Shown {
                natural: (4, 3),
                ..
            }
        ));
        assert!(read_if_changed(&path, stamp).is_none());
        std::fs::remove_file(&path).unwrap();
        let (picture, _) = read_if_changed(&path, stamp).unwrap();
        assert!(matches!(picture, Picture::Missing));
        std::fs::remove_dir_all(&dir).ok();
    }
}
