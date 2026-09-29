//! Drawing a toolbar: its bar, and each item as a button, a status
//! widget, a separator, a spacer or a menu button. Sizes come from the
//! theme's `toolbar.` tokens, colours from the UI theme's controls.

use gasp_config::toolbars::{Density, Toolbar, ToolbarItem, ToolbarMenu, Widget};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    AnyElement, AnyView, App, BoxShadow, Div, ElementId, EntityId, MouseButton, Pixels,
    SharedString, Stateful, canvas, div, prelude::*,
};

use super::fitted::{BarAxis, FittedItems, OverflowCell, Slot, SlotKind, fitted_items};
use super::{
    AddToToolbar, FocusStop, OpenToolbarOverflow, PressToolbarItem, add_key, button_label,
    item_key, more_key,
};
use crate::icons::{IconName, icon};
use crate::theme::UiTheme;
use crate::ui::Selectable;
use crate::ui::Tooltip;
use crate::workspace::status::StatusInfo;

/// What a bar sits in, which sets its shape and sizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarFrame {
    /// The row along the window's bottom, as tall as it's always been.
    StatusBar,
    /// A strip above or below the notes.
    Row,
    /// A strip down a side of the window.
    Column,
    /// A bar floating over the note, or one shown on hover over an edge.
    Floating,
    /// A pill floating down a side of the note.
    FloatingColumn,
}

impl BarFrame {
    pub fn is_column(self) -> bool {
        matches!(self, BarFrame::Column | BarFrame::FloatingColumn)
    }

    fn axis(self) -> BarAxis {
        if self.is_column() {
            BarAxis::Column
        } else {
            BarAxis::Row
        }
    }
}

/// The widest each status widget has been while showing one note the
/// same way, so a count or a cursor position that gets shorter keeps its
/// room and the widgets beside it stay where they are. It starts over
/// for another note, or when a selection starts or ends.
#[derive(Clone, Default)]
pub struct WidgetWidths {
    showing: Rc<Cell<Option<(EntityId, bool)>>>,
    widest: Rc<RefCell<HashMap<Widget, Pixels>>>,
}

impl WidgetWidths {
    /// Says what the status widgets describe now: `note`, and whether
    /// its selection. Anything else forgets the widths.
    pub fn describe(&self, note: Option<EntityId>, for_selection: bool) {
        let showing = note.map(|note| (note, for_selection));
        if self.showing.get() != showing {
            self.showing.set(showing);
            self.widest.borrow_mut().clear();
        }
    }

    pub fn widest(&self, widget: Widget) -> Option<Pixels> {
        self.widest.borrow().get(&widget).copied()
    }

    /// An empty element over the widget that keeps its widest width.
    fn probe(&self, widget: Widget) -> impl IntoElement {
        let widest = self.widest.clone();
        canvas(
            move |bounds, _, _| {
                let mut widest = widest.borrow_mut();
                let width = widest.entry(widget).or_default();
                *width = (*width).max(bounds.size.width);
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
    }
}

/// What a bar's items show now.
pub struct BarState<'a> {
    pub status: Option<&'a StatusInfo>,
    /// The status widgets' widest widths, where they keep them.
    pub widths: Option<&'a WidgetWidths>,
    pub sync: Option<AnyView>,
    /// Toggle commands that are on where the cursor is.
    pub active: &'a [&'static str],
    /// Where the keyboard is in this bar, if it's here.
    pub focus: Option<FocusStop>,
    /// Whether a command can run now; the rest are drawn disabled.
    pub can_run: &'a dyn Fn(&str) -> bool,
    pub menus: &'a [ToolbarMenu],
    pub frame: BarFrame,
}

/// The sizes one bar's items share.
#[derive(Clone, Copy, Debug)]
struct Metrics {
    side: Pixels,
    icon: Pixels,
    text: Pixels,
    gap: Pixels,
}

