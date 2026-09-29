//! The file sidebar's chrome around whatever the left panel hosts: a
//! header with the search button and the sidebar button, the file tree's
//! tools (new note, new folder, sort order, collapse all), and a footer
//! with the vault switcher, help and settings.

use gpui::{AnyElement, Context, Entity, Pixels, SharedString, Window, div, prelude::*};

use super::Workspace;
use super::files::folder_name;
use super::help::ShortcutsHelp;
use super::state::AppState;
use super::window::open_vault_window;
use crate::file_tree::{EntryKind, SortOrder};
use crate::icons::{IconName, icon};
use crate::ui::Selectable;
use crate::ui::{IconButton, MenuAnchor, MenuItem, Tooltip, ui_theme};

pub const SORT_KEY: &str = "sidebar-sort";
pub const VAULT_KEY: &str = "sidebar-vault";

/// The sort menu: (order, label).
const SORT_ORDERS: [(SortOrder, &str); 4] = [
    (SortOrder::NameAscending, "File name (A to Z)"),
    (SortOrder::NameDescending, "File name (Z to A)"),
    (SortOrder::ModifiedNewest, "Modified time (new to old)"),
    (SortOrder::ModifiedOldest, "Modified time (old to new)"),
];

impl Workspace {
    /// A button that runs command `id` here. Commands nothing runs get no
    /// button.
    fn command_button(
        &self,
        key: &'static str,
        name: IconName,
        id: &'static str,
        cx: &mut Context<Self>,
    ) -> Option<IconButton> {
        if !self.can_run(id) {
            return None;
        }
        Some(
            IconButton::new(key, name)
                .command(id, cx)
                .on_click(cx.listener(move |workspace, _, window, cx| {
                    workspace.run_command(id, window, cx);
                })),
        )
    }

