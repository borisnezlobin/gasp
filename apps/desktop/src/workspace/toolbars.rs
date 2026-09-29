//! The toolbars the workspace draws, docked in the status bar, down the
//! window's sides and on the note's card in the active pane (a strip at
//! its top or bottom, or a pill floating over the note), and the
//! keyboard's way through every toolbar: `toolbar.focus` moves into them
//! (again, to the next one), the arrows move between buttons, Enter or
//! Space presses, Tab goes to the next bar and Escape goes back to the
//! note.

use std::collections::HashMap;

use gasp_config::toolbars::{Behaviour, Place, Toolbar, ToolbarConditions, ToolbarItem};
use gpui::{
    AnyElement, App, Context, Div, KeyDownEvent, MouseButton, MouseDownEvent, SharedString, Task,
    Window, div, prelude::*,
};

use super::Workspace;
use super::pane::CardBars;
use crate::icons::IconName;
use crate::keymap::RunCommand;
use crate::toolbar::fitted::{FittedItems, OverflowCell};
use crate::toolbar::render::{
    BarFrame, BarState, WidgetWidths, add_button, bar, bar_items, bar_thickness, fitted_bar_items,
};
use crate::toolbar::{
    AddToToolbar, FocusStop, OpenToolbarOverflow, PressToolbarItem, ToolbarFocus, fitted_stops,
    focus_stops, item_key, more_key, overflow_items, step_stop,
};
use crate::ui::Selectable;
use crate::ui::{DrawnArea, MenuAnchor, MenuItem};

/// The docked places, in the order the keyboard visits them.
const DOCKED: [Place; 5] = [
    Place::EditorTop,
    Place::WindowLeft,
    Place::WindowRight,
    Place::EditorBottom,
    Place::StatusBar,
];

/// The edges of the note's card a bar can float over.
const CARD_EDGES: [Place; 4] = [
    Place::EditorTop,
    Place::EditorBottom,
    Place::WindowLeft,
    Place::WindowRight,
];

/// Bars shown on hover: which edges have one showing, and the timers
/// that show or hide one after the pointer rests.
#[derive(Default)]
pub(crate) struct ToolbarHover {
    revealed: Vec<Place>,
    reveal: Option<(Place, Task<()>)>,
    hide: HashMap<Place, Task<()>>,
    /// Where each place's shown bars were last drawn.
    areas: HashMap<(Layer, Place), DrawnArea>,
    /// Where the pointer was last seen, in the window or leaving it.
    pointer: Option<gpui::Point<gpui::Pixels>>,
    /// Whether the pointer is on the status bar, which shows its add button.
    on_status_bar: bool,
    /// The status widgets' widest widths, so they don't shuffle.
    status_widths: WidgetWidths,
    /// Each fitted bar's first item that went into its More button.
    overflow: HashMap<String, OverflowCell>,
}

/// A place's bars sorted by how they sit.
#[derive(Default)]
struct PlaceBars {
    /// In a strip of their own.
    steady: Vec<Toolbar>,
    /// Shown on hover, floating over the strip's edge.
    hovering: Vec<Toolbar>,
    /// Floating over the note as pills.
    over_note: Vec<Toolbar>,
}

/// Where a bar shown on hover or over the note floats: over the window's
/// edge, or over the note's card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Layer {
    Window,
    Card,
}

/// A place's docked bars: the strip they take in the layout, and the
/// ones shown on hover, which float over the edge instead.
#[derive(Default)]
pub(crate) struct DockedBars {
    pub strip: Option<AnyElement>,
    pub overlay: Option<AnyElement>,
    pub edge: Option<AnyElement>,
}

impl Workspace {
    /// Whether the keyboard is in a toolbar.
    pub fn toolbar_focus(&self) -> Option<&ToolbarFocus> {
        self.toolbar_focus.as_ref()
    }

    /// What decides whether a bar shows now, from the note in the active
    /// pane, for `toolbar` at `place`.
    fn conditions_for(&self, toolbar: &Toolbar, cx: &gpui::App) -> ToolbarConditions {
        let base = self
            .active_editor(cx)
            .map(|editor| editor.read(cx).toolbar_conditions())
            .unwrap_or_default();
        ToolbarConditions {
            hovered: self.toolbar_hover.revealed.contains(&toolbar.place),
            focused: self.focus_on(&toolbar.id).is_some(),
            ..base
        }
    }