fn metrics(frame: BarFrame, density: Density, theme: &UiTheme) -> Metrics {
    let tokens = &theme.toolbar;
    let (side, icon, text) = match (frame, density) {
        // The status bar keeps its height, so its buttons fit inside it.
        (BarFrame::StatusBar, _) => (
            theme.status_height - theme.space_xs * 2.,
            theme.small_icon_size,
            theme.small_font_size,
        ),
        (_, Density::Compact) => (
            tokens.compact_button,
            tokens.compact_icon,
            theme.small_font_size,
        ),
        (_, Density::Comfortable) => (
            tokens.comfortable_button,
            tokens.comfortable_icon,
            theme.font_size,
        ),
    };
    Metrics {
        side,
        icon,
        text,
        gap: tokens.gap(density),
    }
}

/// How deep a docked strip's bar is across its length, so a pane that
/// doesn't show the bar can keep the same room.
pub fn bar_thickness(frame: BarFrame, density: Density, theme: &UiTheme) -> Pixels {
    metrics(frame, density, theme).side + theme.toolbar.padding * 2.
}

/// The bar itself, before its items: laid out for `frame`. A strip's bar
/// fills its strip, so its items fit against the strip's length.
pub fn bar(frame: BarFrame, density: Density, theme: &UiTheme) -> Div {
    let m = metrics(frame, density, theme);
    let padding = theme.toolbar.padding;
    match frame {
        BarFrame::StatusBar => div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(theme.status_gap)
            .h(theme.status_height)
            .px(theme.space_lg)
            .text_size(theme.small_font_size)
            .text_color(theme.text_faint),
        BarFrame::Row => div()
            .flex()
            .flex_row()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(m.gap)
            .h(bar_thickness(frame, density, theme))
            .px(theme.space_md),
        BarFrame::Column => div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .items_center()
            .gap(m.gap)
            .w(bar_thickness(frame, density, theme))
            .py(theme.space_md),
        BarFrame::Floating => floating_surface(div().flex().flex_row().items_center(), theme)
            .min_w_0()
            .gap(m.gap)
            .p(padding),
        BarFrame::FloatingColumn => floating_surface(div().flex().flex_col().items_center(), theme)
            .min_h_0()
            .gap(m.gap)
            .p(padding),
    }
}

/// A surface that floats: the menu fill, its shadow and hairline ring,
/// and corners concentric with the buttons inside.
pub fn floating_surface(element: Div, theme: &UiTheme) -> Div {
    element
        .rounded(theme.toolbar.bar_radius())
        .bg(theme.menu_background)
        .shadow(theme.menu_shadows())
        .text_size(theme.small_font_size)
        .text_color(theme.text_muted)
}

/// Every item of `toolbar`, drawn for `state`. `attached` gives the open
/// menu for an item's key, drawn in the item's box. `add`, the button
/// that adds to the bar, goes just before its first spacer, where it moves
/// nothing, or else at the end.
pub fn bar_items(
    toolbar: &Toolbar,
    state: &BarState<'_>,
    attached: &mut dyn FnMut(&str) -> Option<AnyElement>,
    mut add: Option<AnyElement>,
    cx: &mut App,
) -> Vec<AnyElement> {
    let theme = crate::ui::ui_theme(cx);
    let m = metrics(state.frame, toolbar.density, &theme);
    let mut elements = Vec::new();
    let mut buttons: Vec<AnyElement> = Vec::new();
    for (index, item) in toolbar.items.iter().enumerate() {
        if let Some(button) = button_item(toolbar, index, state, attached, m, &theme, cx) {
            buttons.push(button);
            continue;
        }
        flush_buttons(&mut elements, &mut buttons, state.frame, m);
        if *item == ToolbarItem::Spacer {
            elements.extend(add.take());
        }
        elements.extend(passive_item(item, state, m, &theme));
    }
    flush_buttons(&mut elements, &mut buttons, state.frame, m);
    elements.extend(add);
    elements
}

