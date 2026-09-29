//! Drawing the Toolbars page's rows and the keys they take.
//!
//! A toolbar's header has its switch (and, for one the vault added, a
//! remove button that asks once more). Place and "When it shows" are
//! dropdowns; "Buttons show" and "Size" are segmented. Each item has a
//! handle to drag it by, its icon, name and shortcut, and buttons to move
//! it up or down or take it off. The keys: Space or Enter presses, Left
//! and Right change a choice, Alt with Up or Down moves an item, Delete
//! takes it off.

use gasp_config::toolbars::{ToolbarContext, ToolbarItem, choice_name};
use gpui::{
    AnyElement, ClickEvent, Context, Div, MouseButton, MouseDownEvent, SharedString, Stateful,
    Window, div, prelude::*,
};

use super::controls::{
    button, choice_button, control_note, icon_button, icon_label_button, row_text, segment,
    segmented, toggle_switch, two_column_row,
};
use super::model::PageSpec;
use super::toolbars_page::{
    TOOLBARS_KEY, ToolbarField, item_icon, toolbar_choice_label, toolbar_error_key,
};
use super::view::{ControlRow, SettingsFocus, SettingsView};
use crate::icons::{IconName, icon};
use crate::ui::{Tooltip, keycap};

/// An item being dragged to a new place, on its toolbar or another.
#[derive(Clone, Debug)]
pub struct DraggedToolbarItem {
    pub toolbar: String,
    pub index: usize,
    pub label: SharedString,
}

/// What follows the pointer while an item is dragged.
struct DraggedItemPreview {
    label: SharedString,
}

impl Render for DraggedItemPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = crate::ui::ui_theme(cx);
        div()
            .px(ui.space_md)
            .py(ui.space_xs)
            .rounded(ui.row_radius)
            .bg(ui.menu_background)
            .shadow(ui.menu_shadows())
            .text_size(ui.small_font_size)
            .text_color(ui.text)
            .child(self.label.clone())
    }
}

impl ControlRow {
    /// Whether the row is on the Toolbars page.
    pub fn is_toolbar_row(&self) -> bool {
        matches!(
            self,
            ControlRow::ToolbarHeader(_)
                | ControlRow::ToolbarField { .. }
                | ControlRow::ToolbarContexts(_)
                | ControlRow::ToolbarItem { .. }
                | ControlRow::ToolbarAdd(_)
                | ControlRow::NewToolbar
                | ControlRow::ResetToolbars
        )
    }

    /// A Toolbars page row's title without the view, for search: the
    /// toolbar's id, or what the row does. The page shows the titles
    /// [`SettingsView::toolbar_row_title`] gives.
    pub(super) fn toolbar_placeholder_title(&self) -> String {
        match self {
            ControlRow::NewToolbar => "Add a toolbar".to_owned(),
            ControlRow::ResetToolbars => "Reset toolbars".to_owned(),
            _ => self.toolbar_id().unwrap_or_default().to_owned(),
        }
    }

    /// The toolbar a row belongs to, for its errors.
    fn toolbar_id(&self) -> Option<&str> {
        match self {
            ControlRow::ToolbarHeader(id)
            | ControlRow::ToolbarContexts(id)
            | ControlRow::ToolbarAdd(id)
            | ControlRow::ToolbarField { toolbar: id, .. }
            | ControlRow::ToolbarItem { toolbar: id, .. } => Some(id),
            _ => None,
        }
    }
}

impl SettingsView {
    /// The key a Toolbars page row's write errors are kept under.
    pub(super) fn toolbar_row_error_key(&self, row: &ControlRow) -> String {
        row.toolbar_id()
            .map_or_else(|| TOOLBARS_KEY.to_owned(), toolbar_error_key)
    }

