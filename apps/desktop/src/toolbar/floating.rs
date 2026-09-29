//! Bars that float over the note: by the selection, just above it, or at
//! the end of the cursor's line. The editor draws them, since it knows
//! where its text is on screen, and keeps what else they need: whether
//! typing has paused, which toggles are on at the cursor, and whether the
//! keyboard is in one of them.

use std::time::Duration;

use gasp_config::toolbars::{
    Place, Toolbar, ToolbarConditions, ToolbarContext, ToolbarMenu, Toolbars,
};
use gpui::{
    AnyElement, AvailableSpace, Bounds, Context, Pixels, Point, Size, Task, Window, point,
    prelude::*,
};

use super::render::{BarFrame, BarState, bar, bar_items};
use super::{ToolbarFocus, toolbar_context};
use crate::editor::EditorView;
use crate::frame::FrameLayout;
use crate::ui::Selectable;

/// The floating bars a vault's toolbars have, and their state in one
/// editor.
#[derive(Default)]
pub struct FloatingToolbars {
    toolbars: Vec<Toolbar>,
    menus: Vec<ToolbarMenu>,
    typing_pause: Duration,
    /// Whether the note was typed in less than the typing pause ago.
    typing: bool,
    /// Redraws once typing has paused, so bars hidden while typing return.
    typing_timer: Option<Task<()>>,
    /// Which bar the keyboard is in, while it's one of these.
    focus: Option<ToolbarFocus>,
    /// Where each bar was last drawn: its bottom-left corner, where a
    /// menu it opens from the keyboard hangs.
    placed: Vec<(String, Point<Pixels>)>,
}

impl FloatingToolbars {
    pub fn new(toolbars: &Toolbars) -> FloatingToolbars {
        let mut floating = FloatingToolbars::default();
        floating.configure(toolbars);
        floating
    }

    /// Takes the floating bars from a vault's toolbars.
    pub fn configure(&mut self, toolbars: &Toolbars) {
        self.toolbars = toolbars
            .toolbars
            .iter()
            .filter(|toolbar| toolbar.enabled && toolbar.place.is_floating())
            .cloned()
            .collect();
        self.menus = toolbars.menus.clone();
        self.typing_pause = toolbars.timing.typing_pause;
    }

    pub fn toolbars(&self) -> &[Toolbar] {
        &self.toolbars
    }
}

impl EditorView {
    /// Notes that the person typed, so bars hidden while typing stay
    /// hidden until typing pauses, then come back.
    pub(crate) fn note_typing(&mut self, cx: &mut Context<Self>) {
        let pause = self.floating.typing_pause;
        self.floating.typing = true;
        self.floating.typing_timer = Some(cx.spawn(async move |view, cx| {
            cx.background_executor().timer(pause).await;
            view.update(cx, |view, cx| {
                view.floating.typing = false;
                cx.notify();
            })
            .ok();
        }));
    }

    /// Whether the person is typing: it hasn't paused as long as the
    /// toolbars' `typing-pause` yet.
    pub fn is_typing(&self) -> bool {
        self.floating.typing
    }

