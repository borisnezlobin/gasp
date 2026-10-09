//! The window's own buttons and edges, drawn by the app where the desktop
//! leaves them to it: GNOME on Wayland gives windows no title bar, so
//! without these the window couldn't be minimized, maximized, closed,
//! resized or moved. Everywhere else (macOS, X11, KDE and other desktops
//! that draw a title bar) the system's stay and nothing here is drawn.
//!
//! The buttons sit at the window's top-right corner, over the end of the
//! top-right tab bar or the right sidebar's header, which leave
//! [`width`] free for them. The tab bar's empty space moves the window
//! (`crate::window_drag`).

use gpui::{
    AnyElement, App, CursorStyle, Decorations, MouseButton, Pixels, ResizeEdge, Tiling, Window,
    div, prelude::*, px,
};

use crate::icons::IconName;
use crate::ui::{IconButton, Selectable, ui_theme};

/// How far in from each edge a press resizes the window.
const EDGE: Pixels = px(5.);
/// The square at each corner where a press resizes both ways.
const CORNER: Pixels = px(10.);

/// Whether the app draws the window's buttons and edges itself.
pub fn drawn_by_app(window: &Window) -> bool {
    matches!(window.window_decorations(), Decorations::Client { .. })
}

/// Room the buttons take at the window's top-right: none when the system
/// draws them.
pub fn width(window: &Window, cx: &mut App) -> Pixels {
    if !drawn_by_app(window) {
        return px(0.);
    }
    let ui = ui_theme(cx);
    let count = buttons(window).len() as f32;
    ui.icon_button_size * count + ui.space_xs * (count - 1.) + ui.space_sm * 2.
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Button {
    Minimize,
    Maximize,
    Close,
}

/// The buttons this desktop supports, left to right. Close is always one.
fn buttons(window: &Window) -> Vec<Button> {
    let controls = window.window_controls();
    let mut buttons = Vec::with_capacity(3);
    if controls.minimize {
        buttons.push(Button::Minimize);
    }
    if controls.maximize {
        buttons.push(Button::Maximize);
    }
    buttons.push(Button::Close);
    buttons
}

fn button(kind: Button, window: &Window) -> IconButton {
    match kind {
        Button::Minimize => IconButton::new("window-minimize", IconName::Minus)
            .tooltip("Minimize")
            .on_click(|_, window, _| window.minimize_window()),
        Button::Maximize => {
            let (icon, label) = if window.is_maximized() {
                (IconName::Copy, "Restore")
            } else {
                (IconName::Square, "Maximize")
            };
            IconButton::new("window-maximize", icon)
                .tooltip(label)
                .on_click(|_, window, _| window.zoom_window())
        }
        Button::Close => IconButton::new("window-close", IconName::X)
            .tooltip("Close window")
            .on_click(|_, window, _| window.remove_window()),
    }
}

/// The buttons, pinned to the window's top-right corner, when the app
/// draws them.
pub fn render_buttons(window: &Window, cx: &mut App) -> Option<AnyElement> {
    if !drawn_by_app(window) {
        return None;
    }
    let ui = ui_theme(cx);
    let buttons = buttons(window).into_iter().map(|kind| button(kind, window));
    Some(
        div()
            .id("window-controls")
            .selector(|| "window-controls".to_owned())
            .absolute()
            .top_0()
            .right_0()
            .h(ui.tab_bar_height)
            .px(ui.space_sm)
            .flex()
            .flex_row()
            .items_center()
            .gap(ui.space_xs)
            // A press between the buttons is theirs, not the tab bar's.
            .on_mouse_down(MouseButton::Left, |_, _, cx: &mut App| {
                cx.stop_propagation()
            })
            .children(buttons)
            .into_any_element(),
    )
}

/// Strips along the window's edges and corners that resize it, when the
/// app draws its edges. A maximized or tiled edge has none.
pub fn render_edges(window: &Window) -> Option<AnyElement> {
    let Decorations::Client { tiling } = window.window_decorations() else {
        return None;
    };
    if window.is_maximized() || window.is_fullscreen() {
        return None;
    }
    let edges = EDGES
        .iter()
        .filter(|(edge, _)| !is_tiled(*edge, tiling))
        .map(|(edge, cursor)| edge_strip(*edge, *cursor));
    Some(
        div()
            .absolute()
            .size_full()
            .top_0()
            .left_0()
            .children(edges)
            .into_any_element(),
    )
}

const EDGES: [(ResizeEdge, CursorStyle); 8] = [
    (ResizeEdge::Top, CursorStyle::ResizeUpDown),
    (ResizeEdge::Bottom, CursorStyle::ResizeUpDown),
    (ResizeEdge::Left, CursorStyle::ResizeLeftRight),
    (ResizeEdge::Right, CursorStyle::ResizeLeftRight),
    (ResizeEdge::TopLeft, CursorStyle::ResizeUpLeftDownRight),
    (ResizeEdge::BottomRight, CursorStyle::ResizeUpLeftDownRight),
    (ResizeEdge::TopRight, CursorStyle::ResizeUpRightDownLeft),
    (ResizeEdge::BottomLeft, CursorStyle::ResizeUpRightDownLeft),
];

fn is_tiled(edge: ResizeEdge, tiling: Tiling) -> bool {
    match edge {
        ResizeEdge::Top => tiling.top,
        ResizeEdge::Bottom => tiling.bottom,
        ResizeEdge::Left => tiling.left,
        ResizeEdge::Right => tiling.right,
        ResizeEdge::TopLeft => tiling.top || tiling.left,
        ResizeEdge::TopRight => tiling.top || tiling.right,
        ResizeEdge::BottomLeft => tiling.bottom || tiling.left,
        ResizeEdge::BottomRight => tiling.bottom || tiling.right,
    }
}

fn edge_strip(edge: ResizeEdge, cursor: CursorStyle) -> impl IntoElement {
    let strip = div()
        .id(gpui::SharedString::from(format!("window-edge-{edge:?}")))
        .absolute()
        .occlude()
        .cursor(cursor)
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            window.start_window_resize(edge);
        });
    match edge {
        ResizeEdge::Top => strip.top_0().left(CORNER).right(CORNER).h(EDGE),
        ResizeEdge::Bottom => strip.bottom_0().left(CORNER).right(CORNER).h(EDGE),
        ResizeEdge::Left => strip.left_0().top(CORNER).bottom(CORNER).w(EDGE),
        ResizeEdge::Right => strip.right_0().top(CORNER).bottom(CORNER).w(EDGE),
        ResizeEdge::TopLeft => strip.top_0().left_0().size(CORNER),
        ResizeEdge::TopRight => strip.top_0().right_0().size(CORNER),
        ResizeEdge::BottomLeft => strip.bottom_0().left_0().size(CORNER),
        ResizeEdge::BottomRight => strip.bottom_0().right_0().size(CORNER),
    }
}
