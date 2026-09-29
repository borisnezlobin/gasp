//! The settings screen's building blocks: the two-column row, pill
//! switches, dropdown buttons, buttons, steppers, removable key chips,
//! the field a shortcut is pressed into, swatches, the dropdown menu's
//! panel and the note hung under a control. Each takes its look from
//! [`SettingsTheme`] and leaves behaviour to the caller.
//!
//! These are general enough to share with other screens.

use gpui::{AnyElement, Div, ElementId, Hsla, SharedString, Stateful, div, prelude::*, px};

use super::popover::Popover;

use crate::icons::{IconName, icon};
use crate::picker::shortcut::Shortcut;
use crate::theme::{KeycapTheme, SettingsTheme, over};
use crate::ui::Selectable;
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
                .selector(|| format!("settings-text-{name}"))
                .flex_1()
                .min_w(style.text_min_width)
                .child(text),
        )
        .children(control.map(|control| {
            div()
                .selector(|| format!("settings-control-{name}"))
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
/// The track darkens under the pointer and more while pressed. It sits
/// in a [`focus_frame`], so its focus ring is concentric and the frame is
/// part of what's clicked.
pub fn toggle_switch(
    id: impl Into<ElementId>,
    on: bool,
    focused: bool,
    style: &SettingsTheme,
) -> Stateful<Div> {
    let knob = style.toggle_height - style.toggle_knob_inset * 2.;
    let (track, hover, pressed) = if on {
        (style.accent, style.accent_hover, style.accent_pressed)
    } else {
        (
            style.toggle_off,
            style.toggle_off_hover,
            style.toggle_off_pressed,
        )
    };
    let group = SharedString::from("settings-toggle");
    let track = div()
        .id("track")
        .flex_none()
        .w(style.toggle_width)
        .h(style.toggle_height)
        .p(style.toggle_knob_inset)
        .flex()
        .items_center()
        .when(on, |track| track.justify_end())
        .rounded_full()
        .bg(track)
        .group_hover(group.clone(), move |track| track.bg(hover))
        .group_active(group.clone(), move |track| track.bg(pressed))
        .child(
            div()
                .size(knob)
                .rounded_full()
                .bg(style.knob)
                .shadow(vec![style.lift()]),
        );
    focus_frame(track, focused, style)
        .id(id)
        .group(group)
        .cursor_pointer()
}

/// A control's fill at rest, under the pointer and while pressed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fills {
    pub rest: Hsla,
    pub hover: Hsla,
    pub pressed: Hsla,
}

impl Fills {
    /// `rest` with the see-through hover and press fills laid over it, so
    /// they read the same on any colour and stay opaque when it is.
    pub fn over(rest: Hsla, style: &SettingsTheme) -> Fills {
        Fills {
            rest,
            hover: over(style.hover_fill, rest),
            pressed: over(style.pressed, rest),
        }
    }

    /// `rest` that stays under the pointer, for a state the pointer
    /// mustn't hide, such as a chosen chip or a dropdown whose menu is
    /// open. A press still shows.
    pub fn held(rest: Hsla, style: &SettingsTheme) -> Fills {
        Fills {
            hover: rest,
            ..Fills::over(rest, style)
        }
    }

    /// A control that asks once more before it removes or resets, in
    /// the warning colour's fill.
    pub fn armed(style: &SettingsTheme) -> Fills {
        Fills::over(over(style.warning_fill, style.control_background), style)
    }

    fn plain(style: &SettingsTheme) -> Fills {
        Fills::over(style.control_background, style)
    }

    fn primary(style: &SettingsTheme) -> Fills {
        Fills {
            rest: style.accent,
            hover: style.accent_hover,
            pressed: style.accent_pressed,
        }
    }
}

/// A raised button's surface, in the accent for the main action.
fn raised_button(id: impl Into<ElementId>, primary: bool, style: &SettingsTheme) -> Stateful<Div> {
    let fills = if primary {
        Fills::primary(style)
    } else {
        Fills::plain(style)
    };
    raised_with(id, fills, style).when(primary, |button| button.text_color(style.on_accent))
}

/// A raised surface in `fills`. They're opaque, so the outline and focus
/// ring stay rings.
fn raised_with(id: impl Into<ElementId>, fills: Fills, style: &SettingsTheme) -> Stateful<Div> {
    raised_surface(id, style)
        .bg(fills.rest)
        .cursor_pointer()
        .hover(move |button| button.bg(fills.hover))
        .active(move |button| button.bg(fills.pressed))
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
    dropdown_button_in(id, label, focused, false, style)
}

/// A [`dropdown_button`] that stays pressed in while its menu is `open`.
pub fn dropdown_button_in(
    id: impl Into<ElementId>,
    label: impl IntoElement,
    focused: bool,
    open: bool,
    style: &SettingsTheme,
) -> Stateful<Div> {
    let fills = if open {
        Fills::held(over(style.pressed, style.control_background), style)
    } else {
        Fills::plain(style)
    };
    let button = raised_with(id, fills, style)
        .flex_shrink()
        .min_w_0()
        .max_w(style.menu_width)
        .gap(style.gap_sm)
        .pl(style.control_padding_x)
        .pr(style.control_padding_x * 0.75)
        .child(div().flex_1().min_w_0().truncate().child(label))
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
    label: impl IntoElement,
    primary: bool,
    focused: bool,
    style: &SettingsTheme,
) -> Stateful<Div> {
    let button = raised_button(id, primary, style)
        .px(style.control_padding_x)
        .whitespace_nowrap()
        .child(label);
    with_focus(button, focused, style)
}

/// A [`button`] in its own `fills`, such as [`Fills::armed`].
pub fn button_in(
    id: impl Into<ElementId>,
    label: impl IntoElement,
    fills: Fills,
    focused: bool,
    style: &SettingsTheme,
) -> Stateful<Div> {
    let button = raised_with(id, fills, style)
        .px(style.control_padding_x)
        .whitespace_nowrap()
        .child(label);
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
    let fills = if chosen {
        Fills::held(style.selected, style)
    } else {
        Fills::plain(style)
    };
    raised_with(id, fills, style)
        .gap(style.gap_sm)
        .px(style.control_padding_x)
        .whitespace_nowrap()
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
            false => {
                let pressed = style.pressed;
                segment
                    .text_color(style.text_muted)
                    .hover(move |segment| segment.text_color(text))
                    .active(move |segment| segment.bg(pressed))
            }
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

/// A borderless square button holding one icon, filled under the
/// pointer and more while pressed.
pub fn icon_button(
    id: impl Into<ElementId>,
    name: IconName,
    color: Hsla,
    style: &SettingsTheme,
) -> Stateful<Div> {
    let (hover, pressed) = (style.hover_fill, style.pressed);
    icon_square(id, name, color, style)
        .cursor_pointer()
        .hover(move |button| button.bg(hover))
        .active(move |button| button.bg(pressed))
}

/// An [`icon_button`] on a fill of its own, such as [`Fills::armed`].
pub fn icon_button_in(
    id: impl Into<ElementId>,
    name: IconName,
    color: Hsla,
    fills: Fills,
    style: &SettingsTheme,
) -> Stateful<Div> {
    icon_square(id, name, color, style)
        .bg(fills.rest)
        .cursor_pointer()
        .hover(move |button| button.bg(fills.hover))
        .active(move |button| button.bg(fills.pressed))
}

/// An [`icon_button`] that can't be pressed right now, such as moving
/// the first item up: its icon faded, with no fill under the pointer.
pub fn inert_icon_button(
    id: impl Into<ElementId>,
    name: IconName,
    style: &SettingsTheme,
) -> Stateful<Div> {
    icon_square(id, name, style.text_faint, style)
}

fn icon_square(
    id: impl Into<ElementId>,
    name: IconName,
    color: Hsla,
    style: &SettingsTheme,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .size(style.control_height)
        .flex()
        .items_center()
        .justify_center()
        .rounded(style.radius)
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
    let state = if focused {
        FieldState::Focused
    } else {
        FieldState::Idle
    };
    field_box_in(input, leading, state, style)
}

/// What a field's ring says about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldState {
    Idle,
    Focused,
    /// Its last value was refused: a ring in the warning colour, which
    /// stays while it's fixed.
    Refused,
}

impl FieldState {
    fn ring(self, style: &SettingsTheme) -> gpui::BoxShadow {
        match self {
            FieldState::Idle => style.outline(),
            FieldState::Focused => style.focus(),
            FieldState::Refused => crate::theme::focus_ring(style.warning),
        }
    }
}

/// A [`field_box`] in any [`FieldState`].
pub fn field_box_in(
    input: impl IntoElement,
    leading: Option<IconName>,
    state: FieldState,
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
        .shadow(vec![state.ring(style)])
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
        .rounded(keycaps.radius - style.gap_xs)
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
    let (hover, pressed) = (style.hover_fill, style.pressed);
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
        .active(move |button| button.bg(pressed))
        .child(
            icon(name)
                .size(style.small_icon_size)
                .text_color(style.text_muted),
        )
}

/// What a shortcut's "+" waits for, hung under it like a menu: a
/// keyboard and "Press a shortcut", or, once a chord was refused, why,
/// with a warning mark. `cancel` stops waiting.
pub fn capture_prompt(
    rejection: Option<String>,
    cancel: Stateful<Div>,
    style: &SettingsTheme,
) -> Popover<Div> {
    let (mark, color, text) = match rejection {
        Some(reason) => (IconName::WarningCircle, style.warning, reason),
        None => (
            IconName::Keyboard,
            style.text_muted,
            "Press a shortcut".to_string(),
        ),
    };
    let panel = note_panel(style)
        .selector(|| "capture-prompt".to_string())
        .items_center()
        .pr(style.gap_sm)
        .child(
            icon(mark)
                .flex_none()
                .size(style.small_icon_size)
                .text_color(color),
        )
        .child(div().flex_1().min_w_0().child(text))
        .child(cancel);
    popover(panel, style)
}

/// A note hung under the control before it in a relative container, such
/// as why a value or key was refused. It's drawn over the rows below, so
/// showing it moves nothing.
pub fn control_note(message: impl Into<SharedString>, style: &SettingsTheme) -> Popover<Div> {
    let panel = note_panel(style)
        .items_start()
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

/// The small raised panel a note or prompt hangs on under its control.
fn note_panel(style: &SettingsTheme) -> Div {
    div()
        .occlude()
        .min_w(style.capture_field_width)
        .max_w(style.menu_width)
        .flex()
        .gap(style.gap_sm)
        .px(style.control_gap)
        .py(style.gap_sm * 1.5)
        .rounded(style.radius)
        .bg(style.background)
        .shadow(vec![style.outline(), style.popover_shadow()])
        .text_size(style.small_text_size)
        .text_color(style.text)
}

/// A round colour swatch in a ring the size of a control, which is also
/// what's clicked. The ring is three filled circles on one centre (ring,
/// gap, colour), so it stays concentric: a shadow's spread would keep the
/// swatch's radius and draw a rounded square. The chosen swatch has a
/// ring in the text colour and a check; under the pointer the ring shows
/// faintly; pressed, a little stronger.
pub fn swatch(
    id: impl Into<ElementId>,
    color: Hsla,
    chosen: bool,
    style: &SettingsTheme,
) -> Stateful<Div> {
    let outer = style.swatch_size + (style.swatch_gap + style.swatch_ring) * 2.;
    let gap = style.swatch_size + style.swatch_gap * 2.;
    let (hover, pressed) = (style.hover, style.pressed);
    let check = chosen.then(|| {
        icon(IconName::Check)
            .size(style.small_icon_size)
            .text_color(crate::styling::ink_on(color))
    });
    let inner = div()
        .size(style.swatch_size)
        .rounded_full()
        .bg(color)
        .flex()
        .items_center()
        .justify_center()
        .children(check);
    div()
        .id(id)
        .flex_none()
        .size(outer)
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .map(|ring| match chosen {
            true => ring.bg(style.text),
            false => ring
                .hover(move |ring| ring.bg(hover))
                .active(move |ring| ring.bg(pressed)),
        })
        .child(
            div()
                .size(gap)
                .rounded_full()
                .bg(style.card_background)
                .flex()
                .items_center()
                .justify_center()
                .child(inner),
        )
}

/// The focus ring around a pill, such as the accent swatches, drawn as a
/// frame that's always there and filled only while focused. A shadow's
/// ring keeps the pill's radius and flattens at the ends; a frame's
/// fill stays concentric, and since it takes its room either way,
/// showing it moves nothing.
pub fn focus_frame(inner: impl IntoElement, focused: bool, style: &SettingsTheme) -> Div {
    div()
        .flex_none()
        .p(px(crate::theme::FOCUS_RING_WIDTH))
        .rounded_full()
        .when(focused, |frame| frame.bg(style.focus_ring))
        .child(inner)
}

/// The room a row's reset button takes. It's kept whether or not the
/// value differs from its default, so the button appearing moves nothing.
pub fn reset_slot(button: Option<Stateful<Div>>, style: &SettingsTheme) -> Div {
    div()
        .flex_none()
        .size(style.control_height)
        .flex()
        .items_center()
        .justify_center()
        .children(button)
}

/// A label that keeps the width of the widest it can say, so a button
/// whose words change (such as "Press again to reset") doesn't move what
/// sits beside it.
pub fn steady_label(shown: impl Into<SharedString>, widest: impl Into<SharedString>) -> Div {
    widest_of(
        div().flex().justify_center().child(shown.into()),
        [widest.into()],
    )
}

/// `shown`, as wide as the widest of `alternatives`: a dropdown labelled
/// this way keeps its width whichever of its options is picked. The
/// alternatives are laid out with no height and never drawn.
pub fn widest_of(
    shown: impl IntoElement,
    alternatives: impl IntoIterator<Item = SharedString>,
) -> Div {
    let sizer = div()
        .h_0()
        .overflow_hidden()
        .invisible()
        .flex()
        .flex_col()
        .children(
            alternatives
                .into_iter()
                .map(|label| div().whitespace_nowrap().child(label)),
        );
    div().flex().flex_col().child(sizer).child(shown)
}

/// A popover panel hung under the right edge of whatever comes before it
/// in a relative container, or over it when there isn't room below,
/// drawn above everything else. Its height is capped to the room on the
/// side it takes.
pub fn popover<E: IntoElement + Styled + 'static>(panel: E, style: &SettingsTheme) -> Popover<E> {
    super::popover::popover(panel, style.menu_offset, style.menu_margin)
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
        .rounded(style.radius + style.gap_sm)
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
    let (hover, pressed) = (style.hover_fill, style.pressed);
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
        .w_full()
        .flex()
        .items_center()
        .gap(style.control_gap)
        .h(style.control_height)
        .px(style.control_gap)
        .rounded(style.radius)
        .cursor_pointer()
        .when(highlighted, |option| option.bg(style.selected))
        .when(!highlighted, |option| option.hover(move |o| o.bg(hover)))
        .active(move |option| option.bg(pressed))
        .child(check)
        .child(div().flex_1().min_w_0().truncate().child(label))
}

/// A control that can't be used right now, such as one whose switch is
/// off: drawn as it is under a cover that takes the pointer, so it shows
/// no hover and ignores clicks.
pub fn inert(control: AnyElement) -> AnyElement {
    div()
        .relative()
        .child(control)
        .child(div().absolute().inset_0().occlude())
        .into_any_element()
}

/// The keys a control's tooltip names, such as `Alt+Up` for moving an
/// item up, as this platform writes them.
pub fn tooltip_keys(chord: &str) -> Option<Shortcut> {
    let platform = gasp_config::Platform::current();
    gasp_config::keys::KeyChord::parse_for(chord, platform)
        .ok()
        .map(|chord| Shortcut::new(chord, platform))
}