    /// The toggle commands that are on at the cursor, such as bold.
    pub fn active_commands(&self) -> Vec<&'static str> {
        gasp_core::commands::active_commands(self.source.tree(), self.cursor())
    }

    /// The kind of text the cursor is in, for bars shown in a context.
    pub fn toolbar_context(&self) -> Option<ToolbarContext> {
        toolbar_context(self.source.tree().context_at(self.cursor()))
    }

    /// What decides whether a bar about this note shows.
    pub fn toolbar_conditions(&self) -> ToolbarConditions {
        ToolbarConditions {
            has_selection: !self.selected_range().is_empty(),
            context: self.toolbar_context(),
            typing: self.is_typing(),
            hovered: self.pointer_at.is_some(),
            focused: false,
        }
    }

    /// Puts the keyboard in one of the floating bars, or out of them.
    pub fn set_toolbar_focus(&mut self, focus: Option<ToolbarFocus>, cx: &mut Context<Self>) {
        if self.floating.focus != focus {
            self.floating.focus = focus;
            cx.notify();
        }
    }

    /// The floating bars showing now, which the keyboard can move into.
    pub fn shown_floating_toolbars(&self) -> Vec<&Toolbar> {
        let conditions = self.toolbar_conditions();
        self.floating
            .toolbars
            .iter()
            .filter(|toolbar| self.floating_bar_shown(toolbar, &conditions))
            .collect()
    }

    /// Where floating bar `id` was last drawn: its bottom-left corner.
    pub fn floating_bar_origin(&self, id: &str) -> Option<Point<Pixels>> {
        self.floating
            .placed
            .iter()
            .find(|(placed, _)| placed == id)
            .map(|(_, origin)| *origin)
    }

    fn floating_bar_shown(&self, toolbar: &Toolbar, conditions: &ToolbarConditions) -> bool {
        let focused = self
            .floating
            .focus
            .as_ref()
            .is_some_and(|focus| focus.toolbar == toolbar.id);
        // A bar over the text waits for a drag selection to end, and
        // steps aside for suggestions.
        let busy = self.is_selecting || self.suggestions().is_some();
        let with = ToolbarConditions {
            focused,
            ..*conditions
        };
        toolbar.is_shown(&with) && (focused || !busy)
    }

    /// Lays out the floating bars for this frame and draws them over the
    /// text in `frame`. None show unless the editor has the keyboard or
    /// one of them does.
    pub(crate) fn draw_floating_toolbars(
        &mut self,
        frame: &FrameLayout,
        editor_focused: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.floating.placed.clear();
        let bar_focused = self.floating.focus.as_ref().is_some_and(|focus| {
            self.floating
                .toolbars
                .iter()
                .any(|toolbar| toolbar.id == focus.toolbar)
        });
        if self.read_only || !(editor_focused || bar_focused) {
            return;
        }
        let conditions = self.toolbar_conditions();
        let shown: Vec<Toolbar> = self
            .floating
            .toolbars
            .iter()
            .filter(|toolbar| self.floating_bar_shown(toolbar, &conditions))
            .cloned()
            .collect();
        for toolbar in &shown {
            self.draw_floating_bar(toolbar, frame, window, cx);
        }
    }

    /// Measures one bar, then places it by its text: it's laid out first
    /// so the placement knows its size and keeps all of it in the pane.
    fn draw_floating_bar(
        &mut self,
        toolbar: &Toolbar,
        frame: &FrameLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(anchor) = self.floating_anchor(toolbar.place, frame) else {
            return;
        };
        let gap = crate::ui::ui_theme(cx).toolbar.float_gap;
        let mut body = self.floating_bar_body(toolbar, cx);
        let bar_size = body.layout_as_root(AvailableSpace::min_size(), window, cx);
        let origin = anchor.origin(bar_size, frame.bounds, gap);
        let bottom_left = point(origin.x, origin.y + bar_size.height);
        self.floating.placed.push((toolbar.id.clone(), bottom_left));
        window.defer_draw(body, origin, crate::element::SUGGESTION_LAYER);
    }

    fn floating_bar_body(&mut self, toolbar: &Toolbar, cx: &mut Context<Self>) -> AnyElement {
        let theme = crate::ui::ui_theme(cx);
        let active = self.active_commands();
        let focus = self
            .floating
            .focus
            .as_ref()
            .filter(|focus| focus.toolbar == toolbar.id)
            .map(|focus| focus.stop);
        let state = BarState {
            status: None,
            sync: None,
            active: &active,
            focus,
            can_run: &|_| true,
            menus: &self.floating.menus,
            frame: BarFrame::Floating,
        };
        let items = bar_items(toolbar, &state, &mut |_| None, None, cx);
        let selector = format!("floating-toolbar-{}", toolbar.id);
        bar(BarFrame::Floating, toolbar.density, &theme)
            .id(gpui::ElementId::Name(selector.clone().into()))
            .selector(move || selector)
            .occlude()
            .children(items)
            .into_any_element()
    }

    /// What a floating bar is placed by: the selection's rows on screen,
    /// or the caret's row. `None` when that text is off screen.
    fn floating_anchor(&self, place: Place, frame: &FrameLayout) -> Option<FloatAnchor> {
        match place {
            Place::Selection => {
                let rects = frame.range_rects(&self.selected_range(), &self.theme);
                let rows =
                    visible_selection_rows(rects, frame.bounds, self.theme.newline_selection_width);
                (!rows.is_empty()).then_some(FloatAnchor::Selection(rows))
            }
            Place::CursorLine => {
                let caret = frame.caret_bounds(self.cursor(), &self.theme)?;
                let on_screen =
                    caret.bottom() > frame.bounds.top() && caret.top() < frame.bounds.bottom();
                on_screen.then_some(FloatAnchor::CursorLine {
                    caret,
                    text_right: frame.text_right(),
                })
            }
            _ => None,
        }
    }
}