/// Every item of a docked `toolbar`, fitted to the length its bar has:
/// what doesn't fit goes into a trailing More button, which `overflow`
/// tells the workspace about.
pub fn fitted_bar_items(
    toolbar: &Toolbar,
    state: &BarState<'_>,
    attached: &mut dyn FnMut(&str) -> Option<AnyElement>,
    overflow: OverflowCell,
    cx: &mut App,
) -> FittedItems {
    let theme = crate::ui::ui_theme(cx);
    let m = metrics(state.frame, toolbar.density, &theme);
    let slots = toolbar
        .items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let button = button_item(toolbar, index, state, attached, m, &theme, cx);
            let slot = |kind, element| Slot {
                index,
                kind,
                element,
            };
            match (button, item) {
                (Some(button), _) => Some(slot(SlotKind::Button, Some(button))),
                (None, ToolbarItem::Spacer) => Some(slot(SlotKind::Spacer, None)),
                (None, ToolbarItem::Separator) => Some(slot(
                    SlotKind::Separator,
                    passive_item(item, state, m, &theme),
                )),
                (None, _) => passive_item(item, state, m, &theme)
                    .map(|widget| slot(SlotKind::Widget, Some(widget))),
            }
        })
        .collect();
    let focused = state.focus == Some(FocusStop::More);
    let open = attached(&more_key(&toolbar.id));
    let more = more_button(toolbar, focused, open, m, &theme);
    fitted_items(state.frame.axis(), m.gap, slots, more, overflow)
}

/// Item `index` of `toolbar` when it's a button: a command, or a menu
/// that exists.
fn button_item(
    toolbar: &Toolbar,
    index: usize,
    state: &BarState<'_>,
    attached: &mut dyn FnMut(&str) -> Option<AnyElement>,
    m: Metrics,
    theme: &UiTheme,
    cx: &App,
) -> Option<AnyElement> {
    let key = item_key(&toolbar.id, index);
    let focused = state.focus == Some(FocusStop::Item(index));
    match toolbar.items.get(index)? {
        ToolbarItem::Command(id) => {
            let look = ButtonLook::for_command(id, state, focused);
            Some(command_button(&key, toolbar, index, id, look, m, theme, cx))
        }
        ToolbarItem::Menu(id) => {
            let open = attached(&key);
            menu_button(
                &key,
                toolbar,
                index,
                state.menus.iter().find(|menu| menu.id == *id),
                focused,
                open,
                m,
                theme,
            )
        }
        _ => None,
    }
}

/// The button at a bar's end that holds the items that don't fit: its
/// icon, pressed in while its menu is open.
fn more_button(
    toolbar: &Toolbar,
    focused: bool,
    open: Option<AnyElement>,
    m: Metrics,
    theme: &UiTheme,
) -> AnyElement {
    let look = ButtonLook {
        active: open.is_some(),
        disabled: false,
        focused,
    };
    let has_open = open.is_some();
    let target: SharedString = toolbar.id.clone().into();
    button_box(&more_key(&toolbar.id), look, theme)
        .size(m.side)
        .child(
            icon(IconName::DotsThree)
                .size(m.icon)
                .text_color(icon_color(look, theme)),
        )
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            let open = OpenToolbarOverflow {
                toolbar: target.clone(),
            };
            window.dispatch_action(Box::new(open), cx);
        })
        .when(!has_open, |button| {
            button.tooltip(Tooltip::new("More buttons", None).builder())
        })
        .children(open)
        .into_any_element()
}

/// Buttons side by side sit closer together in the status bar than its
/// widgets do, so a run of them is grouped there.
fn flush_buttons(
    elements: &mut Vec<AnyElement>,
    buttons: &mut Vec<AnyElement>,
    frame: BarFrame,
    m: Metrics,
) {
    if buttons.is_empty() {
        return;
    }
    if frame != BarFrame::StatusBar {
        elements.append(buttons);
        return;
    }
    let group = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(m.gap)
        .children(std::mem::take(buttons));
    elements.push(group.into_any_element());
}

/// A widget, separator or spacer.
fn passive_item(
    item: &ToolbarItem,
    state: &BarState<'_>,
    m: Metrics,
    theme: &UiTheme,
) -> Option<AnyElement> {
    match item {
        ToolbarItem::Widget(Widget::Sync) => state.sync.clone().map(AnyView::into_any_element),
        ToolbarItem::Widget(widget) => {
            let text = state.status?.widget_text(*widget)?;
            Some(status_widget(*widget, text, state.widths, theme))
        }
        ToolbarItem::Separator => Some(separator(state.frame, m, theme)),
        ToolbarItem::Spacer => Some(div().flex_1().into_any_element()),
        ToolbarItem::Command(_) | ToolbarItem::Menu(_) => None,
    }
}

