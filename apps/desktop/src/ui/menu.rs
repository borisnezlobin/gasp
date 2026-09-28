//! Dropdown and context menus: a list of commands with icons, shortcut
//! hints, separators and submenus. Up and Down move, Enter runs, Right
//! opens a submenu, Left closes it and Escape closes the menu.
//!
//! A view that shows menus owns a [`MenuSlot`]. The slot opens one menu at
//! a time, draws a backdrop that closes it on a click outside, and gives
//! focus back to where it was before the chosen item runs, so editor
//! commands reach the editor.

use std::rc::Rc;

use gpui::{
    AnyElement, App, AppContext, Context, Corner, DismissEvent, Div, ElementId, Entity,
    EventEmitter, FocusHandle, Focusable, KeyDownEvent, MouseButton, Pixels, Point, SharedString,
    Stateful, Subscription, WeakEntity, Window, anchored, deferred, div, point, prelude::*, px,
};

use super::{keycap, ui_theme};
use crate::icons::{IconName, icon};
use crate::keymap::RunCommand;
use crate::picker::shortcut::Shortcut;
use crate::theme::UiTheme;

/// What choosing an item does. It runs after the menu has closed and focus
/// is back where it was.
pub type MenuHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// One row of a menu.
#[derive(Clone)]
pub enum MenuItem {
    Entry(MenuEntry),
    Separator,
    Submenu {
        label: SharedString,
        icon: Option<IconName>,
        items: Vec<MenuItem>,
    },
}

/// A row that does something.
#[derive(Clone)]
pub struct MenuEntry {
    pub label: SharedString,
    pub icon: Option<IconName>,
    pub shortcut: Option<Shortcut>,
    pub checked: bool,
    pub disabled: bool,
    handler: MenuHandler,
}

impl MenuItem {
    /// Runs command `id`, showing its title and shortcut.
    pub fn command(id: &str, cx: &App) -> MenuItem {
        let command: SharedString = id.to_owned().into();
        MenuItem::Entry(MenuEntry {
            label: super::hints::command_title(id),
            icon: None,
            shortcut: super::hints::shortcut(id, cx),
            checked: false,
            disabled: false,
            handler: Rc::new(move |window, cx| {
                window.dispatch_action(
                    Box::new(RunCommand {
                        id: command.clone(),
                    }),
                    cx,
                )
            }),
        })
    }