/// The text a floating bar is placed by.
#[derive(Clone, Debug, PartialEq)]
enum FloatAnchor {
    /// The selection's rows that are on screen, top to bottom.
    Selection(Vec<Bounds<Pixels>>),
    /// The caret, and where the text column ends.
    CursorLine {
        caret: Bounds<Pixels>,
        text_right: Pixels,
    },
}

impl FloatAnchor {
    fn origin(&self, bar: Size<Pixels>, pane: Bounds<Pixels>, gap: Pixels) -> Point<Pixels> {
        match self {
            FloatAnchor::Selection(rows) => selection_bar_origin(rows, pane, bar, gap),
            FloatAnchor::CursorLine { caret, text_right } => {
                cursor_line_bar_origin(*caret, *text_right, pane, bar, gap)
            }
        }
    }
}

/// The selection's rows the bar is placed by: those at least partly in
/// the pane, without the slivers for line breaks and blank lines the
/// selection starts with, so a selection begun at a line's end sits by
/// the text it covers.
fn visible_selection_rows(
    mut rects: Vec<Bounds<Pixels>>,
    pane: Bounds<Pixels>,
    newline_width: Pixels,
) -> Vec<Bounds<Pixels>> {
    let leading_breaks = rects
        .iter()
        .take_while(|row| row.size.width <= newline_width)
        .count();
    rects.drain(..leading_breaks.min(rects.len().saturating_sub(1)));
    rects.retain(|row| row.bottom() > pane.top() && row.top() < pane.bottom());
    rects
}

/// The top-left corner of the bar by a selection: a gap above its first
/// row on screen, at the row's left; below its last row when there's no
/// room above; and, when the selection fills the pane, just inside the
/// pane's top. It never leaves the pane.
fn selection_bar_origin(
    rows: &[Bounds<Pixels>],
    pane: Bounds<Pixels>,
    bar: Size<Pixels>,
    gap: Pixels,
) -> Point<Pixels> {
    let (Some(first), Some(last)) = (rows.first(), rows.last()) else {
        return pane.origin;
    };
    let above = first.top() - gap - bar.height;
    let below = last.bottom() + gap;
    let (left, top) = if above >= pane.top() {
        (first.left(), above)
    } else if below + bar.height <= pane.bottom() {
        (last.left(), below)
    } else {
        (first.left(), pane.top() + gap)
    };
    point(clamp_left(left, pane, bar.width, gap), top)
}

/// The top-left corner of the bar by the cursor's line: past the text
/// column's end, level with the caret. When the pane is too narrow for
/// that, it moves over the line, above it or below it near the pane's
/// top, so it never covers the caret's row.
fn cursor_line_bar_origin(
    caret: Bounds<Pixels>,
    text_right: Pixels,
    pane: Bounds<Pixels>,
    bar: Size<Pixels>,
    gap: Pixels,
) -> Point<Pixels> {
    let beside = text_right + gap;
    if beside + bar.width + gap <= pane.right() {
        let top = caret.center().y - bar.height / 2.;
        let top = top.clamp(pane.top(), (pane.bottom() - bar.height).max(pane.top()));
        return point(beside, top);
    }
    let left = clamp_left(text_right - bar.width, pane, bar.width, gap);
    let above = caret.top() - gap - bar.height;
    if above >= pane.top() {
        return point(left, above);
    }
    point(left, caret.bottom() + gap)
}