    /// A Toolbars page row's text column: an item shows its icon and, for
    /// a command, its shortcut.
    pub(super) fn toolbar_row_text(&self, row: &ControlRow) -> AnyElement {
        let description = self.toolbar_row_description(row);
        let description =
            (!description.is_empty()).then(|| div().child(description).into_any_element());
        let text = row_text(
            self.toolbar_row_title(row),
            description,
            Vec::new(),
            &self.style,
        );
        let ControlRow::ToolbarItem { toolbar, index } = row else {
            return text.into_any_element();
        };
        let Some(item) = self
            .toolbar(toolbar)
            .and_then(|t| t.items.get(*index))
            .cloned()
        else {
            return text.into_any_element();
        };
        let style = &self.style;
        div()
            .flex()
            .items_center()
            .gap(style.control_gap)
            .child(
                icon(IconName::DotsSixVertical)
                    .flex_none()
                    .size(style.small_icon_size)
                    .text_color(style.text_faint),
            )
            .child(
                icon(item_icon(&item, &self.toolbars))
                    .flex_none()
                    .size(style.icon_size)
                    .text_color(style.text_muted),
            )
            .child(text.flex_1().min_w_0())
            .into_any_element()
    }

    /// A Toolbars page row's control.
    pub(super) fn toolbar_control(
        &self,
        index: usize,
        row: &ControlRow,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match row {
            ControlRow::ToolbarHeader(id) => self.toolbar_header_control(id, focused, cx),
            ControlRow::ToolbarField { toolbar, field } => {
                self.toolbar_field_control(index, toolbar, *field, focused, cx)
            }
            ControlRow::ToolbarContexts(id) => self.toolbar_contexts_control(id, focused, cx),
            ControlRow::ToolbarItem {
                toolbar,
                index: item,
            } => self.toolbar_item_control(toolbar, *item, focused, cx),
            ControlRow::ToolbarAdd(id) => self.toolbar_add_control(index, id, focused, cx),
            ControlRow::NewToolbar => self.new_toolbar_control(focused, cx),
            ControlRow::ResetToolbars => self.reset_toolbars_control(focused, cx),
            _ => div().into_any_element(),
        }
    }

