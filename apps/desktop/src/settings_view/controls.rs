//! The settings screen's building blocks: the two-column row, pill
//! switches, dropdown buttons, buttons, steppers, keycaps, swatches and
//! the dropdown menu's panel. Each takes its look from
//! [`SettingsTheme`] and leaves behaviour to the caller.
//!
//! These are general enough to share with other screens.

use gpui::{
    AnyElement, Corner, Div, ElementId, Hsla, SharedString, Stateful, anchored, deferred, div,
    point, prelude::*, px,
};

use crate::icons::{IconName, icon};
use crate::theme::SettingsTheme;

/// A row with its text on the left and its control on the right. The
/// text column takes the room that's left and wraps; the control column
/// is as wide as its content. They never overlap: when the row gets too
/// narrow for both, the control column wraps its controls onto more
/// lines before the text column goes below its minimum width.
/// `name` tells rows apart in tests: the columns get the debug selectors
/// `settings-text-<name>` and `settings-control-<name>`.
pub fn two_column_row(
    name: &str,
    text: impl IntoElement,
    control: Option<AnyElement>,
    style: &SettingsTheme,
) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(style.row_gap)
        .child(
            div()
                .debug_selector(|| format!("settings-text-{name}"))
                .flex_1()
                .min_w(style.text_min_width)
                .child(text),
        )
        .children(control.map(|control| {
            div()
                .debug_selector(|| format!("settings-control-{name}"))
                .flex_initial()
                .min_w_0()
                .flex()
                .flex_wrap()
                .justify_end()
                .items_center()
                .gap(style.control_gap)
                .child(control)
        }))
}

/// A row's title, description and any notes under them, such as an
/// error, stacked in the text column.
pub fn row_text(
    title: impl Into<SharedString>,
    description: Option<AnyElement>,
    notes: Vec<AnyElement>,
    style: &SettingsTheme,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(style.text_gap)
        .child(div().child(title.into()))
        .children(description.map(|description| {
            div()
                .text_size(style.small_text_size)
                .text_color(style.text_muted)
                .child(description)
        }))
        .children(notes.into_iter().map(|note| {
            div()
                .text_size(style.small_text_size)
                .text_color(style.warning)
                .child(note)
        }))
}

fn with_focus(element: Stateful<Div>, focused: bool, style: &SettingsTheme) -> Stateful<Div> {
    element.when(focused, |element| element.shadow(vec![style.focus()]))
}

/// A pill switch; the knob sits right and the track fills while it's on.
pub fn toggle_switch(
    id: impl Into<ElementId>,
    on: bool,
    focused: bool,
    style: &SettingsTheme,
) -> Stateful<Div> {
    let knob = style.toggle_height - style.toggle_knob_inset * 2.;
    let track = if on { style.accent } else { style.toggle_off };
    let switch = div()
        .id(id)
        .flex_none()
        .w(style.toggle_width)
        .h(style.toggle_height)
        .p(style.toggle_knob_inset)
        .flex()
        .items_center()
        .when(on, |track| track.justify_end())
        .rounded(style.toggle_height)
        .bg(track)
        .cursor_pointer()
        .child(
            div()
                .size(knob)
                .rounded(knob)
                .bg(style.knob)
                .shadow(vec![style.lift()]),
        );
    with_focus(switch, focused, style)
}

/// The raised surface buttons, dropdowns and steppers share.
fn raised(id: impl Into<ElementId>, style: &SettingsTheme) -> Stateful<Div> {
    let hover = style.hover;
    div()
        .id(id)
        .flex_none()
        .h(style.control_height)
        .flex()
        .items_center()
        .rounded(style.radius)
        .bg(style.control_background)
        .shadow(vec![style.outline(), style.lift()])
        .cursor_pointer()
        .hover(move |button| button.bg(hover))
}

/// A compact button showing the current choice, with an up-down chevron.
pub fn dropdown_button(
    id: impl Into<ElementId>,
    label: impl IntoElement,
    focused: bool,
    style: &SettingsTheme,
) -> Stateful<Div> {
    let button = raised(id, style)
        .flex_shrink()
        .min_w_0()
        .max_w(style.menu_width)
        .gap(style.gap_sm)
        .pl(style.control_padding_x)
        .pr(style.control_padding_x * 0.75)
        .child(div().min_w_0().truncate().child(label))
        .child(
            icon(IconName::CaretUpDown)
                .flex_none()
                .size(style.small_icon_size)
                .text_color(style.text_muted),
        );
    with_focus(button, focused, style)
}

/// A small bordered button, or a filled one for the main action.
pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    primary: bool,
    focused: bool,
    style: &SettingsTheme,
) -> Stateful<Div> {
    let button = raised(id, style)
        .px(style.control_padding_x)
        .whitespace_nowrap()
        .child(label.into())
        .when(primary, |button| {
            button.bg(style.accent).text_color(style.on_accent)
        });
    with_focus(button, focused, style)
}