/// A status widget's text, in figures of one width, in a box that keeps
/// the widest it's been so the widgets beside it stay put; the cursor
/// position keeps room for "000:00" from the start. The text sits at the
/// box's end, beside the next widget.
fn status_widget(
    widget: Widget,
    text: String,
    widths: Option<&WidgetWidths>,
    theme: &UiTheme,
) -> AnyElement {
    let name = widget.name();
    let reserved = match widget {
        Widget::CursorPosition => theme.status_position_width,
        _ => Pixels::ZERO,
    };
    let widest = widths
        .and_then(|widths| widths.widest(widget))
        .unwrap_or_default();
    div()
        .id(ElementId::Name(format!("status-{name}").into()))
        .selector(move || format!("status-{name}"))
        .relative()
        .flex()
        .flex_none()
        .justify_end()
        .min_w(reserved.max(widest))
        .font(theme.tabular_font())
        .child(SharedString::from(text))
        .children(widths.map(|widths| widths.probe(widget)))
        .into_any_element()
}

fn separator(frame: BarFrame, m: Metrics, theme: &UiTheme) -> AnyElement {
    let line = div().flex_none().bg(theme.menu_separator);
    let length = m.side * 0.6;
    let width = theme.toolbar.separator_width;
    let line = if frame.is_column() {
        line.h(width).w(length)
    } else {
        line.w(width).h(length)
    };
    line.into_any_element()
}

/// How a command's button looks now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButtonLook {
    pub active: bool,
    pub disabled: bool,
    pub focused: bool,
}

impl ButtonLook {
    fn for_command(id: &str, state: &BarState<'_>, focused: bool) -> ButtonLook {
        ButtonLook {
            active: state.active.contains(&id),
            disabled: !(state.can_run)(id),
            focused,
        }
    }
}

/// The fill and ring of a toolbar button in its state: a toggle that's on
/// gets the "on" fill with a hairline ring, keyboard focus the focus ring.
fn button_box(id: &str, look: ButtonLook, theme: &UiTheme) -> Stateful<Div> {
    let mut shadows: Vec<BoxShadow> = Vec::new();
    if look.active {
        shadows.push(theme.ring(theme.menu_ring));
    }
    if look.focused {
        shadows.push(theme.focus());
    }
    let selector = id.to_owned();
    div()
        .id(ElementId::Name(id.to_owned().into()))
        .selector(move || selector)
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(theme.toolbar.button_radius)
        .when(look.active, |button| {
            button.bg(crate::theme::over(
                theme.control_active,
                theme.menu_background,
            ))
        })
        .when(!shadows.is_empty(), |button| button.shadow(shadows))
        .when(!look.disabled, |button| {
            button
                .cursor_pointer()
                .hover(|style| style.bg(theme.control_hover))
                .active(|style| style.bg(theme.control_pressed))
        })
}

fn icon_color(look: ButtonLook, theme: &UiTheme) -> gpui::Hsla {
    match (look.disabled, look.active) {
        (true, _) => theme.icon_disabled,
        (false, true) => theme.icon_active,
        (false, false) => theme.icon,
    }
}

fn text_color(look: ButtonLook, theme: &UiTheme) -> gpui::Hsla {
    match (look.disabled, look.active) {
        (true, _) => theme.icon_disabled,
        (false, true) => theme.text,
        (false, false) => theme.text_muted,
    }
}

/// A button's icon and label, as `style` says.
fn button_face(
    button: Stateful<Div>,
    icon_name: IconName,
    label: Option<String>,
    look: ButtonLook,
    m: Metrics,
    theme: &UiTheme,
) -> Stateful<Div> {
    let button = match &label {
        Some(_) => button
            .h(m.side)
            .px(theme.toolbar.padding * 2.)
            .gap(theme.toolbar.label_gap),
        None => button.size(m.side),
    };
    button
        .child(
            icon(icon_name)
                .size(m.icon)
                .text_color(icon_color(look, theme)),
        )
        .children(label.map(|text| {
            div()
                .text_size(m.text)
                .text_color(text_color(look, theme))
                .whitespace_nowrap()
                .child(text)
        }))
}