    pub fn action(
        label: impl Into<SharedString>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> MenuItem {
        MenuItem::Entry(MenuEntry {
            label: label.into(),
            icon: None,
            shortcut: None,
            checked: false,
            disabled: false,
            handler: Rc::new(handler),
        })
    }

    pub fn submenu(label: impl Into<SharedString>, items: Vec<MenuItem>) -> MenuItem {
        MenuItem::Submenu {
            label: label.into(),
            icon: None,
            items,
        }
    }

    pub fn label(&self) -> Option<&SharedString> {
        match self {
            MenuItem::Entry(entry) => Some(&entry.label),
            MenuItem::Submenu { label, .. } => Some(label),
            MenuItem::Separator => None,
        }
    }

    pub fn with_label(mut self, text: impl Into<SharedString>) -> MenuItem {
        match &mut self {
            MenuItem::Entry(entry) => entry.label = text.into(),
            MenuItem::Submenu { label, .. } => *label = text.into(),
            MenuItem::Separator => {}
        }
        self
    }

    pub fn with_icon(mut self, name: IconName) -> MenuItem {
        match &mut self {
            MenuItem::Entry(entry) => entry.icon = Some(name),
            MenuItem::Submenu { icon, .. } => *icon = Some(name),
            MenuItem::Separator => {}
        }
        self
    }

    pub fn with_shortcut(mut self, shortcut: Option<Shortcut>) -> MenuItem {
        if let MenuItem::Entry(entry) = &mut self {
            entry.shortcut = shortcut;
        }
        self
    }

    /// Does `handler` in place of what the item did, keeping its label.
    pub fn with_handler(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> MenuItem {
        if let MenuItem::Entry(entry) = &mut self {
            entry.handler = Rc::new(handler);
        }
        self
    }

    /// Marks the item that's current, such as the sort order in use.
    pub fn checked(mut self, checked: bool) -> MenuItem {
        if let MenuItem::Entry(entry) = &mut self {
            entry.checked = checked;
        }
        self
    }

    pub fn disabled(mut self, disabled: bool) -> MenuItem {
        if let MenuItem::Entry(entry) = &mut self {
            entry.disabled = disabled;
        }
        self
    }

    fn is_selectable(&self) -> bool {
        match self {
            MenuItem::Entry(entry) => !entry.disabled,
            MenuItem::Submenu { .. } => true,
            MenuItem::Separator => false,
        }
    }

    fn leading_icon(&self) -> Option<IconName> {
        match self {
            MenuItem::Entry(entry) if entry.checked => Some(IconName::Check),
            MenuItem::Entry(entry) => entry.icon,
            MenuItem::Submenu { icon, .. } => *icon,
            MenuItem::Separator => None,
        }
    }
}

struct OpenSubmenu {
    index: usize,
    menu: Entity<DropdownMenu>,
}

/// A menu. It emits `DismissEvent` when it should close, after an item
/// was chosen or on Escape.
pub struct DropdownMenu {
    focus_handle: FocusHandle,
    items: Vec<MenuItem>,
    highlighted: Option<usize>,
    submenu: Option<OpenSubmenu>,
    parent: Option<WeakEntity<DropdownMenu>>,
}

impl EventEmitter<DismissEvent> for DropdownMenu {}

impl Focusable for DropdownMenu {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl DropdownMenu {
    pub fn new(items: Vec<MenuItem>, cx: &mut Context<Self>) -> Self {
        DropdownMenu {
            focus_handle: cx.focus_handle(),
            items,
            highlighted: None,
            submenu: None,
            parent: None,
        }
    }

    pub fn items(&self) -> &[MenuItem] {
        &self.items
    }

    /// Item labels in order, with `-` for separators.
    pub fn labels(&self) -> Vec<String> {
        self.items
            .iter()
            .map(|item| {
                item.label()
                    .map_or_else(|| "-".to_owned(), ToString::to_string)
            })
            .collect()
    }

    pub fn highlighted(&self) -> Option<usize> {
        self.highlighted
    }

    /// The open submenu, if any.
    pub fn submenu(&self) -> Option<Entity<DropdownMenu>> {
        self.submenu.as_ref().map(|open| open.menu.clone())
    }

    /// Chooses the item labelled `label`, as a click would.
    pub fn choose(&mut self, label: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let found = self
            .items
            .iter()
            .position(|item| item.label().is_some_and(|text| text == label));
        match found {
            Some(index) => {
                self.confirm(index, window, cx);
                true
            }
            None => false,
        }
    }

    /// Moves the highlight to the next selectable item in `step`'s
    /// direction, wrapping around.
    fn move_highlight(&mut self, step: isize, cx: &mut Context<Self>) {
        let count = self.items.len() as isize;
        if count == 0 {
            return;
        }
        let mut index = match self.highlighted {
            Some(index) => index as isize,
            None if step > 0 => -1,
            None => count,
        };
        for _ in 0..count {
            index = (index + step).rem_euclid(count);
            if self.items[index as usize].is_selectable() {
                self.highlighted = Some(index as usize);
                cx.notify();
                return;
            }
        }
    }

    /// Runs the item at `index`, or opens it when it's a submenu.
    pub fn confirm(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        match self.items.get(index) {
            Some(MenuItem::Entry(entry)) if !entry.disabled => {
                let handler = entry.handler.clone();
                // The whole menu closes, and focus goes back, before the
                // item runs.
                self.dismiss(cx);
                window.defer(cx, move |window, cx| handler(window, cx));
            }
            Some(MenuItem::Submenu { .. }) => self.open_submenu(index, true, window, cx),
            _ => {}
        }
    }

    fn open_submenu(
        &mut self,
        index: usize,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(MenuItem::Submenu { items, .. }) = self.items.get(index) else {
            return;
        };
        self.highlighted = Some(index);
        let already_open = self
            .submenu
            .as_ref()
            .is_some_and(|open| open.index == index);
        if !already_open {
            let items = items.clone();
            let parent = cx.entity().downgrade();
            let menu = cx.new(|cx| DropdownMenu {
                parent: Some(parent),
                ..DropdownMenu::new(items, cx)
            });
            self.submenu = Some(OpenSubmenu { index, menu });
        }
        if let (true, Some(open)) = (focus, self.submenu.as_ref()) {
            let menu = open.menu.clone();
            menu.update(cx, |menu, cx| menu.move_highlight(1, cx));
            window.focus(&menu.focus_handle(cx));
        }
        cx.notify();
    }

    /// Closes the menu this one belongs to.
    fn dismiss(&mut self, cx: &mut Context<Self>) {
        match self.parent.as_ref().and_then(WeakEntity::upgrade) {
            Some(parent) => parent.update(cx, |parent, cx| parent.dismiss(cx)),
            None => cx.emit(DismissEvent),
        }
    }

    fn close_submenu(&mut self, cx: &mut Context<Self>) {
        if self.submenu.take().is_some() {
            cx.notify();
        }
    }

    /// Escape or Left in a submenu: back to the parent menu.
    fn back_to_parent(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(parent) = self.parent.as_ref().and_then(WeakEntity::upgrade) else {
            return false;
        };
        let focus = parent.read(cx).focus_handle.clone();
        parent.update(cx, |parent, cx| parent.close_submenu(cx));
        window.focus(&focus);
        true
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let handled = match event.keystroke.key.as_str() {
            "up" => {
                self.move_highlight(-1, cx);
                true
            }
            "down" => {
                self.move_highlight(1, cx);
                true
            }
            "enter" | "right" => self.key_confirm(event.keystroke.key == "right", window, cx),
            "left" => self.back_to_parent(window, cx),
            "escape" => {
                if !self.back_to_parent(window, cx) {
                    cx.emit(DismissEvent);
                }
                true
            }
            _ => false,
        };
        if handled {
            cx.stop_propagation();
        }
    }

    /// Enter runs the highlighted item; Right only opens a submenu.
    fn key_confirm(
        &mut self,
        submenu_only: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(index) = self.highlighted else {
            return !submenu_only;
        };
        let is_submenu = matches!(self.items.get(index), Some(MenuItem::Submenu { .. }));
        if submenu_only && !is_submenu {
            return false;
        }
        self.confirm(index, window, cx);
        true
    }

    fn on_row_hover(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.items[index].is_selectable() {
            return;
        }
        self.highlighted = Some(index);
        if matches!(self.items[index], MenuItem::Submenu { .. }) {
            self.open_submenu(index, false, window, cx);
        } else {
            self.close_submenu(cx);
        }
        cx.notify();
    }

    fn render_row(
        &self,
        index: usize,
        has_icons: bool,
        theme: &UiTheme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let item = &self.items[index];
        let Some(label) = item.label().cloned() else {
            return separator(theme);
        };
        let disabled = !item.is_selectable();
        let leading = has_icons.then(|| menu_icon(item.leading_icon(), disabled, theme));
        let selector = format!("menu-item-{label}");
        menu_row(
            ElementId::NamedInteger("menu-item".into(), index as u64),
            self.highlighted == Some(index),
            disabled,
            theme,
        )
        .debug_selector(|| selector)
        .relative()
        .on_hover(cx.listener(move |menu, hovered: &bool, window, cx| {
            if *hovered {
                menu.on_row_hover(index, window, cx);
            }
        }))
        .on_click(cx.listener(move |menu, _, window, cx| {
            cx.stop_propagation();
            menu.confirm(index, window, cx);
        }))
        .children(leading)
        .child(super::truncated(label).grow())
        .child(self.render_row_end(index, theme))
        .into_any_element()
    }

    /// A shortcut hint, or the caret and the open submenu.
    fn render_row_end(&self, index: usize, theme: &UiTheme) -> AnyElement {
        match &self.items[index] {
            MenuItem::Entry(entry) => {
                // A disabled entry's keys fade with its label.
                let text = if entry.disabled {
                    theme.text_faint
                } else {
                    theme.text_muted
                };
                let keys = theme.keycap.clone().compact().on_text(text);
                div()
                    .flex_none()
                    .pl(theme.space_lg)
                    .children(entry.shortcut.map(|shortcut| keycap(shortcut, &keys)))
                    .into_any_element()
            }
            MenuItem::Submenu { .. } => {
                let open = self
                    .submenu
                    .as_ref()
                    .filter(|open| open.index == index)
                    .map(|open| {
                        // Slid up or left to stay inside the window when
                        // it opens near an edge.
                        div()
                            .absolute()
                            .top(-theme.menu_padding)
                            .left_full()
                            .pl(theme.space_xs)
                            .child(anchored().snap_to_window().child(open.menu.clone()))
                    });
                div()
                    .flex_none()
                    .child(
                        icon(IconName::CaretRight)
                            .size(theme.small_icon_size)
                            .text_color(theme.icon),
                    )
                    .children(open)
                    .into_any_element()
            }
            MenuItem::Separator => div().into_any_element(),
        }
    }
}

/// A menu row: a fixed height, the highlight, and faint text when it
/// can't be chosen. Every menu in the app, the file tree's included, is
/// made of these inside a [`super::popover`].
pub fn menu_row(
    id: impl Into<ElementId>,
    highlighted: bool,
    disabled: bool,
    theme: &UiTheme,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_md)
        .h(theme.menu_row_height)
        .px(theme.menu_row_padding_x)
        .rounded(theme.menu_row_radius)
        .when(highlighted, |row| row.bg(theme.menu_highlight))
        .when(disabled, |row| row.text_color(theme.text_faint))
}

/// The icon column of a menu row. It keeps its width when the row has no
/// icon, so labels line up, and fades with a disabled row's label.
pub fn menu_icon(name: Option<IconName>, disabled: bool, theme: &UiTheme) -> Div {
    let color = if disabled {
        theme.icon_disabled
    } else {
        theme.icon
    };
    div()
        .flex_none()
        .size(theme.small_icon_size)
        .children(name.map(|name| icon(name).size(theme.small_icon_size).text_color(color)))
}

fn separator(theme: &UiTheme) -> AnyElement {
    div()
        .my(theme.menu_padding)
        .mx(theme.menu_row_padding_x)
        .h(theme.hairline)
        .bg(theme.menu_separator)
        .into_any_element()
}

/// How tall a menu of `items` is with every row showing.
fn menu_height(items: &[MenuItem], theme: &UiTheme) -> Pixels {
    let separator = theme.menu_padding * 2. + theme.hairline;
    let rows = items.iter().fold(Pixels::ZERO, |height, item| {
        height
            + match item {
                MenuItem::Separator => separator,
                _ => theme.menu_row_height,
            }
    });
    rows + theme.menu_padding * 2.
}

impl Render for DropdownMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ui_theme(cx);
        let has_icons = self.items.iter().any(|item| item.leading_icon().is_some());
        let rows: Vec<AnyElement> = (0..self.items.len())
            .map(|index| self.render_row(index, has_icons, &theme, cx))
            .collect();
        // A menu taller than the window keeps inside it and scrolls. Only
        // then, since scrolling clips what hangs outside it, a submenu
        // included.
        let room = window.viewport_size().height - theme.space_md * 2.;
        let capped = menu_height(&self.items, &theme) > room;
        super::popover(&theme)
            .id("dropdown-menu")
            .when(capped, |menu| menu.max_h(room).overflow_y_scroll())
            .key_context("Menu")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .occlude()
            .min_w(theme.menu_min_width)
            .max_w(theme.menu_max_width)
            .children(rows)
    }
}

