//! Bars that float over the note: by the selection, just above it, or at
//! the end of the cursor's line. The editor draws them, since it knows
//! where its text is on screen, and keeps what else they need: whether
//! typing has paused, which toggles are on at the cursor, and whether the
//! keyboard is in one of them.

use std::time::Duration;

use gasp_config::toolbars::{
    Place, Toolbar, ToolbarConditions, ToolbarContext, ToolbarMenu, Toolbars,
};
use gpui::{AnyElement, Bounds, Context, Corner, Pixels, Point, Task, anchored, point, prelude::*};

use super::render::{BarFrame, BarState, bar, bar_items};
use super::{ToolbarFocus, toolbar_context};
use crate::editor::EditorView;
use crate::frame::FrameLayout;

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

    /// The floating bars to draw this frame, placed by the text in `frame`.
    /// None show unless the editor has the keyboard or one of them does.
    pub(crate) fn floating_toolbars(
        &mut self,
        frame: &FrameLayout,
        editor_focused: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        self.floating.placed.clear();
        let bar_focused = self.floating.focus.as_ref().is_some_and(|focus| {
            self.floating
                .toolbars
                .iter()
                .any(|toolbar| toolbar.id == focus.toolbar)
        });
        if self.read_only || !(editor_focused || bar_focused) {
            return Vec::new();
        }
        let conditions = self.toolbar_conditions();
        let shown: Vec<Toolbar> = self
            .floating
            .toolbars
            .iter()
            .filter(|toolbar| self.floating_bar_shown(toolbar, &conditions))
            .cloned()
            .collect();
        shown
            .iter()
            .filter_map(|toolbar| self.floating_bar(toolbar, frame, cx))
            .collect()
    }

    fn floating_bar(
        &mut self,
        toolbar: &Toolbar,
        frame: &FrameLayout,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let theme = crate::ui::ui_theme(cx);
        let height = theme.toolbar.button(toolbar.density) + theme.toolbar.padding * 2.;
        let gap = theme.toolbar.float_gap;
        let (corner, position) = self.floating_place(toolbar.place, frame, height, gap)?;
        let bottom_left = match corner {
            Corner::BottomLeft => position,
            _ => point(position.x, position.y + height),
        };
        self.floating.placed.push((toolbar.id.clone(), bottom_left));
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
        let body = bar(BarFrame::Floating, toolbar.density, &theme)
            .id(gpui::ElementId::Name(selector.clone().into()))
            .debug_selector(move || selector)
            .occlude()
            .children(items);
        Some(
            anchored()
                .anchor(corner)
                .position(position)
                .snap_to_window()
                .child(body)
                .into_any_element(),
        )
    }

    /// Where a floating bar goes: above the selection's first line, or
    /// below its last when there's no room above; or at the end of the
    /// cursor's line, level with it. `None` when that text is off screen.
    fn floating_place(
        &self,
        place: Place,
        frame: &FrameLayout,
        height: Pixels,
        gap: Pixels,
    ) -> Option<(Corner, Point<Pixels>)> {
        match place {
            Place::Selection => {
                let rects = frame.range_rects(&self.selected_range(), &self.theme);
                let (first, last) = (rects.first()?, rects.last()?);
                Some(selection_place(first, last, frame.bounds, height, gap))
            }
            Place::CursorLine => {
                let caret = frame.caret_bounds(self.cursor(), &self.theme)?;
                let top = caret.center().y - height / 2.;
                Some((Corner::TopLeft, point(frame.text_right() + gap, top)))
            }
            _ => None,
        }
    }
}

/// Above the selection's first row when the bar fits between it and the
/// note's top, else below its last row.
fn selection_place(
    first: &Bounds<Pixels>,
    last: &Bounds<Pixels>,
    note: Bounds<Pixels>,
    height: Pixels,
    gap: Pixels,
) -> (Corner, Point<Pixels>) {
    let above = first.top() - gap;
    if above - height >= note.top() {
        return (Corner::BottomLeft, point(first.left(), above));
    }
    (Corner::TopLeft, point(first.left(), last.bottom() + gap))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{px, size};

    #[test]
    fn the_selection_bar_goes_above_unless_the_note_top_is_in_the_way() {
        let note = Bounds::new(point(px(0.), px(0.)), size(px(800.), px(600.)));
        let row = |top: f32| Bounds::new(point(px(40.), px(top)), size(px(100.), px(20.)));
        let (corner, at) = selection_place(&row(200.), &row(240.), note, px(34.), px(8.));
        assert_eq!(corner, Corner::BottomLeft);
        assert_eq!(at, point(px(40.), px(192.)));
        let (corner, at) = selection_place(&row(10.), &row(50.), note, px(34.), px(8.));
        assert_eq!(corner, Corner::TopLeft);
        assert_eq!(at, point(px(40.), px(78.)));
    }
}