/// Keeps a bar's left edge where all of it fits in the pane, a gap in
/// from each side when there's room for that.
fn clamp_left(left: Pixels, pane: Bounds<Pixels>, width: Pixels, gap: Pixels) -> Pixels {
    let min = pane.left() + gap;
    let max = pane.right() - gap - width;
    if max < min {
        return pane.left().max(pane.right() - width);
    }
    left.clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{px, size};

    fn pane() -> Bounds<Pixels> {
        Bounds::new(point(px(0.), px(80.)), size(px(800.), px(600.)))
    }

    fn row(left: f32, top: f32, width: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(left), px(top)), size(px(width), px(20.)))
    }

    fn bar_size() -> Size<Pixels> {
        size(px(150.), px(34.))
    }

    #[test]
    fn the_selection_bar_sits_a_gap_above_the_first_row() {
        let rows = [row(40., 300., 500.), row(20., 320., 200.)];
        let at = selection_bar_origin(&rows, pane(), bar_size(), px(8.));
        assert_eq!(at, point(px(40.), px(258.)));
        assert!(at.y + bar_size().height + px(8.) <= rows[0].top());
    }

    #[test]
    fn the_selection_bar_flips_below_the_last_row_near_the_pane_top() {
        let rows = [row(40., 90., 500.), row(20., 110., 200.)];
        let at = selection_bar_origin(&rows, pane(), bar_size(), px(8.));
        assert_eq!(at, point(px(20.), px(138.)));
    }

    #[test]
    fn a_selection_filling_the_pane_keeps_its_bar_inside_the_pane() {
        let rows: Vec<_> = (0..30)
            .map(|index| row(20., 82. + index as f32 * 20., 700.))
            .collect();
        let at = selection_bar_origin(&rows, pane(), bar_size(), px(8.));
        assert!(at.y >= pane().top() && at.y + bar_size().height <= pane().bottom());
    }

    #[test]
    fn the_selection_bar_stays_inside_the_pane_at_its_right_edge() {
        let rows = [row(760., 300., 30.)];
        let at = selection_bar_origin(&rows, pane(), bar_size(), px(8.));
        assert_eq!(at.x, px(800. - 8. - 150.));
    }

    #[test]
    fn a_selection_begun_at_a_line_end_is_placed_by_the_text_it_covers() {
        let rects = vec![
            row(600., 260., 6.),
            row(20., 280., 6.),
            row(20., 300., 200.),
        ];
        let rows = visible_selection_rows(rects, pane(), px(6.));
        assert_eq!(rows, vec![row(20., 300., 200.)]);
        let only_breaks = vec![row(600., 260., 6.), row(20., 280., 6.)];
        let rows = visible_selection_rows(only_breaks, pane(), px(6.));
        assert_eq!(rows, vec![row(20., 280., 6.)]);
    }

    #[test]
    fn rows_scrolled_out_of_the_pane_do_not_place_the_bar() {
        let rects = vec![
            row(20., 20., 200.),
            row(20., 40., 200.),
            row(20., 300., 200.),
        ];
        let rows = visible_selection_rows(rects, pane(), px(6.));
        assert_eq!(rows, vec![row(20., 300., 200.)]);
        let at = selection_bar_origin(&rows, pane(), bar_size(), px(8.));
        assert_eq!(at.y, px(258.));
    }

    #[test]
    fn the_cursor_line_bar_sits_past_the_text_or_above_the_line_when_narrow() {
        let caret = Bounds::new(point(px(100.), px(300.)), size(px(2.), px(20.)));
        let at = cursor_line_bar_origin(caret, px(500.), pane(), bar_size(), px(8.));
        assert_eq!(at, point(px(508.), px(293.)));
        let at = cursor_line_bar_origin(caret, px(700.), pane(), bar_size(), px(8.));
        assert_eq!(at, point(px(550.), px(258.)));
    }
}