/// Where a menu opens.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuAnchor {
    /// At the pointer, as a context menu does.
    Pointer(Point<Pixels>),
    /// Under the control called `key`, whose owner draws it there with
    /// [`MenuSlot::render_attached`]. With `align_right` the menu's right
    /// edge lines up with the control's.
    Below {
        key: SharedString,
        align_right: bool,
    },
    /// Over the control called `key`, for controls at the window's bottom.
    Above { key: SharedString },
}

impl MenuAnchor {
    fn key(&self) -> Option<&SharedString> {
        match self {
            MenuAnchor::Pointer(_) => None,
            MenuAnchor::Below { key, .. } | MenuAnchor::Above { key } => Some(key),
        }
    }
}

/// A view that owns a [`MenuSlot`].
pub trait HasMenuSlot: 'static + Sized {
    fn menu_slot(&mut self) -> &mut MenuSlot;
}

struct OpenMenu {
    menu: Entity<DropdownMenu>,
    anchor: MenuAnchor,
    previous_focus: Option<FocusHandle>,
    _dismiss: Subscription,
}

/// The one open menu of a view.
#[derive(Default)]
pub struct MenuSlot {
    open: Option<OpenMenu>,
}

impl MenuSlot {
    /// Opens a menu of `items` at `anchor`, closing any other, and gives
    /// it the keyboard.
    pub fn open<V: HasMenuSlot>(
        &mut self,
        items: Vec<MenuItem>,
        anchor: MenuAnchor,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Entity<DropdownMenu> {
        let previous_focus = self.close(window).or_else(|| window.focused(cx));
        let menu = cx.new(|cx| DropdownMenu::new(items, cx));
        let dismiss = cx.subscribe_in(
            &menu,
            window,
            |view: &mut V, _, _: &DismissEvent, window, cx| {
                view.menu_slot().close(window);
                cx.notify();
            },
        );
        window.focus(&menu.focus_handle(cx));
        self.open = Some(OpenMenu {
            menu: menu.clone(),
            anchor,
            previous_focus,
            _dismiss: dismiss,
        });
        cx.notify();
        menu
    }

    /// Closes the menu and gives focus back. Returns where focus went.
    pub fn close(&mut self, window: &mut Window) -> Option<FocusHandle> {
        let open = self.open.take()?;
        if let Some(previous) = &open.previous_focus {
            window.focus(previous);
        }
        open.previous_focus
    }

    pub fn menu(&self) -> Option<Entity<DropdownMenu>> {
        self.open.as_ref().map(|open| open.menu.clone())
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Whether the open menu hangs under the control called `key`.
    pub fn is_open_at(&self, key: &str) -> bool {
        self.open
            .as_ref()
            .and_then(|open| open.anchor.key())
            .is_some_and(|open_key| open_key == key)
    }

    /// The backdrop that closes the menu on a click outside it, and the
    /// menu itself when it opened at the pointer. Draw it anywhere in the
    /// view.
    pub fn render_overlay<V: HasMenuSlot>(
        &self,
        window: &Window,
        cx: &mut Context<V>,
    ) -> Option<AnyElement> {
        let open = self.open.as_ref()?;
        let viewport = window.viewport_size();
        let close = || {
            cx.listener(|view: &mut V, _: &gpui::MouseDownEvent, window, cx| {
                view.menu_slot().close(window);
                cx.notify();
            })
        };
        let backdrop = div()
            .id("menu-backdrop")
            .w(viewport.width)
            .h(viewport.height)
            .occlude()
            .on_mouse_down(MouseButton::Left, close())
            .on_mouse_down(MouseButton::Right, close());
        let backdrop =
            deferred(anchored().position(point(px(0.), px(0.))).child(backdrop)).with_priority(1);
        let menu = match &open.anchor {
            // Below and right of the pointer, or above or left of it
            // where there isn't room, then slid inside the window.
            MenuAnchor::Pointer(position) => Some(
                deferred(anchored().position(*position).child(open.menu.clone())).with_priority(2),
            ),
            MenuAnchor::Below { .. } | MenuAnchor::Above { .. } => None,
        };
        Some(div().child(backdrop).children(menu).into_any_element())
    }

    /// The menu, when it hangs under the control called `key`. Put it in
    /// that control's box.
    pub fn render_attached(&self, key: &str, gap: Pixels) -> Option<AnyElement> {
        let open = self.open.as_ref()?;
        if open.anchor.key().is_none_or(|open_key| open_key != key) {
            return None;
        }
        let (corner, spot, offset) = match &open.anchor {
            MenuAnchor::Below {
                align_right: true, ..
            } => (Corner::TopRight, div().absolute().right_0().top_full(), gap),
            MenuAnchor::Above { .. } => (
                Corner::BottomLeft,
                div().absolute().left_0().bottom_full(),
                -gap,
            ),
            _ => (Corner::TopLeft, div().absolute().left_0().top_full(), gap),
        };
        let menu = anchored()
            .anchor(corner)
            .offset(point(px(0.), offset))
            .snap_to_window()
            .child(open.menu.clone());
        Some(
            spot.child(deferred(menu).with_priority(2))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use gpui::px;

    use super::{MenuItem, menu_height};
    use crate::theme::UiTheme;

    #[test]
    fn a_menu_is_as_tall_as_its_rows_and_separators() {
        let theme = UiTheme::default();
        let row = || MenuItem::action("Row", |_, _| {});
        let items = [row(), MenuItem::Separator, row()];
        let separator = theme.menu_padding * 2. + theme.hairline;
        let expected = theme.menu_row_height * 2. + separator + theme.menu_padding * 2.;
        assert_eq!(menu_height(&items, &theme), expected);
        assert!(menu_height(&[], &theme) > px(0.));
    }
}
