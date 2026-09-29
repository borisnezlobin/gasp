//! The toolbars the workspace draws, docked in the status bar, above or
//! below the notes and down the window's sides, and the keyboard's way
//! through every toolbar: `toolbar.focus` moves into them (again, to the
//! next one), the arrows move between buttons, Enter or Space presses,
//! Tab goes to the next bar and Escape goes back to the note.

use gasp_config::toolbars::{Behaviour, Place, Toolbar, ToolbarConditions, ToolbarItem};
use gpui::{
    AnyElement, Context, Div, KeyDownEvent, MouseButton, MouseDownEvent, Task, Window, div,
    prelude::*,
};

use super::Workspace;
use crate::icons::IconName;
use crate::keymap::RunCommand;
use crate::toolbar::render::{BarFrame, BarState, add_button, bar, bar_items, floating_surface};
use crate::toolbar::{
    AddToToolbar, FocusStop, PressToolbarItem, ToolbarFocus, focus_stops, item_key, step_stop,
};
use crate::ui::{MenuAnchor, MenuItem};

/// The docked places, in the order the keyboard visits them.
const DOCKED: [Place; 5] = [
    Place::EditorTop,
    Place::WindowLeft,
    Place::WindowRight,
    Place::EditorBottom,
    Place::StatusBar,
];

/// Bars shown on hover: which edges have one showing, and the timers
/// that show or hide one after the pointer rests.
#[derive(Default)]
pub(crate) struct ToolbarHover {
    revealed: Vec<Place>,
    reveal: Option<(Place, Task<()>)>,
    hide: Option<(Place, Task<()>)>,
    /// Whether the pointer is on the status bar, which shows its add button.
    on_status_bar: bool,
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

    /// The docked bars at `place`, drawn for this frame.
    pub(crate) fn docked_bars(
        &mut self,
        place: Place,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> DockedBars {
        let toolbars: Vec<Toolbar> = self.config.toolbars.at(place).cloned().collect();
        let (hovering, steady): (Vec<Toolbar>, Vec<Toolbar>) = toolbars
            .into_iter()
            .partition(|toolbar| toolbar.behaviour == Behaviour::OnHover);
        let strip = (!steady.is_empty()).then(|| self.strip(place, &steady, window, cx));
        let overlay = self.hover_overlay(place, &hovering, window, cx);
        let edge = (!hovering.is_empty()).then(|| self.hover_edge(place, cx));
        DockedBars {
            strip,
            overlay,
            edge,
        }
    }

    fn frame_for(place: Place) -> BarFrame {
        match place {
            Place::StatusBar => BarFrame::StatusBar,
            Place::WindowLeft | Place::WindowRight => BarFrame::Column,
            _ => BarFrame::Row,
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
                let items = self.items_if_shown(toolbar, frame, None, window, cx);
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
        .debug_selector(|| "status-bar".to_owned())
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
        let menu = &self.menu;
        let gap = crate::ui::ui_theme(cx).space_xs;
        let mut attached = |key: &str| menu.render_attached(key, gap);
        let state = BarState {
            status: self.status.as_ref(),
            sync: self.sync_indicator.clone(),
            active: &active,
            focus: self.focus_on(&toolbar.id),
            can_run: &can_run,
            menus: &menus,
            frame,
        };
        bar_items(toolbar, &state, &mut attached, add, cx)
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
            Err(error) => eprintln!("could not hide the toolbar: {error}"),
        }
    }

    // ---- Shown on hover ----

    /// The strip along `place`'s edge that shows its hover bars.
    fn hover_edge(&self, place: Place, cx: &mut Context<Self>) -> AnyElement {
        let ui = crate::ui::ui_theme(cx);
        let edge = edge_at(div(), place, ui.toolbar.hover_edge).id(gpui::ElementId::Name(
            format!("toolbar-edge-{place:?}").into(),
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
        self.toolbar_hover.hide = None;
        if !self.toolbar_hover.revealed.contains(&place) {
            self.toolbar_hover.revealed.push(place);
        }
        cx.notify();
    }

    fn start_hide(&mut self, place: Place, window: &mut Window, cx: &mut Context<Self>) {
        let delay = self.config.toolbars.timing.hide_delay;
        let task = cx.spawn_in(window, async move |workspace, cx| {
            cx.background_executor().timer(delay).await;
            workspace
                .update(cx, |workspace, cx| {
                    workspace.toolbar_hover.hide = None;
                    workspace
                        .toolbar_hover
                        .revealed
                        .retain(|shown| *shown != place);
                    cx.notify();
                })
                .ok();
        });
        self.toolbar_hover.hide = Some((place, task));
    }

    /// A place's hover bars, floating over its edge while they show.
    fn hover_overlay(
        &mut self,
        place: Place,
        toolbars: &[Toolbar],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let frame = Self::frame_for(place);
        let ui = crate::ui::ui_theme(cx);
        let shown: Vec<&Toolbar> = toolbars
            .iter()
            .filter(|toolbar| toolbar.is_shown(&self.conditions_for(toolbar, cx)))
            .collect();
        let bars: Vec<AnyElement> = shown
            .into_iter()
            .map(|toolbar| {
                let items = self.render_items(toolbar, frame, None, window, cx);
                let body = floating_surface(bar(frame, toolbar.density, &ui), &ui).children(items);
                self.with_context_menu(body, toolbar, cx)
            })
            .collect();
        if bars.is_empty() {
            return None;
        }
        let overlay = edge_at(div().flex().gap(ui.space_md), place, gpui::px(0.))
            .when(frame == BarFrame::Column, |overlay| {
                overlay.flex_col().p(ui.space_md)
            })
            .when(frame != BarFrame::Column, |overlay| {
                overlay.flex_row().justify_center().p(ui.space_xs)
            })
            .id(gpui::ElementId::Name(
                format!("toolbar-overlay-{place:?}").into(),
            ))
            .occlude()
            .on_hover(cx.listener(move |workspace, hovered: &bool, window, cx| {
                if *hovered {
                    workspace.toolbar_hover.hide = None;
                } else {
                    workspace.start_hide(place, window, cx);
                }
            }))
            .children(bars);
        Some(overlay.into_any_element())
    }

    // ---- Keyboard ----

    /// The bars the keyboard can move into, in order, with where it can
    /// stop in each: the docked ones, then those floating over the note.
    fn focusable_toolbars(&self, cx: &gpui::App) -> Vec<(String, Vec<FocusStop>)> {
        let docked = DOCKED
            .iter()
            .flat_map(|place| self.config.toolbars.at(*place))
            .map(|toolbar| (toolbar.id.clone(), focus_stops(toolbar, has_add(toolbar))));
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
        docked
            .chain(floating)
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
        let Some(menu) = self.config.toolbars.menu(menu_id).cloned() else {
            return;
        };
        let workspace = cx.entity().downgrade();
        let items = menu
            .items
            .iter()
            .map(|id| {
                let command = id.clone();
                let workspace = workspace.clone();
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
            })
            .collect();
        let anchor = self.menu_anchor(toolbar, index, by_pointer, window, cx);
        self.menu.open(items, anchor, window, cx);
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
            return match toolbar.place {
                Place::StatusBar | Place::EditorBottom => MenuAnchor::Above { key },
                _ => MenuAnchor::Below {
                    key,
                    align_right: toolbar.place == Place::WindowRight,
                },
            };
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
