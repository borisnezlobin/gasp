//! The settings screen's building blocks: the two-column row, pill
//! switches, dropdown buttons, buttons, steppers, removable key chips,
//! the field a shortcut is pressed into, swatches, the dropdown menu's
//! panel and the note hung under a control. Each takes its look from
//! [`SettingsTheme`] and leaves behaviour to the caller.
//!
//! These are general enough to share with other screens.

use gpui::{
    AnyElement, Corner, Div, ElementId, Hsla, SharedString, Stateful, anchored, deferred, div,
    point, prelude::*, px,
};

use crate::icons::{IconName, icon};
use crate::picker::shortcut::Shortcut;
use crate::theme::{KeycapTheme, SettingsTheme};
use crate::ui::{Tooltip, keycap};

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
    raised_button(id, false, style)
}

/// A raised button's surface, in the accent for the main action, with
/// its fill under the pointer.
fn raised_button(id: impl Into<ElementId>, primary: bool, style: &SettingsTheme) -> Stateful<Div> {
    let (fill, hover) = if primary {
        (style.accent, style.accent_hover)
    } else {
        (style.control_background, style.hover)
    };
    raised_surface(id, style)
        .bg(fill)
        .when(primary, |button| button.text_color(style.on_accent))
        .cursor_pointer()
        .hover(move |button| button.bg(hover))
}

/// A raised control's surface without its own hover, for one that holds
/// buttons: each button shows its hover, not the whole control.
fn raised_surface(id: impl Into<ElementId>, style: &SettingsTheme) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .h(style.control_height)
        .flex()
        .items_center()
        .rounded(style.radius)
        .bg(style.control_background)
        .shadow(vec![style.outline(), style.lift()])
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
    let button = raised_button(id, primary, style)
        .px(style.control_padding_x)
        .whitespace_nowrap()
        .child(label.into());
    with_focus(button, focused, style)
}

/// A button with an icon before its label, such as "New snippet".
pub fn icon_label_button(
    id: impl Into<ElementId>,
    name: IconName,
    label: impl Into<SharedString>,
    primary: bool,
    focused: bool,
    style: &SettingsTheme,
) -> Stateful<Div> {
    let color = if primary { style.on_accent } else { style.text };
    let button = raised_button(id, primary, style)
        .gap(style.gap_sm * 1.5)
        .pl(style.control_padding_x * 0.75)
        .pr(style.control_padding_x)
        .whitespace_nowrap()
        .child(
            icon(name)
                .flex_none()
                .size(style.small_icon_size)
                .text_color(color),
        )
        .child(label.into());
    with_focus(button, focused, style)
}

/// One of a few options side by side. The chosen one is filled and
/// checked, so it reads as picked rather than focused.
pub fn choice_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    chosen: bool,
    style: &SettingsTheme,
) -> Stateful<Div> {
    let check = chosen.then(|| {
        icon(IconName::Check)
            .flex_none()
            .size(style.small_icon_size)
            .text_color(style.text)
    });
    raised(id, style)
        .gap(style.gap_sm)
        .px(style.control_padding_x)
        .whitespace_nowrap()
        .when(chosen, |button| button.bg(style.selected))
        .children(check)
        .child(label.into())
}

/// One option of a [`segmented`] control. The chosen one is raised out
/// of the track; the others are its text alone.
pub fn segment(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    chosen: bool,
    style: &SettingsTheme,
) -> Stateful<Div> {
    let text = style.text;
    div()
        .id(id)
        .flex_none()
        .h_full()
        .flex()
        .items_center()
        .px(style.control_padding_x)
        .rounded(style.radius - style.segment_inset)
        .whitespace_nowrap()
        .cursor_pointer()
        .map(|segment| match chosen {
            true => segment
                .bg(style.control_background)
                .text_color(text)
                .shadow(vec![style.outline(), style.lift()]),
            false => segment
                .text_color(style.text_muted)
                .hover(move |segment| segment.text_color(text)),
        })
        .child(label.into())
}

/// Two or three choices side by side in one track, such as where a
/// snippet works. Left and right move the choice; the caller wires them.
pub fn segmented(
    id: impl Into<ElementId>,
    segments: impl IntoIterator<Item = Stateful<Div>>,
    focused: bool,
    style: &SettingsTheme,
) -> Stateful<Div> {
    let track = div()
        .id(id)
        .flex_none()
        .h(style.control_height)
        .p(style.segment_inset)
        .flex()
        .items_center()
        .gap(style.segment_inset)
        .rounded(style.radius)
        .bg(style.segment_track)
        .children(segments);
    with_focus(track, focused, style)
}