    /// The ids of the docked toolbars that show now.
    pub fn shown_toolbars(&self, cx: &gpui::App) -> Vec<String> {
        DOCKED
            .iter()
            .flat_map(|place| self.config.toolbars.at(*place))
            .filter(|toolbar| toolbar.is_shown(&self.conditions_for(toolbar, cx)))
            .map(|toolbar| toolbar.id.clone())
            .collect()
    }

    fn focus_on(&self, toolbar: &str) -> Option<FocusStop> {
        self.toolbar_focus
            .as_ref()
            .filter(|focus| focus.toolbar == toolbar)
            .map(|focus| focus.stop)
    }

    fn bars_at(&self, place: Place) -> PlaceBars {
        let mut bars = PlaceBars::default();
        for toolbar in self.config.toolbars.at(place).cloned() {
            let group = match (toolbar.floats_over_note(), toolbar.behaviour) {
                (true, _) => &mut bars.over_note,
                (false, Behaviour::OnHover) => &mut bars.hovering,
                (false, _) => &mut bars.steady,
            };
            group.push(toolbar);
        }
        bars
    }

    /// The bars along the window's edge at `place` (the status bar and
    /// its sides), drawn for this frame.
    pub(crate) fn docked_bars(
        &mut self,
        place: Place,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> DockedBars {
        let PlaceBars {
            steady, hovering, ..
        } = self.bars_at(place);
        let strip = (!steady.is_empty()).then(|| self.strip(place, &steady, window, cx));
        let overlay = self.floating_bars(place, Layer::Window, &hovering, cx);
        let edge = (!hovering.is_empty()).then(|| self.hover_edge(place, Layer::Window, cx));
        DockedBars {
            strip,
            overlay,
            edge,
        }
    }

    /// The bars a pane's card holds this frame. The active pane's card has
    /// the bars at the top and bottom of the notes and the ones floating
    /// over the note; the others keep the strips' room, empty, so nothing
    /// moves when another pane becomes the active one.
    pub(crate) fn card_bars(
        &mut self,
        active: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> CardBars {
        let top = self.card_strip(Place::EditorTop, active, window, cx);
        let bottom = self.card_strip(Place::EditorBottom, active, window, cx);
        let over_note = if active {
            CARD_EDGES
                .into_iter()
                .flat_map(|place| self.card_layers(place, cx))
                .collect()
        } else {
            Vec::new()
        };
        CardBars {
            top,
            bottom,
            over_note,
        }
    }

    fn card_strip(
        &mut self,
        place: Place,
        active: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let steady = self.bars_at(place).steady;
        if steady.is_empty() {
            return None;
        }
        if active {
            return Some(self.strip(place, &steady, window, cx));
        }
        let ui = crate::ui::ui_theme(cx);
        let frame = Self::frame_for(place);
        let depth = steady
            .iter()
            .map(|toolbar| bar_thickness(frame, toolbar.density, &ui))
            .fold(gpui::px(0.), gpui::Pixels::max);
        Some(div().flex_none().w_full().h(depth).into_any_element())
    }

    /// The bars floating over the card's edge at `place`, and the strip
    /// that reveals the ones shown on hover.
    fn card_layers(&mut self, place: Place, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let PlaceBars {
            hovering,
            over_note,
            ..
        } = self.bars_at(place);
        let mut floating = over_note;
        if !place.is_vertical() {
            floating.extend(hovering);
        }
        let reveals = floating
            .iter()
            .any(|toolbar| toolbar.behaviour == Behaviour::OnHover);
        let edge = reveals.then(|| self.hover_edge(place, Layer::Card, cx));
        let bars = self.floating_bars(place, Layer::Card, &floating, cx);
        edge.into_iter().chain(bars).collect()
    }

    fn frame_for(place: Place) -> BarFrame {
        match place {
            Place::StatusBar => BarFrame::StatusBar,
            Place::WindowLeft | Place::WindowRight => BarFrame::Column,
            _ => BarFrame::Row,
        }
    }

    fn floating_frame_for(place: Place) -> BarFrame {
        match place {
            Place::WindowLeft | Place::WindowRight => BarFrame::FloatingColumn,
            _ => BarFrame::Floating,
        }
    }

    /// The strip a place's steady bars take. A bar that's hidden keeps its
    /// room, so the notes don't jump as it comes and goes.
    fn strip(
        &mut self,
        place: Place,
        toolbars: &[Toolbar],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let frame = Self::frame_for(place);
        if place == Place::StatusBar {
            return self.status_bar(toolbars, window, cx);
        }
        let ui = crate::ui::ui_theme(cx);
        let bars: Vec<AnyElement> = toolbars
            .iter()
            .map(|toolbar| {
                let items = self.fitted_if_shown(toolbar, frame, cx);
                self.with_context_menu(
                    bar(frame, toolbar.density, &ui).children(items),
                    toolbar,
                    cx,
                )
            })
            .collect();
        let strip = div().flex().flex_none();
        let strip = if frame == BarFrame::Column {
            strip.flex_col().h_full()
        } else {
            strip.flex_row().w_full()
        };
        strip
            .id(gpui::ElementId::Name(
                format!("toolbar-strip-{place:?}").into(),
            ))
            .children(bars)
            .into_any_element()
    }

    /// The status bar: every bar placed there, in one row as tall as the
    /// status bar has always been, with the add button while the pointer
    /// is on it.
    fn status_bar(
        &mut self,
        toolbars: &[Toolbar],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = crate::ui::ui_theme(cx);
        let mut children = Vec::new();
        for toolbar in toolbars {
            let focused_add = self.focus_on(&toolbar.id) == Some(FocusStop::Add);
            let add = (self.toolbar_hover.on_status_bar || focused_add)
                .then(|| add_button(toolbar, focused_add, BarFrame::StatusBar, cx));
            children.extend(self.items_if_shown(toolbar, BarFrame::StatusBar, add, window, cx));
        }
        let first = toolbars.first().cloned();
        let bar = bar(
            BarFrame::StatusBar,
            gasp_config::toolbars::Density::Compact,
            &ui,
        )
        .id("status-bar")
        .selector(|| "status-bar".to_owned())
        .on_hover(cx.listener(|workspace, hovered: &bool, _, cx| {
            workspace.toolbar_hover.on_status_bar = *hovered;
            cx.notify();
        }))
        .children(children);
        match first {
            Some(toolbar) => self.with_context_menu(bar, &toolbar, cx),
            None => bar.into_any_element(),
        }
    }

    /// A bar's items when it shows now, else none.
    fn items_if_shown(
        &mut self,
        toolbar: &Toolbar,
        frame: BarFrame,
        add: Option<AnyElement>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if !toolbar.is_shown(&self.conditions_for(toolbar, cx)) {
            return add.into_iter().collect();
        }
        self.render_items(toolbar, frame, add, window, cx)
    }

    fn render_items(
        &mut self,
        toolbar: &Toolbar,
        frame: BarFrame,
        add: Option<AnyElement>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        self.draw_items(toolbar, frame, cx, |state, attached, cx| {
            bar_items(toolbar, state, attached, add, cx)
        })
    }

    /// A bar's items fitted to its length when it shows now, else none.
    fn fitted_if_shown(
        &mut self,
        toolbar: &Toolbar,
        frame: BarFrame,
        cx: &mut Context<Self>,
    ) -> Option<FittedItems> {
        let overflow = self
            .toolbar_hover
            .overflow
            .entry(toolbar.id.clone())
            .or_default()
            .clone();
        if !toolbar.is_shown(&self.conditions_for(toolbar, cx)) {
            overflow.set(None);
            return None;
        }
        Some(self.draw_items(toolbar, frame, cx, |state, attached, cx| {
            fitted_bar_items(toolbar, state, attached, overflow, cx)
        }))
    }

    /// The first of `toolbar`'s items that went into its More button, as
    /// it was last drawn.
    fn first_hidden(&self, toolbar: &str) -> Option<usize> {
        self.toolbar_hover
            .overflow
            .get(toolbar)
            .and_then(|overflow| overflow.get())
    }

    /// Draws `toolbar`'s items with `draw`, from what they show now.
    fn draw_items<R>(
        &mut self,
        toolbar: &Toolbar,
        frame: BarFrame,
        cx: &mut Context<Self>,
        draw: impl FnOnce(&BarState<'_>, &mut dyn FnMut(&str) -> Option<AnyElement>, &mut App) -> R,
    ) -> R {
        let active = self
            .active_editor(cx)
            .map(|editor| editor.read(cx).active_commands())
            .unwrap_or_default();
        let has_note = self.active_editor(cx).is_some();
        let can_run = |id: &str| {
            let needs_note = crate::commands::handles(id);
            self.can_run(id) && (has_note || !needs_note)
        };
        let menus = self.config.toolbars.menus.clone();
        let note = self.active_editor(cx).map(|editor| editor.entity_id());
        let selecting = self
            .status
            .as_ref()
            .is_some_and(|status| status.for_selection);
        self.toolbar_hover.status_widths.describe(note, selecting);
        let menu = &self.menu;
        let gap = crate::ui::ui_theme(cx).space_xs;
        let mut attached = |key: &str| menu.render_attached(key, gap);
        let state = BarState {
            status: self.status.as_ref(),
            widths: Some(&self.toolbar_hover.status_widths),
            sync: self.sync_indicator.clone(),
            active: &active,
            focus: self.focus_on(&toolbar.id),
            can_run: &can_run,
            menus: &menus,
            frame,
        };
        draw(&state, &mut attached, cx)
    }

    /// A right-click on a bar offers to add to it, customize toolbars or
    /// hide it.
    fn with_context_menu(
        &self,
        bar: impl InteractiveElement + IntoElement + 'static,
        toolbar: &Toolbar,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = toolbar.id.clone();
        bar.on_mouse_down(
            MouseButton::Right,
            cx.listener(move |workspace, event: &MouseDownEvent, window, cx| {
                workspace.open_bar_menu(&id, event.position, window, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn open_bar_menu(
        &mut self,
        id: &str,
        at: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(toolbar) = self.config.toolbars.get(id).cloned() else {
            return;
        };
        let add = add_action(&toolbar.id);
        let hide_id = toolbar.id.clone();
        let workspace = cx.entity().downgrade();
        let items = vec![
            MenuItem::action(
                format!("Add to {}…", toolbar.title.to_lowercase()),
                move |window, cx| window.dispatch_action(Box::new(add.clone()), cx),
            )
            .with_icon(IconName::Plus),
            MenuItem::command("toolbar.customize", cx).with_icon(IconName::SlidersHorizontal),
            MenuItem::Separator,
            MenuItem::action(
                format!("Hide {}", toolbar.title.to_lowercase()),
                move |_, cx| {
                    workspace
                        .update(cx, |workspace, cx| workspace.hide_toolbar(&hide_id, cx))
                        .ok();
                },
            )
            .with_icon(IconName::EyeSlash),
        ];
        self.menu.open(items, MenuAnchor::Pointer(at), window, cx);
    }

    /// Turns a toolbar off in the vault's `toolbars.toml`.
    fn hide_toolbar(&mut self, id: &str, cx: &mut Context<Self>) {
        match gasp_config::toolbar_files::remove_toolbar(&self.vault, id) {
            Ok(_) => self.reload_config(cx),
            Err(error) => {
                crate::notices::problem(format!("Couldn’t hide the toolbar: {error}"), cx);
            }
        }
    }

    // ---- Shown on hover ----

    /// The strip along `place`'s edge that shows its hover bars.
    fn hover_edge(&self, place: Place, layer: Layer, cx: &mut Context<Self>) -> AnyElement {
        let ui = crate::ui::ui_theme(cx);
        let edge = edge_at(div(), place, ui.toolbar.hover_edge).id(gpui::ElementId::Name(
            format!("toolbar-edge-{layer:?}-{place:?}").into(),
        ));
        edge.on_hover(cx.listener(move |workspace, hovered: &bool, window, cx| {
            if *hovered {
                workspace.start_reveal(place, window, cx);
            } else {
                workspace.toolbar_hover.reveal = None;
            }
        }))
        .into_any_element()
    }

    fn start_reveal(&mut self, place: Place, window: &mut Window, cx: &mut Context<Self>) {
        let delay = self.config.toolbars.timing.hover_delay;
        let task = cx.spawn_in(window, async move |workspace, cx| {
            cx.background_executor().timer(delay).await;
            workspace
                .update(cx, |workspace, cx| workspace.reveal(place, cx))
                .ok();
        });
        self.toolbar_hover.reveal = Some((place, task));
    }

    fn reveal(&mut self, place: Place, cx: &mut Context<Self>) {
        self.toolbar_hover.reveal = None;
        self.toolbar_hover.hide.remove(&place);
        if !self.toolbar_hover.revealed.contains(&place) {
            self.toolbar_hover.revealed.push(place);
        }
        cx.notify();
    }

    /// The pointer is at `pointer`: each bar shown on hover stays while
    /// it's in use and otherwise starts its hide delay, once.
    pub(super) fn follow_pointer_for_bars(
        &mut self,
        pointer: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toolbar_hover.pointer = Some(pointer);
        for place in self.toolbar_hover.revealed.clone() {
            if self.bar_in_use(place) {
                self.toolbar_hover.hide.remove(&place);
            } else if !self.toolbar_hover.hide.contains_key(&place) {
                self.start_hide(place, window, cx);
            }
        }
    }

    /// Whether the bars shown on hover at `place` are in use: the pointer
    /// is over them (by where they were drawn, whatever is drawn on top),
    /// a menu is open, or the keyboard is in one of them.
    fn bar_in_use(&self, place: Place) -> bool {
        let pointer = self.toolbar_hover.pointer.is_some_and(|pointer| {
            self.toolbar_hover
                .areas
                .iter()
                .any(|((_, at), area)| *at == place && area.contains(pointer))
        });
        let keyboard = self.toolbar_focus.as_ref().is_some_and(|focus| {
            self.config
                .toolbars
                .get(&focus.toolbar)
                .is_some_and(|toolbar| toolbar.place == place)
        });
        pointer || keyboard || self.menu.is_open()
    }

    fn start_hide(&mut self, place: Place, window: &mut Window, cx: &mut Context<Self>) {
        let delay = self.config.toolbars.timing.hide_delay;
        let task = cx.spawn_in(window, async move |workspace, cx| {
            cx.background_executor().timer(delay).await;
            workspace
                .update(cx, |workspace, cx| {
                    workspace.toolbar_hover.hide.remove(&place);
                    if workspace.bar_in_use(place) {
                        return;
                    }
                    workspace
                        .toolbar_hover
                        .revealed
                        .retain(|shown| *shown != place);
                    cx.notify();
                })
                .ok();
        });
        self.toolbar_hover.hide.insert(place, task);
    }

    /// Bars floating over `place`'s edge while they show, as pills
    /// centred along it and inset by the theme's spacing. They take no
    /// room, and each fits its items to the room the edge has.
    fn floating_bars(
        &mut self,
        place: Place,
        layer: Layer,
        toolbars: &[Toolbar],
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let frame = Self::floating_frame_for(place);
        let ui = crate::ui::ui_theme(cx);
        let bars: Vec<AnyElement> = toolbars
            .iter()
            .filter_map(|toolbar| {
                let items = self.fitted_if_shown(toolbar, frame, cx)?;
                let pill = bar(frame, toolbar.density, &ui)
                    .occlude()
                    .child(items.hug());
                Some(self.with_context_menu(pill, toolbar, cx))
            })
            .collect();
        let area = self
            .toolbar_hover
            .areas
            .entry((layer, place))
            .or_default()
            .clone();
        if bars.is_empty() {
            area.set(None);
            return None;
        }
        let overlay = edge_at(div().flex().gap(ui.space_md), place, gpui::px(0.))
            .map(|overlay| {
                if frame.is_column() {
                    overlay.flex_col()
                } else {
                    overlay.flex_row()
                }
            })
            .justify_center()
            .items_center()
            .p(ui.space_md)
            .id(gpui::ElementId::Name(
                format!("toolbar-overlay-{layer:?}-{place:?}").into(),
            ))
            .child(area.probe().size_full())
            .children(bars);
        Some(overlay.into_any_element())
    }

    // ---- Keyboard ----

    /// The bars the keyboard can move into, in order, with where it can
    /// stop in each: those floating over the note first, since they're
    /// about the selection or the cursor's line, then the docked ones.
    fn focusable_toolbars(&self, cx: &gpui::App) -> Vec<(String, Vec<FocusStop>)> {
        let docked = DOCKED
            .iter()
            .flat_map(|place| self.config.toolbars.at(*place))
            .map(|toolbar| {
                let stops = focus_stops(toolbar, has_add(toolbar));
                let first_hidden = self.first_hidden(&toolbar.id);
                (toolbar.id.clone(), fitted_stops(stops, first_hidden))
            });
        let floating: Vec<(String, Vec<FocusStop>)> = self
            .active_editor(cx)
            .map(|editor| {
                editor
                    .read(cx)
                    .shown_floating_toolbars()
                    .into_iter()
                    .map(|toolbar| (toolbar.id.clone(), focus_stops(toolbar, false)))
                    .collect()
            })
            .unwrap_or_default();
        floating
            .into_iter()
            .chain(docked)
            .filter(|(_, stops)| !stops.is_empty())
            .collect()
    }

    /// `toolbar.focus`: into the first bar, or on to the next one.
    pub fn focus_toolbars(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.step_toolbar(1, window, cx);
    }

    fn step_toolbar(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        let bars = self.focusable_toolbars(cx);
        if bars.is_empty() {
            return;
        }
        let current = self
            .toolbar_focus
            .as_ref()
            .and_then(|focus| bars.iter().position(|(id, _)| *id == focus.toolbar));
        let next = match current {
            Some(at) => (at as isize + step).rem_euclid(bars.len() as isize) as usize,
            None => 0,
        };
        let (toolbar, stops) = &bars[next];
        self.set_toolbar_focus(
            Some(ToolbarFocus {
                toolbar: toolbar.clone(),
                stop: stops[0],
            }),
            cx,
        );
        window.focus(&self.focus_handle);
        crate::ui::focus_visible::set_keyboard_driving(true, cx);
    }

    /// Moves the keyboard in or out of the bars, telling the note in the
    /// active pane, which draws the floating ones.
    fn set_toolbar_focus(&mut self, focus: Option<ToolbarFocus>, cx: &mut Context<Self>) {
        self.toolbar_focus = focus.clone();
        if let Some(editor) = self.active_editor(cx) {
            editor.update(cx, |editor, cx| editor.set_toolbar_focus(focus, cx));
        }
        cx.notify();
    }

    /// Lets go of the bars when the keyboard has gone somewhere else, as
    /// after a click in a note.
    pub(crate) fn drop_stale_toolbar_focus(&mut self, window: &Window, cx: &mut Context<Self>) {
        let away = !self.focus_handle.is_focused(window) && !self.menu.is_open();
        if self.toolbar_focus.is_some() && away {
            self.set_toolbar_focus(None, cx);
        }
    }

    pub(crate) fn on_toolbar_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(focus) = self.toolbar_focus.clone() else {
            return;
        };
        let keystroke = &event.keystroke;
        let back = keystroke.modifiers.shift;
        let handled = match keystroke.key.as_str() {
            "left" | "up" => self.step_stop(&focus, -1, cx),
            "right" | "down" => self.step_stop(&focus, 1, cx),
            "home" => self.step_stop_to_end(&focus, false, cx),
            "end" => self.step_stop_to_end(&focus, true, cx),
            "tab" => {
                self.step_toolbar(if back { -1 } else { 1 }, window, cx);
                true
            }
            "enter" | "space" => {
                self.press_stop(&focus, window, cx);
                true
            }
            "escape" => {
                self.set_toolbar_focus(None, cx);
                self.focus_active(window, cx);
                true
            }
            _ => false,
        };
        if handled {
            cx.stop_propagation();
        }
    }

    fn stops_of(&self, toolbar: &str, cx: &gpui::App) -> Vec<FocusStop> {
        self.focusable_toolbars(cx)
            .into_iter()
            .find(|(id, _)| id == toolbar)
            .map(|(_, stops)| stops)
            .unwrap_or_default()
    }

    fn step_stop(&mut self, focus: &ToolbarFocus, step: isize, cx: &mut Context<Self>) -> bool {
        let stops = self.stops_of(&focus.toolbar, cx);
        if let Some(stop) = step_stop(&stops, focus.stop, step) {
            let toolbar = focus.toolbar.clone();
            self.set_toolbar_focus(Some(ToolbarFocus { toolbar, stop }), cx);
        }
        true
    }

    fn step_stop_to_end(
        &mut self,
        focus: &ToolbarFocus,
        last: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let stops = self.stops_of(&focus.toolbar, cx);
        let stop = if last { stops.last() } else { stops.first() };
        if let Some(stop) = stop.copied() {
            let toolbar = focus.toolbar.clone();
            self.set_toolbar_focus(Some(ToolbarFocus { toolbar, stop }), cx);
        }
        true
    }

    fn press_stop(&mut self, focus: &ToolbarFocus, window: &mut Window, cx: &mut Context<Self>) {
        match focus.stop {
            FocusStop::Add => self.add_to_toolbar(&focus.toolbar, window, cx),
            FocusStop::More => self.open_overflow(&focus.toolbar, window, cx),
            FocusStop::Item(index) => {
                let press = PressToolbarItem {
                    toolbar: focus.toolbar.clone().into(),
                    index,
                    by_pointer: false,
                };
                self.on_press_toolbar_item(&press, window, cx);
            }
        }
    }

    // ---- Pressing ----

    /// Finds a toolbar by id, docked in the vault's config or floating in
    /// the active note.
    fn toolbar_by_id(&self, id: &str) -> Option<Toolbar> {
        self.config.toolbars.get(id).cloned()
    }

    pub(crate) fn on_press_toolbar_item(
        &mut self,
        press: &PressToolbarItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(toolbar) = self.toolbar_by_id(&press.toolbar) else {
            return;
        };
        match toolbar.items.get(press.index) {
            Some(ToolbarItem::Command(id)) => self.press_command(id, window, cx),
            Some(ToolbarItem::Menu(menu)) => {
                self.open_toolbar_menu(&toolbar, press.index, menu, press.by_pointer, window, cx)
            }
            _ => {}
        }
    }

    /// Runs a toolbar's command as its key would: in the note when it's a
    /// note's command, the keyboard going back there first.
    pub(crate) fn press_command(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let from_keyboard = self.toolbar_focus.is_some();
        self.set_toolbar_focus(None, cx);
        let note_focused = self
            .active_editor(cx)
            .is_some_and(|editor| editor.read(cx).focus_handle.is_focused(window));
        if from_keyboard || (crate::commands::handles(id) && !note_focused) {
            self.focus_active(window, cx);
        }
        let action = RunCommand {
            id: id.to_owned().into(),
        };
        cx.defer_in(window, move |_, window, cx| {
            window.dispatch_action(Box::new(action), cx);
        });
    }

    fn open_toolbar_menu(
        &mut self,
        toolbar: &Toolbar,
        index: usize,
        menu_id: &str,
        by_pointer: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(items) = self.menu_commands(menu_id, cx) else {
            return;
        };
        let anchor = self.menu_anchor(toolbar, index, by_pointer, window, cx);
        self.menu.open(items, anchor, window, cx);
    }

    /// The commands of `[menu.<menu_id>]`, each pressed as a bar's
    /// button would be.
    fn menu_commands(&self, menu_id: &str, cx: &mut Context<Self>) -> Option<Vec<MenuItem>> {
        let menu = self.config.toolbars.menu(menu_id)?;
        Some(
            menu.items
                .iter()
                .map(|id| self.command_menu_item(id, cx))
                .collect(),
        )
    }

    fn command_menu_item(&self, id: &str, cx: &mut Context<Self>) -> MenuItem {
        let command = id.to_owned();
        let workspace = cx.entity().downgrade();
        MenuItem::command(id, cx)
            .with_icon(IconName::for_command(id))
            .disabled(!self.can_run(id))
            .with_handler(move |window, cx| {
                workspace
                    .update(cx, |workspace, cx| {
                        workspace.press_command(&command, window, cx)
                    })
                    .ok();
            })
    }

    pub(crate) fn on_open_toolbar_overflow(
        &mut self,
        action: &OpenToolbarOverflow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_overflow(&action.toolbar, window, cx);
    }

    /// Opens the menu of `id`'s items that didn't fit on it, under (or
    /// over) its More button.
    fn open_overflow(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(toolbar), Some(first_hidden)) = (self.toolbar_by_id(id), self.first_hidden(id))
        else {
            return;
        };
        let items: Vec<MenuItem> = overflow_items(&toolbar.items[first_hidden..])
            .iter()
            .filter_map(|item| self.overflow_entry(item, cx))
            .collect();
        let anchor = more_anchor(toolbar.place, more_key(id).into());
        self.menu.open(items, anchor, window, cx);
    }

    fn overflow_entry(&self, item: &ToolbarItem, cx: &mut Context<Self>) -> Option<MenuItem> {
        match item {
            ToolbarItem::Command(id) => Some(self.command_menu_item(id, cx)),
            ToolbarItem::Menu(id) => {
                let menu = self.config.toolbars.menu(id)?;
                let icon = IconName::from_name(&menu.icon).unwrap_or(IconName::DotsThree);
                let title = menu.title.clone();
                let commands = self.menu_commands(id, cx)?;
                Some(MenuItem::submenu(title, commands).with_icon(icon))
            }
            ToolbarItem::Separator => Some(MenuItem::Separator),
            _ => None,
        }
    }

    /// Under a docked bar's button, over one in the status bar, and at the
    /// pointer, or the bar's corner, for a floating bar.
    fn menu_anchor(
        &self,
        toolbar: &Toolbar,
        index: usize,
        by_pointer: bool,
        window: &Window,
        cx: &gpui::App,
    ) -> MenuAnchor {
        let key = item_key(&toolbar.id, index).into();
        if !toolbar.place.is_floating() {
            return docked_anchor(toolbar.place, key);
        }
        let corner = self
            .active_editor(cx)
            .and_then(|editor| editor.read(cx).floating_bar_origin(&toolbar.id));
        match (by_pointer, corner) {
            (false, Some(corner)) => MenuAnchor::Pointer(corner),
            _ => MenuAnchor::Pointer(window.mouse_position()),
        }
    }

    /// Opens the Toolbars settings page ready to add to toolbar `id`,
    /// through `toolbar.customize`, which reads which toolbar from
    /// [`Workspace::take_toolbar_to_add_to`].
    pub(crate) fn add_to_toolbar(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.set_toolbar_focus(None, cx);
        self.toolbar_to_add_to = Some(id.to_owned());
        self.run_command("toolbar.customize", window, cx);
    }

    /// The toolbar the settings page should open ready to add to, once.
    pub fn take_toolbar_to_add_to(&mut self) -> Option<String> {
        self.toolbar_to_add_to.take()
    }

    pub(crate) fn on_add_to_toolbar(
        &mut self,
        action: &AddToToolbar,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.add_to_toolbar(&action.toolbar, window, cx);
    }
}

/// Where a docked bar's menu hangs from the button called `key`: over it
/// at the bottom of the window or the notes, else under it, lined up with
/// its right edge down the window's right side.
fn docked_anchor(place: Place, key: SharedString) -> MenuAnchor {
    match place {
        Place::StatusBar | Place::EditorBottom => MenuAnchor::Above { key },
        _ => MenuAnchor::Below {
            key,
            align_right: place == Place::WindowRight,
        },
    }
}

/// Where the More menu hangs: a row's More button sits at its right end,
/// so the menu lines up with the button's right edge.
fn more_anchor(place: Place, key: SharedString) -> MenuAnchor {
    match docked_anchor(place, key) {
        MenuAnchor::Below { key, .. } if !place.is_vertical() => MenuAnchor::Below {
            key,
            align_right: true,
        },
        anchor => anchor,
    }
}

/// The action the context menu's first item dispatches.
fn add_action(id: &str) -> AddToToolbar {
    AddToToolbar {
        toolbar: id.to_owned().into(),
    }
}

/// Whether a docked bar has an add button the keyboard can reach: the
/// status bar's bars do.
fn has_add(toolbar: &Toolbar) -> bool {
    toolbar.place == Place::StatusBar
}

/// `element` laid along `place`'s edge of its parent, `thickness` deep
/// (or as deep as its content, for zero).
fn edge_at(element: Div, place: Place, thickness: gpui::Pixels) -> Div {
    let element = element.absolute();
    let sized = |element: Div, vertical: bool| match (thickness > gpui::px(0.), vertical) {
        (false, _) => element,
        (true, true) => element.w(thickness),
        (true, false) => element.h(thickness),
    };
    match place {
        Place::WindowLeft => sized(element.left_0().top_0().bottom_0(), true),
        Place::WindowRight => sized(element.right_0().top_0().bottom_0(), true),
        Place::EditorTop => sized(element.left_0().right_0().top_0(), false),
        _ => sized(element.left_0().right_0().bottom_0(), false),
    }
}
