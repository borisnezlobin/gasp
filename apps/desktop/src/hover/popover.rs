//! The hover preview's popover: a note as a small page with its title
//! above it, a footnote's text, or a line saying what's missing. It hangs
//! under the link, or above it when there's no room below.

use gpui::{
    AnyElement, ClickEvent, Context, Corner, Div, Entity, FontWeight, MouseButton, Pixels,
    SharedString, TextRun, Window, anchored, div, point, prelude::*,
};

use super::PreviewContent;
use crate::editor::EditorView;
use crate::frame::FrameLayout;
use crate::icons::IconName;
use crate::theme::UiTheme;
use crate::ui::{Button, IconButton, popover, truncated};

impl EditorView {
    /// The popover for the open preview, placed at the link. `None` while
    /// the note loads or when the link is off screen.
    pub(crate) fn hover_popover(
        &self,
        frame: &FrameLayout,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let open = self.hover.open.as_ref()?;
        let anchor = *frame.range_rects(&open.range, &self.theme).first()?;
        let theme = crate::ui::ui_theme(cx);
        let (body, height) = match &open.content {
            PreviewContent::Loading => return None,
            PreviewContent::Note {
                title,
                folder,
                view,
                ..
            } => note_page(title, folder, view, &theme, cx),
            PreviewContent::Footnote { view } => footnote_text(view, &theme, cx),
            PreviewContent::Missing { link } => {
                missing_note(link.name(), &theme, cx.listener(Self::create_previewed))
            }
            PreviewContent::Message(message) => message_line(message, &theme, window),
        };
        let viewport = window.viewport_size().height;
        let gap = theme.suggestion_gap;
        let fits_below = anchor.bottom() + gap + height + theme.space_xl <= viewport;
        let (corner, y) = if fits_below {
            (Corner::TopLeft, anchor.bottom() + gap)
        } else {
            (Corner::BottomLeft, anchor.top() - gap)
        };
        let body = body
            .id("hover-preview")
            .debug_selector(|| "hover-preview".into())
            .occlude()
            .on_hover(
                cx.listener(|view, hovered: &bool, _, cx| view.hover_popover_hovered(*hovered, cx)),
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        Some(
            anchored()
                .anchor(corner)
                .position(point(anchor.left() - theme.menu_padding, y))
                .snap_to_window_with_margin(theme.space_md)
                .child(body)
                .into_any_element(),
        )
    }

    fn create_previewed(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.open_previewed(window, cx);
    }
}

/// A preview's surface: a popover in the note's own background, so the
/// page inside reads as the note.
fn surface(theme: &UiTheme) -> Div {
    popover(theme).bg(theme.note_background)
}

/// The note under its title. The title row opens the note; the page
/// scrolls on its own.
fn note_page(
    title: &str,
    folder: &str,
    view: &Entity<EditorView>,
    theme: &UiTheme,
    cx: &mut Context<EditorView>,
) -> (Div, Pixels) {
    let inset = theme.menu_padding * 2.;
    let room = theme.hover_preview_height - theme.hover_preview_header_height - inset;
    let page = view.read(cx).content_height().min(room);
    let height = theme.hover_preview_header_height + page + inset;
    let header = div()
        .id("hover-preview-title")
        .flex()
        .flex_none()
        .items_center()
        .gap(theme.space_md)
        .h(theme.hover_preview_header_height)
        .pl(theme.hover_preview_padding)
        .pr(theme.space_xs)
        .rounded(theme.menu_row_radius)
        .cursor_pointer()
        .hover(|style| style.bg(theme.row_hover))
        .on_click(cx.listener(|view, _: &ClickEvent, window, cx| view.open_previewed(window, cx)))
        .child(
            div()
                .min_w_0()
                .font_weight(FontWeight::SEMIBOLD)
                .child(truncated(SharedString::from(title.to_owned()))),
        )
        .when(!folder.is_empty(), |header| {
            header.child(
                div()
                    .min_w_0()
                    .flex_shrink()
                    .text_size(theme.small_font_size)
                    .text_color(theme.text_detail)
                    .child(truncated(SharedString::from(folder.to_owned()))),
            )
        })
        .child(div().flex_1())
        .child(
            IconButton::new("hover-preview-open", IconName::ArrowSquareOut)
                .small()
                .tooltip("Open note")
                .on_click(
                    cx.listener(|view, _: &ClickEvent, window, cx| view.open_previewed(window, cx)),
                ),
        );
    let body = surface(theme)
        .w(theme.hover_preview_width)
        .h(height)
        .child(header)
        .child(div().h(page).overflow_hidden().child(view.clone()));
    (body, height)
}

/// A footnote's text, as tall as it needs up to the preview's height.
fn footnote_text(
    view: &Entity<EditorView>,
    theme: &UiTheme,
    cx: &mut Context<EditorView>,
) -> (Div, Pixels) {
    let inset = theme.menu_padding * 2.;
    let page = view
        .read(cx)
        .content_height()
        .min(theme.hover_preview_height - inset);
    let body = surface(theme)
        .w(theme.hover_footnote_width)
        .h(page + inset)
        .child(div().h(page).overflow_hidden().child(view.clone()));
    (body, page + inset)
}

/// A link to a note that isn't there yet, with the button that makes it.
fn missing_note(
    name: &str,
    theme: &UiTheme,
    on_create: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> (Div, Pixels) {
    let body = surface(theme)
        .p(theme.popover_padding)
        .gap(theme.space_md)
        .max_w(theme.hover_footnote_width)
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .child(truncated(SharedString::from(name.to_owned()))),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(theme.space_lg)
                .child(
                    div()
                        .flex_1()
                        .text_color(theme.text_muted)
                        .child("This note doesn't exist yet."),
                )
                .child(Button::new("hover-preview-create", "Create").on_click(on_create)),
        );
    let height = theme.popover_padding * 2. + theme.menu_row_height + theme.button_height;
    (body, height)
}

/// One line of explanation, such as a footnote with no definition.
/// It hugs a short message and wraps a long one at the footnote width.
/// The width is measured here because a popover is laid out at its
/// content's natural width, where text doesn't know to wrap.
fn message_line(message: &str, theme: &UiTheme, window: &Window) -> (Div, Pixels) {
    let run = TextRun {
        len: message.len(),
        font: gpui::font(theme.font_family.clone()),
        color: theme.text_muted,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let natural = window
        .text_system()
        .shape_line(message.to_owned().into(), theme.font_size, &[run], None)
        .width;
    let padding = theme.popover_padding;
    let width = (natural + padding * 2. + theme.space_xs).min(theme.hover_footnote_width);
    let body = surface(theme)
        .px(padding)
        .py(theme.space_md)
        .w(width)
        .text_color(theme.text_muted)
        .child(message.to_owned());
    (body, theme.menu_row_height + theme.space_md * 2.)
}