/// A button that can't be pressed right now: the same shape, with its
/// label faded and no hover.
pub fn inert_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    style: &SettingsTheme,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .h(style.control_height)
        .flex()
        .items_center()
        .px(style.control_padding_x)
        .rounded(style.radius)
        .bg(style.control_background)
        .shadow(vec![style.outline()])
        .whitespace_nowrap()
        .text_color(style.text_faint)
        .child(label.into())
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
    let stepper = raised_surface(id, style)
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

/// A shortcut's chip with a cross inside its right edge that removes it.
/// The cross is always shown, so removing never hides behind a hover,
/// and its tooltip says which key it removes. `marked` stresses the chip,
/// such as for the key a search found, and `warning` for a key another
/// command also uses; both tint it.
pub fn removable_keycap(
    id: SharedString,
    shortcut: Shortcut,
    marked: bool,
    warning: bool,
    keycaps: &KeycapTheme,
    style: &SettingsTheme,
) -> (Div, Stateful<Div>) {
    let cross = keycaps.height - style.gap_sm;
    let (hover, strong) = (keycaps.fill, style_text(keycaps));
    let remove_label = format!("Remove {}", shortcut.label());
    let group = SharedString::from(format!("{id}-group"));
    let remove = div()
        .id(id)
        .flex_none()
        .size(cross)
        .flex()
        .items_center()
        .justify_center()
        .rounded(keycaps.radius - style.hairline)
        .cursor_pointer()
        .hover(move |button| button.bg(hover))
        .tooltip(Tooltip::new(remove_label, None).builder())
        .group(group.clone())
        .child(
            icon(IconName::X)
                .size(keycaps.icon_size)
                .text_color(style.text_muted)
                .group_hover(group, move |cross| cross.text_color(strong)),
        );
    // Colour, not a ring or shadow, marks a chip: shadows fill in under
    // a see-through chip rather than outlining it.
    let keycaps = if warning {
        keycaps.clone().on_text(style.warning)
    } else if marked {
        keycaps.clone().emphasized()
    } else {
        keycaps.clone()
    };
    let chip = keycap(shortcut, &keycaps).pr(style.gap_xs);
    (chip, remove)
}

/// The glyph colour at full strength, for a hovered cross.
fn style_text(keycaps: &KeycapTheme) -> Hsla {
    Hsla {
        a: 1.,
        ..keycaps.glyph
    }
}

/// A small borderless button holding one small icon, for use inside a
/// field or chip.
pub fn small_icon_button(
    id: impl Into<ElementId>,
    name: IconName,
    style: &SettingsTheme,
) -> Stateful<Div> {
    let hover = style.hover;
    let size = style.control_height - style.gap_sm * 2.;
    div()
        .id(id)
        .flex_none()
        .size(size)
        .flex()
        .items_center()
        .justify_center()
        .rounded(style.radius - style.gap_xs)
        .cursor_pointer()
        .hover(move |button| button.bg(hover))
        .child(
            icon(name)
                .size(style.small_icon_size)
                .text_color(style.text_muted),
        )
}

/// The box a chord is pressed into: ringed like a focused field, saying
/// what it waits for, with a button that stops waiting.
pub fn capture_field(
    prompt: impl Into<SharedString>,
    cancel: Stateful<Div>,
    style: &SettingsTheme,
) -> Div {
    div()
        .flex_none()
        .h(style.control_height)
        .min_w(style.capture_field_width)
        .pl(style.control_gap)
        .pr(style.gap_xs)
        .flex()
        .items_center()
        .justify_between()
        .gap(style.gap_sm)
        .rounded(style.radius)
        .bg(style.control_background)
        .shadow(vec![style.focus()])
        .text_size(style.small_text_size)
        .text_color(style.text_muted)
        .whitespace_nowrap()
        .child(prompt.into())
        .child(cancel)
}

/// A note hung under the control before it in a relative container, such
/// as why a value or key was refused. It's drawn over the rows below, so
/// showing it moves nothing.
pub fn control_note(message: impl Into<SharedString>, style: &SettingsTheme) -> Div {
    let panel = div()
        .occlude()
        .max_w(style.menu_width)
        .flex()
        .items_start()
        .gap(style.gap_sm)
        .px(style.control_gap)
        .py(style.gap_sm * 1.5)
        .rounded(style.radius)
        .bg(style.background)
        .shadow(vec![style.outline(), style.popover_shadow()])
        .text_size(style.small_text_size)
        .text_color(style.text)
        .child(
            icon(IconName::WarningCircle)
                .flex_none()
                .mt(style.gap_xs)
                .size(style.small_icon_size)
                .text_color(style.warning),
        )
        .child(div().min_w_0().child(message.into()));
    popover(panel, style)
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