    fn toolbar_header_control(
        &self,
        id: &str,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(toolbar) = self.toolbar(id).cloned() else {
            return div().into_any_element();
        };
        let reset = self.toolbar_changed(id).then(|| {
            let id = id.to_owned();
            self.reset_button(&toolbar_error_key(&toolbar.id), cx, move |view, cx| {
                view.reset_toolbar(&id, cx)
            })
        });
        let remove = (!toolbar.built_in).then(|| self.remove_toolbar_button(id, cx));
        let target = id.to_owned();
        let selector = format!("toolbar-switch-{id}");
        let switch = toggle_switch(
            SharedString::from(selector.clone()),
            toolbar.enabled,
            focused,
            &self.style,
        )
        .debug_selector(move || selector)
        .tooltip(
            Tooltip::new(
                if toolbar.enabled {
                    "Turn off"
                } else {
                    "Turn on"
                },
                None,
            )
            .builder(),
        )
        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| view.toggle_toolbar(&target, cx)));
        div()
            .flex()
            .items_center()
            .gap(self.style.gap_sm)
            .children(reset)
            .children(remove)
            .child(switch)
            .into_any_element()
    }

    /// Removing a toolbar asks once more: the first press arms the button,
    /// which then says what the next press does.
    fn remove_toolbar_button(&self, id: &str, cx: &mut Context<Self>) -> AnyElement {
        let armed = self.armed_row.as_ref() == Some(&ControlRow::ToolbarHeader(id.to_owned()));
        let selector = format!("remove-toolbar-{id}");
        let target = id.to_owned();
        let click = cx.listener(move |view, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            view.press_remove_toolbar(&target, cx);
        });
        if armed {
            return button(
                SharedString::from(selector.clone()),
                "Remove toolbar",
                false,
                false,
                &self.style,
            )
            .debug_selector(move || selector)
            .text_color(self.style.warning)
            .on_click(click)
            .into_any_element();
        }
        icon_button(
            SharedString::from(selector.clone()),
            IconName::Trash,
            self.style.text_muted,
            &self.style,
        )
        .debug_selector(move || selector)
        .tooltip(Tooltip::new("Remove this toolbar", None).builder())
        .on_click(click)
        .into_any_element()
    }

    pub(super) fn press_remove_toolbar(&mut self, id: &str, cx: &mut Context<Self>) {
        let row = ControlRow::ToolbarHeader(id.to_owned());
        if self.armed_row.as_ref() == Some(&row) {
            self.armed_row = None;
            self.remove_toolbar(id, cx);
        } else {
            self.armed_row = Some(row);
            cx.notify();
        }
    }

    fn toolbar_field_control(
        &self,
        index: usize,
        id: &str,
        field: ToolbarField,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(toolbar) = self.toolbar(id) else {
            return div().into_any_element();
        };
        let current = field.value(toolbar);
        if field.is_dropdown() {
            let label = div()
                .child(toolbar_choice_label(&current))
                .into_any_element();
            let dropdown_id = format!("toolbar-{}-{id}", field.key());
            return self.dropdown(index, dropdown_id, label, focused, cx);
        }
        let segments: Vec<Stateful<Div>> = field
            .options()
            .into_iter()
            .map(|option| {
                let chosen = option == current;
                let selector = format!("toolbar-{}-{id}-{option}", field.key());
                let (target, value) = (id.to_owned(), option.clone());
                segment(
                    SharedString::from(selector.clone()),
                    toolbar_choice_label(&option),
                    chosen,
                    &self.style,
                )
                .debug_selector(move || selector)
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    view.choose_toolbar_field(&target, field, &value, cx)
                }))
            })
            .collect();
        let id = format!("toolbar-{}-{id}", field.key());
        segmented(SharedString::from(id), segments, focused, &self.style).into_any_element()
    }

    fn toolbar_contexts_control(
        &self,
        id: &str,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let on = self
            .toolbar(id)
            .map(|t| t.contexts.clone())
            .unwrap_or_default();
        let chips: Vec<Stateful<Div>> = ToolbarContext::ALL
            .into_iter()
            .enumerate()
            .map(|(position, context)| {
                let name = choice_name(context);
                let selector = format!("toolbar-context-{id}-{name}");
                let target = id.to_owned();
                let ringed = focused && position == self.context_chip;
                choice_button(
                    SharedString::from(selector.clone()),
                    toolbar_choice_label(&name),
                    on.contains(&context),
                    &self.style,
                )
                .debug_selector(move || selector)
                .when(ringed, |chip| chip.shadow(vec![self.style.focus()]))
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    view.toggle_toolbar_context(&target, context, cx)
                }))
            })
            .collect();
        div()
            .flex()
            .flex_wrap()
            .justify_end()
            .gap(self.style.gap_sm)
            .children(chips)
            .into_any_element()
    }

    fn toolbar_item_control(
        &self,
        id: &str,
        index: usize,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = self.toolbar(id).map_or(0, |toolbar| toolbar.items.len());
        let shortcut = match self.toolbar(id).and_then(|t| t.items.get(index)) {
            Some(ToolbarItem::Command(command)) => crate::ui::hints::shortcut(command, cx),
            _ => None,
        };
        let keycaps = self
            .keycaps
            .clone()
            .compact()
            .on_text(self.style.text_muted);
        let step = |name: IconName, delta: isize, label: &str, enabled: bool| {
            let selector = format!("toolbar-item-{id}-{index}-{label}");
            let target = id.to_owned();
            let color = if enabled {
                self.style.text_muted
            } else {
                self.style.text_faint
            };
            icon_button(
                SharedString::from(selector.clone()),
                name,
                color,
                &self.style,
            )
            .debug_selector(move || selector)
            .tooltip(Tooltip::new(label.to_owned(), None).builder())
            .when(enabled, |button| {
                button.on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.move_toolbar_item(&target, index, delta, cx)
                }))
            })
        };
        let up = step(IconName::ArrowUp, -1, "Move up", index > 0);
        let down = step(IconName::ArrowDown, 1, "Move down", index + 1 < count);
        let selector = format!("toolbar-item-{id}-{index}-remove");
        let target = id.to_owned();
        let remove = icon_button(
            SharedString::from(selector.clone()),
            IconName::X,
            self.style.text_muted,
            &self.style,
        )
        .debug_selector(move || selector)
        .tooltip(Tooltip::new("Take off the toolbar", None).builder())
        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            view.remove_toolbar_item(&target, index, cx)
        }));
        div()
            .flex()
            .items_center()
            .gap(self.style.gap_xs)
            .rounded(self.style.radius)
            .when(focused, |group| group.shadow(vec![self.style.focus()]))
            .children(shortcut.map(|shortcut| keycap(shortcut, &keycaps).mr(self.style.gap_sm)))
            .child(up)
            .child(down)
            .child(remove)
            .into_any_element()
    }

    fn toolbar_add_control(
        &self,
        index: usize,
        id: &str,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = div()
            .flex()
            .items_center()
            .gap(self.style.gap_sm)
            .child(
                icon(IconName::Plus)
                    .size(self.style.small_icon_size)
                    .text_color(self.style.text_muted),
            )
            .child("Choose")
            .into_any_element();
        self.dropdown(index, format!("toolbar-add-{id}"), label, focused, cx)
    }

    fn new_toolbar_control(&self, focused: bool, cx: &mut Context<Self>) -> AnyElement {
        icon_label_button(
            "new-toolbar",
            IconName::Plus,
            "Add a toolbar",
            false,
            focused,
            &self.style,
        )
        .debug_selector(|| "new-toolbar".to_owned())
        .on_click(cx.listener(|view, _: &ClickEvent, window, cx| view.new_toolbar(window, cx)))
        .into_any_element()
    }

    fn reset_toolbars_control(&self, focused: bool, cx: &mut Context<Self>) -> AnyElement {
        let armed = self.armed_row.as_ref() == Some(&ControlRow::ResetToolbars);
        let label = if armed {
            "Press again to reset"
        } else {
            "Reset toolbars"
        };
        button("reset-toolbars", label, false, focused, &self.style)
            .debug_selector(|| "reset-toolbars".to_owned())
            .when(armed, |button| button.text_color(self.style.warning))
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| view.press_reset_toolbars(cx)))
            .into_any_element()
    }

    pub(super) fn press_reset_toolbars(&mut self, cx: &mut Context<Self>) {
        if self.armed_row.as_ref() == Some(&ControlRow::ResetToolbars) {
            self.armed_row = None;
            self.reset_toolbars(cx);
        } else {
            self.armed_row = Some(ControlRow::ResetToolbars);
            cx.notify();
        }
    }

    /// An item's row: the usual two columns, dragged by anywhere on it to
    /// a new place, on its toolbar or another.
    pub(super) fn render_toolbar_item_row(
        &mut self,
        index: usize,
        row: &ControlRow,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ControlRow::ToolbarItem {
            toolbar,
            index: item,
        } = row.clone()
        else {
            return div().into_any_element();
        };
        let page = self
            .current_section()
            .map_or("", |page| PageSpec::get(page).id);
        let text = self.toolbar_row_text(row);
        let control = self.toolbar_control(index, row, focused, cx);
        let note = self
            .row_error(row)
            .map(|message| control_note(message, &self.style));
        let dragged = DraggedToolbarItem {
            toolbar: toolbar.clone(),
            index: item,
            label: self.toolbar_row_title(row).into(),
        };
        let drop_fill = self.style.hover;
        let (into, at) = (toolbar.clone(), item);
        div()
            .id(SharedString::from(format!(
                "toolbar-item-row-{toolbar}-{item}"
            )))
            .relative()
            .rounded(self.style.radius)
            .child(two_column_row(
                &format!("{page}-{index}"),
                text,
                Some(control),
                &self.style,
            ))
            .children(note)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                    if view.focus != SettingsFocus::Control(index) {
                        view.set_focus(SettingsFocus::Control(index), window, cx);
                    }
                }),
            )
            .on_drag(dragged, |dragged, _, _, cx| {
                let label = dragged.label.clone();
                cx.new(|_| DraggedItemPreview { label })
            })
            .drag_over::<DraggedToolbarItem>(move |style, _, _, _| style.bg(drop_fill))
            .on_drop(
                cx.listener(move |view, dragged: &DraggedToolbarItem, _, cx| {
                    view.drop_toolbar_item((&dragged.toolbar, dragged.index), (&into, at), cx);
                }),
            )
            .into_any_element()
    }

    /// A picker option: its icon, its name, and a command's shortcut.
    pub(super) fn picker_row_label(&self, option: &str, label: Div, cx: &mut Context<Self>) -> Div {
        let item = ToolbarItem::parse(option);
        let shortcut = match &item {
            ToolbarItem::Command(id) => crate::ui::hints::shortcut(id, cx),
            _ => None,
        };
        let keycaps = self
            .keycaps
            .clone()
            .compact()
            .on_text(self.style.text_muted);
        div()
            .flex()
            .items_center()
            .gap(self.style.control_gap)
            .child(
                icon(item_icon(&item, &self.toolbars))
                    .flex_none()
                    .size(self.style.small_icon_size)
                    .text_color(self.style.text_muted),
            )
            .child(label.flex_1().min_w_0().truncate())
            .children(shortcut.map(|shortcut| keycap(shortcut, &keycaps)))
    }

    // ---- Keys ----

    /// Keys on a Toolbars page row. Returns whether the key was used.
    pub(super) fn toolbar_key(
        &mut self,
        index: usize,
        row: &ControlRow,
        keystroke: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = keystroke.key.as_str();
        match row {
            ControlRow::ToolbarHeader(id) => self.toolbar_header_key(id, key, cx),
            ControlRow::ToolbarField { toolbar, field } => {
                self.toolbar_field_key(index, toolbar, *field, key, window, cx)
            }
            ControlRow::ToolbarContexts(id) => self.toolbar_contexts_key(id, key, cx),
            ControlRow::ToolbarItem {
                toolbar,
                index: item,
            } => self.toolbar_item_key(toolbar, *item, keystroke, cx),
            ControlRow::ToolbarAdd(_) if matches!(key, "enter" | "space") => {
                self.open_menu(index, window, cx);
                true
            }
            ControlRow::NewToolbar if matches!(key, "enter" | "space") => {
                self.new_toolbar(window, cx);
                true
            }
            ControlRow::ResetToolbars if matches!(key, "enter" | "space") => {
                self.press_reset_toolbars(cx);
                true
            }
            _ => false,
        }
    }

    fn toolbar_header_key(&mut self, id: &str, key: &str, cx: &mut Context<Self>) -> bool {
        let on = self.toolbar(id).is_some_and(|toolbar| toolbar.enabled);
        let built_in = self.toolbar(id).is_some_and(|toolbar| toolbar.built_in);
        match key {
            "space" | "enter" => self.toggle_toolbar(id, cx),
            "left" if on => self.toggle_toolbar(id, cx),
            "right" if !on => self.toggle_toolbar(id, cx),
            "delete" | "backspace" if !built_in => self.press_remove_toolbar(id, cx),
            "delete" | "backspace" => self.reset_toolbar(id, cx),
            _ => return false,
        }
        true
    }

    fn toolbar_field_key(
        &mut self,
        index: usize,
        id: &str,
        field: ToolbarField,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match key {
            "left" => self.step_toolbar_field(id, field, -1, cx),
            "right" => self.step_toolbar_field(id, field, 1, cx),
            "space" | "enter" if field.is_dropdown() => self.open_menu(index, window, cx),
            "space" | "enter" => self.step_toolbar_field(id, field, 1, cx),
            _ => return false,
        }
        true
    }

    fn toolbar_contexts_key(&mut self, id: &str, key: &str, cx: &mut Context<Self>) -> bool {
        let last = ToolbarContext::ALL.len() - 1;
        match key {
            "left" => self.context_chip = self.context_chip.saturating_sub(1),
            "right" => self.context_chip = (self.context_chip + 1).min(last),
            "space" | "enter" => {
                let context = ToolbarContext::ALL[self.context_chip.min(last)];
                self.toggle_toolbar_context(id, context, cx);
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    fn toolbar_item_key(
        &mut self,
        id: &str,
        index: usize,
        keystroke: &gpui::Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        let moving = keystroke.modifiers.alt;
        match keystroke.key.as_str() {
            "up" if moving => self.move_toolbar_item(id, index, -1, cx),
            "down" if moving => self.move_toolbar_item(id, index, 1, cx),
            "delete" | "backspace" => self.remove_toolbar_item(id, index, cx),
            _ => return false,
        }
        true
    }
}