/// Presses an item from a click, keeping the keyboard where it was so a
/// command reaches the note.
fn on_press(button: Stateful<Div>, toolbar: &str, index: usize, disabled: bool) -> Stateful<Div> {
    if disabled {
        return button;
    }
    let toolbar: SharedString = toolbar.to_owned().into();
    button
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            let press = PressToolbarItem {
                toolbar: toolbar.clone(),
                index,
                by_pointer: true,
            };
            window.dispatch_action(Box::new(press), cx);
        })
}

#[allow(clippy::too_many_arguments)]
fn command_button(
    key: &str,
    toolbar: &Toolbar,
    index: usize,
    id: &str,
    look: ButtonLook,
    m: Metrics,
    theme: &UiTheme,
    cx: &App,
) -> AnyElement {
    let title = crate::ui::hints::command_title(id);
    let label = toolbar.style.shows_label().then(|| button_label(&title));
    let shows_icon = toolbar.style.shows_icon();
    let face = if shows_icon {
        button_face(
            button_box(key, look, theme),
            IconName::for_command(id),
            label,
            look,
            m,
            theme,
        )
    } else {
        label_only(
            button_box(key, look, theme),
            label.unwrap_or_default(),
            look,
            m,
            theme,
        )
    };
    on_press(face, &toolbar.id, index, look.disabled)
        .tooltip(Tooltip::for_command(id, cx).builder())
        .into_any_element()
}

fn label_only(
    button: Stateful<Div>,
    label: String,
    look: ButtonLook,
    m: Metrics,
    theme: &UiTheme,
) -> Stateful<Div> {
    button
        .h(m.side)
        .px(theme.toolbar.padding * 2.)
        .text_size(m.text)
        .text_color(text_color(look, theme))
        .whitespace_nowrap()
        .child(label)
}

/// A menu's button: its icon and a caret, pressed in while its menu is
/// open. `None` when the menu doesn't exist.
#[allow(clippy::too_many_arguments)]
fn menu_button(
    key: &str,
    toolbar: &Toolbar,
    index: usize,
    menu: Option<&ToolbarMenu>,
    focused: bool,
    open: Option<AnyElement>,
    m: Metrics,
    theme: &UiTheme,
) -> Option<AnyElement> {
    let menu = menu?;
    let look = ButtonLook {
        active: open.is_some(),
        disabled: false,
        focused,
    };
    let label = toolbar.style.shows_label().then(|| menu.title.clone());
    let icon_name = IconName::from_name(&menu.icon).unwrap_or(IconName::DotsThree);
    let base = button_box(key, look, theme);
    let face = if toolbar.style.shows_icon() {
        button_face(base, icon_name, label, look, m, theme)
            .w_auto()
            .px(theme.toolbar.padding)
    } else {
        label_only(base, label.unwrap_or_default(), look, m, theme)
    };
    let face = face.gap(theme.toolbar.padding).child(
        icon(IconName::CaretDown)
            .size(m.icon * 0.6)
            .text_color(icon_color(look, theme)),
    );
    let has_open = open.is_some();
    let button = on_press(face, &toolbar.id, index, false)
        .when(!has_open, |button| {
            button.tooltip(Tooltip::new(menu.title.clone(), None).builder())
        })
        .children(open);
    Some(button.into_any_element())
}

/// The small button that adds to a bar, shown while the pointer is on
/// the bar or the keyboard is on the button.
pub fn add_button(toolbar: &Toolbar, focused: bool, frame: BarFrame, cx: &mut App) -> AnyElement {
    let theme = crate::ui::ui_theme(cx);
    let m = metrics(frame, toolbar.density, &theme);
    let look = ButtonLook {
        active: false,
        disabled: false,
        focused,
    };
    let target: SharedString = toolbar.id.clone().into();
    let tooltip = format!("Add to {}", toolbar.title.to_lowercase());
    button_box(&add_key(&toolbar.id), look, &theme)
        .size(m.side)
        .child(icon(IconName::Plus).size(m.icon).text_color(theme.icon))
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            let add = AddToToolbar {
                toolbar: target.clone(),
            };
            window.dispatch_action(Box::new(add), cx);
        })
        .tooltip(Tooltip::new(tooltip, None).builder())
        .into_any_element()
}