/// A borderless square button holding one icon.
pub fn icon_button(
    id: impl Into<ElementId>,
    name: IconName,
    color: Hsla,
    style: &SettingsTheme,
) -> Stateful<Div> {
    let hover = style.hover;
    div()
        .id(id)
        .flex_none()
        .size(style.control_height)
        .flex()
        .items_center()
        .justify_center()
        .rounded(style.radius)
        .cursor_pointer()
        .hover(move |button| button.bg(hover))
        .child(icon(name).size(style.icon_size).text_color(color))
}

/// The box a text input sits in: a bare input on a field fill, ringed
/// while it has focus. The fill is opaque so the ring stays a ring.
pub fn field_box(
    input: impl IntoElement,
    leading: Option<IconName>,
    focused: bool,
    style: &SettingsTheme,
) -> Div {
    div()
        .flex_none()
        .h(style.control_height)
        .px(style.control_gap)
        .flex()
        .items_center()
        .gap(style.gap_sm)
        .rounded(style.radius)
        .bg(style.control_background)
        .shadow(if focused {
            vec![style.focus()]
        } else {
            vec![style.outline()]
        })
        .children(leading.map(|name| {
            icon(name)
                .flex_none()
                .size(style.small_icon_size)
                .text_color(style.text_muted)
        }))
        .child(div().flex_1().min_w_0().child(input))
}

/// A number with a button on each side to step it.
pub fn stepper(
    id: impl Into<ElementId>,
    minus: Stateful<Div>,
    value: impl Into<SharedString>,
    plus: Stateful<Div>,
    focused: bool,
    style: &SettingsTheme,
) -> Stateful<Div> {
    let stepper = raised(id, style)
        .cursor_default()
        .child(minus)
        .child(
            div()
                .min_w(style.stepper_value_width)
                .flex()
                .justify_center()
                .child(value.into()),
        )
        .child(plus);
    with_focus(stepper, focused, style)
}

/// A key as a small raised cap.
pub fn keycap(label: impl Into<SharedString>, style: &SettingsTheme) -> Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(style.gap_xs)
        .px(style.keycap_padding_x)
        .py(style.keycap_padding_y)
        .rounded(style.radius)
        .bg(style.control_background)
        .shadow(vec![style.outline(), style.lift()])
        .text_size(style.small_text_size)
        .whitespace_nowrap()
        .child(label.into())
}

/// A round colour swatch, ringed while it's the chosen one.
pub fn swatch(
    id: impl Into<ElementId>,
    color: Hsla,
    chosen: bool,
    style: &SettingsTheme,
) -> Stateful<Div> {
    let ring = gpui::BoxShadow {
        color: style.text,
        offset: point(px(0.), px(0.)),
        blur_radius: style.ring_blur,
        spread_radius: style.ring_width * 2.,
    };
    let gap = gpui::BoxShadow {
        spread_radius: style.ring_width * 2.,
        color: style.card_background,
        ..ring.clone()
    };
    let outer = gpui::BoxShadow {
        spread_radius: style.ring_width * 4.,
        ..ring
    };
    div()
        .id(id)
        .flex_none()
        .size(style.swatch_size)
        .rounded(style.swatch_size)
        .bg(color)
        .cursor_pointer()
        .when(chosen, |swatch| swatch.shadow(vec![outer, gap]))
}

/// A popover panel hung under the right edge of whatever comes before it
/// in a relative container, drawn above everything else.
pub fn popover(panel: impl IntoElement, style: &SettingsTheme) -> Div {
    div().absolute().top_full().right_0().child(
        deferred(
            anchored()
                .anchor(Corner::TopRight)
                .offset(point(px(0.), style.menu_offset))
                .snap_to_window_with_margin(style.nav_padding)
                .child(panel),
        )
        .with_priority(1),
    )
}

/// The panel a dropdown's options sit on.
pub fn menu_panel(style: &SettingsTheme) -> Stateful<Div> {
    div()
        .id("settings-menu")
        .occlude()
        .w(style.menu_width)
        .flex()
        .flex_col()
        .gap(style.gap_xs)
        .p(style.gap_sm)
        .rounded(style.radius)
        .bg(style.background)
        .shadow(vec![style.outline(), style.popover_shadow()])
}

/// One option in a dropdown menu.
pub fn menu_option(
    id: impl Into<ElementId>,
    label: impl IntoElement,
    chosen: bool,
    highlighted: bool,
    style: &SettingsTheme,
) -> Stateful<Div> {
    let hover = style.hover;
    let check = div()
        .flex_none()
        .size(style.small_icon_size)
        .when(chosen, |slot| {
            slot.child(
                icon(IconName::Check)
                    .size(style.small_icon_size)
                    .text_color(style.text),
            )
        });
    div()
        .id(id)
        .flex_none()
        .flex()
        .items_center()
        .gap(style.control_gap)
        .h(style.control_height)
        .px(style.control_gap)
        .rounded(style.radius)
        .cursor_pointer()
        .when(highlighted, |option| option.bg(style.selected))
        .when(!highlighted, |option| option.hover(move |o| o.bg(hover)))
        .child(check)
        .child(div().flex_1().min_w_0().truncate().child(label))
}