    /// The sidebar's top row, which leaves `corner_inset` at its left for
    /// the window's own buttons.
    pub(super) fn render_sidebar_header(
        &self,
        corner_inset: Pixels,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let ui = ui_theme(cx);
        // The file tree is the sidebar's only view, so there's no button
        // for it that looks like a selected tab; Mod+Shift+E focuses it.
        let search = self.command_button(
            "sidebar-search",
            IconName::MagnifyingGlass,
            "search.open",
            cx,
        );
        let toggle = self
            .command_button(
                "sidebar-toggle",
                IconName::SidebarSimple,
                "sidebar.files.toggle",
                cx,
            )
            .map(|button| button.label("Hide file sidebar"));
        let views = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(ui.space_xs)
            .children(search);
        div()
            .flex()
            .flex_col()
            .flex_none()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .h(ui.tab_bar_height)
                    .px(ui.sidebar_padding)
                    .pl(ui.sidebar_padding + corner_inset)
                    .child(views)
                    .children(toggle),
            )
            .children(self.render_tree_tools(cx))
    }

    fn render_tree_tools(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.file_tree.as_ref()?;
        let ui = ui_theme(cx);
        let new_note =
            self.command_button("sidebar-new-note", IconName::NotePencil, "note.new", cx);
        let daily = self.command_button(
            "sidebar-daily-note",
            IconName::CalendarBlank,
            "daily.open",
            cx,
        );

        let new_folder = self.command_button(
            "sidebar-new-folder",
            IconName::FolderPlus,
            "file-tree.new-folder",
            cx,
        );
        let sort_menu = self.menu.render_attached(SORT_KEY, ui.space_xs);
        let sort = self
            .command_button(SORT_KEY, IconName::SortAscending, "file-tree.sort", cx)
            .map(|button| {
                button
                    .label("Change sort order")
                    .active(sort_menu.is_some())
                    .attach(sort_menu)
            });
        let collapse = self.command_button(
            "sidebar-collapse-all",
            IconName::ArrowsInLineVertical,
            "file-tree.collapse-all",
            cx,
        );
        Some(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(ui.space_xs)
                .px(ui.sidebar_padding)
                .pb(ui.space_sm)
                .children(new_note)
                .children(daily)
                .children(new_folder)
                .children(sort)
                .children(collapse)
                .into_any_element(),
        )
    }

    pub(super) fn render_sidebar_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = ui_theme(cx);
        let name: SharedString = folder_name(&self.vault).into();
        let vault_menu = self.menu.render_attached(VAULT_KEY, ui.space_xs);
        let open = vault_menu.is_some();
        let vault = div()
            .id(VAULT_KEY)
            .selector(|| VAULT_KEY.to_owned())
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .gap(ui.space_sm)
            .min_w_0()
            .h(ui.icon_button_size)
            .px(ui.space_sm)
            .rounded(ui.icon_button_radius)
            .when(open, |vault| vault.bg(ui.control_active))
            .hover(|style| style.bg(ui.control_hover))
            .active(|style| style.bg(ui.control_pressed))
            .when(!open, |vault| {
                vault.tooltip(Tooltip::for_command("vault.switch", cx).builder())
            })
            .on_click(cx.listener(|workspace, _, window, cx| workspace.open_vault_menu(window, cx)))
            .child(
                icon(IconName::CaretUpDown)
                    .flex_none()
                    .size(ui.small_icon_size)
                    .text_color(ui.icon),
            )
            .child(crate::ui::truncated(name))
            .children(vault_menu);
        let help = self.command_button("sidebar-help", IconName::Question, "help.shortcuts", cx);
        let settings =
            self.command_button("sidebar-settings", IconName::GearSix, "settings.open", cx);
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .justify_between()
            .gap(ui.space_sm)
            .h(ui.sidebar_footer_height)
            .px(ui.sidebar_padding)
            .child(vault)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_none()
                    .gap(ui.space_xs)
                    .children(help)
                    .children(settings),
            )
    }

    /// Opens the sort order menu under its button.
    pub fn open_sort_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.file_tree.clone() else {
            return;
        };
        let current = tree.read(cx).sort_order();
        let items = SORT_ORDERS
            .iter()
            .map(|&(order, label)| {
                let tree = tree.downgrade();
                MenuItem::action(label, move |_, cx| {
                    tree.update(cx, |tree, cx| tree.set_sort_order(order, cx))
                        .ok();
                })
                .checked(order == current)
            })
            .collect();
        let anchor = MenuAnchor::Below {
            key: SORT_KEY.into(),
            align_right: false,
        };
        self.menu.open(items, anchor, window, cx);
    }

    /// Opens the vault switcher: recent vaults, then another one.
    pub fn open_vault_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut items: Vec<MenuItem> = AppState::recent_vaults()
            .into_iter()
            .filter(|vault| *vault != self.vault)
            .map(|vault| {
                MenuItem::action(folder_name(&vault), move |_, cx| {
                    if let Err(error) = open_vault_window(&vault, None, cx) {
                        crate::notices::open_failed(&vault, error, cx);
                    }
                })
                .with_icon(IconName::Folder)
            })
            .collect();
        if !items.is_empty() {
            items.push(MenuItem::Separator);
        }
        let workspace = cx.entity().downgrade();
        items.push(
            MenuItem::command("vault.open", cx)
                .with_label("Open another vault…")
                .with_icon(IconName::FolderOpen)
                .with_handler(move |window, cx| {
                    workspace
                        .update(cx, |workspace, cx| {
                            workspace.run_command("vault.open", window, cx)
                        })
                        .ok();
                }),
        );
        // The footer is at the bottom of the window, so the menu opens upward.
        let anchor = MenuAnchor::Above {
            key: VAULT_KEY.into(),
        };
        self.menu.open(items, anchor, window, cx);
    }

    /// Opens or closes the keyboard shortcuts.
    pub fn toggle_help(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let available: Vec<&'static str> = super::help::HELP_COMMANDS
            .iter()
            .flat_map(|(_, ids)| ids.iter().copied())
            .filter(|id| self.can_run(id))
            .collect();
        self.toggle_modal(window, cx, |window, cx| {
            ShortcutsHelp::new(&available, window, cx)
        });
        let Some(help) = self.active_modal::<ShortcutsHelp>() else {
            return;
        };
        let subscription = cx.subscribe_in(&help, window, Self::on_help_event);
        self._subscriptions.push(subscription);
    }

    fn on_help_event(
        &mut self,
        _: &Entity<ShortcutsHelp>,
        event: &super::help::HelpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let super::help::HelpEvent::Run(id) = event;
        let id = id.clone();
        cx.defer_in(window, move |workspace, window, cx| {
            let pane = workspace.active_pane.clone();
            workspace.run_in_pane(&pane, &id, window, cx);
        });
    }
}

impl Workspace {
    /// `file-tree.new-folder`: shows the sidebar and starts naming a new
    /// folder in the tree.
    pub(crate) fn new_folder_in_tree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.file_tree.clone() else {
            return;
        };
        self.show_left_panel(cx);
        tree.update(cx, |tree, cx| {
            tree.start_create(EntryKind::Folder, window, cx)
        });
    }

    /// `file-tree.collapse-all`: closes every folder in the tree.
    pub(crate) fn collapse_tree(&mut self, cx: &mut Context<Self>) {
        if let Some(tree) = self.file_tree.clone() {
            tree.update(cx, |tree, cx| tree.set_expanded_folders(Vec::new(), cx));
        }
    }
}
