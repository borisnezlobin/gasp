//! Scrolling the tree while something is dragged near its top or bottom,
//! so a folder out of view can be reached without letting go.
//!
//! A pointer held still sends no events, so the scrolling runs on its own
//! timer once a drag comes near an edge, and stops when the pointer moves
//! away, the drag ends or the list can't go further.

use gpui::{Bounds, Context, Pixels, Point, Window, point};

use super::view::FileTree;
use crate::theme::UiTheme;
use crate::ui::ui_theme;

/// Which way, and how hard, a pointer at `position` pushes a list drawn
/// in `area`: -1 at its very top, 1 at its very bottom, easing to 0 at
/// the inner side of a `band` along each edge. `None` anywhere else.
pub(super) fn edge_push(
    position: Point<Pixels>,
    area: Bounds<Pixels>,
    band: Pixels,
) -> Option<f32> {
    if !area.contains(&position) {
        return None;
    }
    // A list too short for two bands splits it between them.
    let band = band.min(area.size.height / 2.);
    let from_top = position.y - area.top();
    let from_bottom = area.bottom() - position.y;
    if from_top < band {
        Some(from_top / band - 1.)
    } else if from_bottom < band {
        Some(1. - from_bottom / band)
    } else {
        None
    }
}

impl FileTree {
    /// Starts scrolling when a drag, of an entry or anything else, comes
    /// near the list's top or bottom.
    pub(super) fn autoscroll_on_move(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.autoscroll.is_some() || !cx.has_active_drag() {
            return;
        }
        let ui = ui_theme(cx);
        if self.push_at(window.mouse_position(), &ui).is_none() {
            return;
        }
        let frame = ui.tree_autoscroll_frame;
        self.autoscroll = Some(cx.spawn_in(window, async move |tree, cx| {
            loop {
                cx.background_executor().timer(frame).await;
                let going = tree
                    .update_in(cx, |tree, window, cx| tree.autoscroll_step(window, cx))
                    .unwrap_or(false);
                if !going {
                    break;
                }
            }
        }));
    }

    /// Scrolls one frame's worth toward the edge the pointer is at.
    /// Returns whether to keep going.
    fn autoscroll_step(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let ui = ui_theme(cx);
        let push = match cx.has_active_drag() {
            true => self.push_at(window.mouse_position(), &ui),
            false => None,
        };
        let step = ui.tree_autoscroll_speed * ui.tree_autoscroll_frame.as_secs_f32();
        let moved = push.is_some_and(|push| self.scroll_by(step * push));
        if moved {
            cx.notify();
        } else {
            self.autoscroll = None;
        }
        moved
    }

    fn push_at(&self, position: Point<Pixels>, ui: &UiTheme) -> Option<f32> {
        let rows = self.scroll.0.borrow().base_handle.bounds();
        // The list's padding, around the rows, counts as their edge too.
        let area = Bounds::from_corners(
            point(rows.left(), rows.top() - ui.space_sm),
            point(rows.right(), rows.bottom() + ui.space_sm),
        );
        edge_push(position, area, ui.tree_autoscroll_band)
    }

    /// Scrolls the rows down by `delta`, or up for a negative one,
    /// stopping at either end. Returns whether they moved.
    fn scroll_by(&self, delta: Pixels) -> bool {
        let handle = self.scroll.0.borrow().base_handle.clone();
        let offset = handle.offset();
        let end = -handle.max_offset().height;
        let y = (offset.y - delta).clamp(end, Pixels::ZERO);
        if y == offset.y {
            return false;
        }
        handle.set_offset(point(offset.x, y));
        true
    }
}

#[cfg(test)]
mod tests {
    use gpui::{px, size};

    use super::*;

    fn area() -> Bounds<Pixels> {
        Bounds::new(point(px(0.), px(100.)), size(px(200.), px(400.)))
    }

    fn push(y: f32) -> Option<f32> {
        edge_push(point(px(50.), px(y)), area(), px(60.))
    }

    #[test]
    fn pushes_harder_nearer_the_edge() {
        assert_eq!(push(100.), Some(-1.));
        assert_eq!(push(130.), Some(-0.5));
        assert_eq!(push(300.), None);
        assert_eq!(push(470.), Some(0.5));
        assert_eq!(push(500.), Some(1.));
    }

    #[test]
    fn nothing_outside_the_list() {
        assert_eq!(push(99.), None);
        assert_eq!(push(501.), None);
        assert_eq!(edge_push(point(px(250.), px(110.)), area(), px(60.)), None);
    }

    #[test]
    fn a_short_list_shares_itself_between_the_bands() {
        let short = Bounds::new(point(px(0.), px(0.)), size(px(200.), px(40.)));
        assert_eq!(
            edge_push(point(px(50.), px(10.)), short, px(60.)),
            Some(-0.5)
        );
        assert_eq!(
            edge_push(point(px(50.), px(30.)), short, px(60.)),
            Some(0.5)
        );
    }
}
