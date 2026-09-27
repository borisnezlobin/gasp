//! Drawing the tree: a virtualised list of rows, the inline name field,
//! the context menu and the trash prompt.

use std::ops::Range;
use std::path::{Path, PathBuf};

use gpui::{
    AnyElement, ClickEvent, Context, Entity, MouseButton, MouseDownEvent, Render, SharedString,
    Window, anchored, deferred, div, prelude::*, uniform_list,
};

use super::entries::{Entry, EntryKind};
use super::menu::ContextMenu;
use super::model::Row;
use super::view::{DisplayRow, EditTarget, FileTree};
use crate::icons::{IconName, icon};
use crate::theme::{PanelTheme, UiTheme};
use crate::ui::ui_theme;

/// What's being dragged: an entry's path relative to the vault.
#[derive(Clone, Debug)]
pub struct DraggedEntry {
    pub path: PathBuf,
    pub label: SharedString,
}

/// The label that follows the pointer while dragging.
struct DragPreview {
    label: SharedString,
    theme: PanelTheme,
}

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        div()
            .px(theme.padding_x)
            .py(theme.padding_y)
            .rounded(theme.radius)
            .bg(theme.menu_background)
            .shadow(vec![theme.menu_shadow()])
            .text_size(theme.font_size)
            .text_color(theme.text)
            .font_family(theme.font_family)
            .child(self.label.clone())
    }
}

fn kind_icon(entry: &Entry, expanded: bool) -> IconName {
    match (entry.kind, expanded) {
        (EntryKind::Folder, true) => IconName::FolderOpen,
        (EntryKind::Folder, false) => IconName::Folder,
        (EntryKind::Note, _) => IconName::File,
        (EntryKind::Image, _) => IconName::Image,
        (EntryKind::Pdf, _) => IconName::FilePdf,
    }
}

/// How a row should look.
#[derive(Clone, Copy, Default)]
struct RowState {
    selected: bool,
    focused: bool,
    active: bool,
    cut: bool,
}

impl Render for FileTree {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = ui_theme(cx);
        let root_menu = self
            .menu
            .as_ref()
            .filter(|menu| menu.target.is_none())
            .map(|menu| self.render_menu(menu, cx));
        div()
            .key_context("FileTree")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_action(cx.listener(Self::on_run_command))
            .size_full()
            .flex()
            .flex_col()
            .font_family(ui.font_family)
            .text_size(ui.font_size)
            .text_color(ui.text)
            .child(self.render_list(window, cx))
            .children(self.render_trash_prompt(cx))
            .children(root_menu)
    }
}

impl FileTree {
    fn render_list(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let rows = uniform_list(
            "file-tree-rows",
            self.display_row_count(),
            cx.processor(|tree, range: Range<usize>, window, cx| {
                range
                    .map(|index| tree.render_display_row(index, window, cx))
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(self.scroll.clone())
        .size_full();
        div()
            .id("file-tree-list")
            .flex_1()
            .min_h_0()
            .px(theme.padding_y)
            .py(theme.padding_y)
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(Self::on_background_right_click),
            )
            .drag_over::<DraggedEntry>({
                let drop = theme.drop_target;
                move |style, _, _, _| style.bg(drop)
            })
            .on_drop(cx.listener(|tree, dragged: &DraggedEntry, _, cx| {
                let root = tree.root().to_path_buf();
                tree.move_into(&dragged.path, &root, cx);
            }))
            .child(rows)
    }

    fn on_background_right_click(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
        self.open_menu(None, Some(event.position), cx);
    }

    fn render_display_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = ui_theme(cx);
        let (row, depth) = match self.display_row(index) {
            DisplayRow::Entry(row) => {
                let depth = self.model.row(row).map_or(0, |row| row.depth);
                (self.render_entry_row(row, &ui, window, cx), depth)
            }
            DisplayRow::NewEntry { depth, kind } => {
                (self.render_new_entry_row(depth, kind, &ui), depth)
            }
        };
        // Room around the row so the list's clipping doesn't cut its focus ring.
        let inset = self.theme.ring_width * 2.;
        div()
            .relative()
            .w_full()
            .px(inset)
            .py(self.theme.ring_width)
            .children(indent_guides(depth, inset, &ui))
            .child(row)
            .into_any_element()
    }

    fn row_state(&self, path: &Path, window: &Window) -> RowState {
        RowState {
            selected: self.selected.as_deref() == Some(path),
            focused: self.focus_handle.is_focused(window),
            active: self.active.as_deref() == Some(path),
            cut: self.cut.as_deref() == Some(path),
        }
    }

    fn row_shell(&self, depth: usize, state: RowState, ui: &UiTheme) -> gpui::Div {
        let theme = &self.theme;
        let background = match (state.selected && state.focused, state.active) {
            (true, _) => Some(theme.selected_focused),
            (false, true) => Some(ui.tree_active_background),
            _ => None,
        };
        let hover = ui.tree_hover_background;
        let mut shell = div()
            .relative()
            .h(ui.tree_row_height)
            .w_full()
            .flex()
            .items_center()
            .gap(ui.tree_row_gap)
            .pl(ui.space_md + ui.tree_indent * depth as f32)
            .pr(ui.space_md)
            .rounded(ui.tree_row_radius)
            .when_some(background, |row, color| row.bg(color))
            .when(background.is_none(), |row| {
                row.hover(move |style| style.bg(hover))
            })
            .when(state.selected && state.focused, |row| {
                row.shadow(vec![ui.ring(ui.tree_focus_ring)])
            });
        if state.cut {
            shell = shell.opacity(theme.cut_opacity);
        }
        shell
    }

    fn render_entry_row(
        &mut self,
        index: usize,
        ui: &UiTheme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = self.model.rows()[index].clone();
        let state = self.row_state(&row.entry.path, window);
        let theme = self.theme.clone();
        let content = self.row_content(&row, state, ui);
        let menu = self
            .menu
            .as_ref()
            .filter(|menu| menu.target.as_ref() == Some(&row.entry.path))
            .map(|menu| self.render_menu(menu, cx));
        let drop_folder = if row.entry.is_folder() {
            row.entry.path.clone()
        } else {
            row.entry.parent().to_path_buf()
        };
        let dragged = DraggedEntry {
            path: row.entry.path.clone(),
            label: row.entry.label().to_string().into(),
        };
        let selector = format!("tree-row-{}", row.entry.label());
        self.row_shell(row.depth, state, ui)
            .id(("file-tree-row", index))
            .debug_selector(|| selector)
            .children(content)
            .children(menu)
            .on_click(cx.listener(move |tree, event: &ClickEvent, window, cx| {
                let new_tab = event.modifiers().secondary();
                tree.click_row(index, new_tab, window, cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |tree, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    window.focus(&tree.focus_handle);
                    let target = tree.model.row(index).map(|row| row.entry.path.clone());
                    tree.open_menu(target, Some(event.position), cx);
                }),
            )
            .on_drag(dragged, move |dragged, _, _, cx| {
                let label = dragged.label.clone();
                let theme = theme.clone();
                cx.new(|_| DragPreview { label, theme })
            })
            .drag_over::<DraggedEntry>({
                let drop = self.theme.drop_target;
                move |style, _, _, _| style.bg(drop)
            })
            .on_drop(cx.listener(move |tree, dragged: &DraggedEntry, _, cx| {
                cx.stop_propagation();
                let folder = tree.absolute(&drop_folder);
                let from = tree.absolute(&dragged.path);
                tree.move_into(&from, &folder, cx);
            }))
            .into_any_element()
    }

    /// Icon and name, or the rename field.
    fn row_content(&self, row: &Row, state: RowState, ui: &UiTheme) -> Vec<AnyElement> {
        let color = if state.active {
            ui.icon_active
        } else {
            ui.icon
        };
        let kind = icon(kind_icon(&row.entry, row.expanded))
            .flex_none()
            .size(ui.icon_size)
            .text_color(color);
        vec![kind.into_any_element(), self.row_label(row)]
    }

    fn row_label(&self, row: &Row) -> AnyElement {
        let renaming = self
            .edit
            .as_ref()
            .filter(|edit| matches!(&edit.target, EditTarget::Rename(entry) if entry.path == row.entry.path));
        if let Some(edit) = renaming {
            return self.name_field(&edit.field, edit.error.clone());
        }
        div()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .child(row.entry.label().to_string())
            .into_any_element()
    }

    fn name_field(
        &self,
        field: &Entity<crate::text_input::TextInput>,
        error: Option<String>,
    ) -> AnyElement {
        let theme = &self.theme;
        div()
            .flex_1()
            .min_w_0()
            .relative()
            .child(field.clone())
            .children(error.map(|message| {
                div()
                    .absolute()
                    .top_full()
                    .left_0()
                    .px(theme.padding_x)
                    .py(theme.padding_y)
                    .rounded(theme.radius)
                    .bg(theme.menu_background)
                    .shadow(vec![theme.menu_shadow()])
                    .text_size(theme.small_font_size)
                    .text_color(theme.error_text)
                    .child(message)
            }))
            .into_any_element()
    }

    fn render_new_entry_row(&mut self, depth: usize, kind: EntryKind, ui: &UiTheme) -> AnyElement {
        let Some(edit) = self.edit.as_ref() else {
            return div().into_any_element();
        };
        let entry = Entry::new("", kind);
        let icon = icon(kind_icon(&entry, false))
            .flex_none()
            .size(ui.icon_size)
            .text_color(ui.icon);
        let state = RowState {
            selected: true,
            focused: true,
            ..RowState::default()
        };
        self.row_shell(depth, state, ui)
            .child(icon)
            .child(self.name_field(&edit.field, edit.error.clone()))
            .into_any_element()
    }

    fn render_menu(&self, menu: &ContextMenu, cx: &mut Context<Self>) -> AnyElement {
        let theme = &self.theme;
        let items = menu.items.iter().enumerate().map(|(index, item)| {
            let item = *item;
            let highlighted = index == menu.highlighted;
            div()
                .id(("file-tree-menu-item", index))
                .debug_selector(|| format!("tree-menu-{}", item.label()))
                .h(theme.row_height)
                .px(theme.padding_x)
                .flex()
                .items_center()
                .gap(theme.gap)
                .rounded(theme.radius)
                .when(highlighted, |row| row.bg(theme.selected_focused))
                .hover(|style| style.bg(theme.hover))
                .child(
                    icon(item.icon())
                        .size(theme.icon_size)
                        .text_color(theme.icon),
                )
                .child(item.label())
                .on_click(cx.listener(move |tree, _: &ClickEvent, window, cx| {
                    tree.run_menu_item(item, window, cx);
                }))
        });
        let panel = div()
            .id("file-tree-menu")
            .occlude()
            .w(theme.menu_width)
            .p(theme.padding_y)
            .rounded(theme.radius)
            .bg(theme.menu_background)
            .shadow(vec![theme.menu_shadow()])
            .font_family(theme.font_family)
            .text_size(theme.font_size)
            .text_color(theme.text)
            .on_mouse_down_out(cx.listener(|tree, _: &MouseDownEvent, _, cx| tree.close_menu(cx)))
            .children(items);
        match menu.position {
            Some(position) => deferred(anchored().position(position).snap_to_window().child(panel))
                .with_priority(1)
                .into_any_element(),
            // From the keyboard, the menu opens just under its row.
            None => div()
                .absolute()
                .top(theme.row_height)
                .left(theme.indent * 2.)
                .child(deferred(anchored().snap_to_window().child(panel)).with_priority(1))
                .into_any_element(),
        }
    }

    fn render_trash_prompt(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let path = self.pending_trash.as_ref()?;
        let theme = &self.theme;
        let name = self
            .model
            .index_of(path)
            .and_then(|index| self.model.row(index))
            .map_or_else(
                || path.to_string_lossy().into_owned(),
                |row| row.entry.label().to_string(),
            );
        let button = |id: &'static str, label: &'static str, primary: bool| {
            div()
                .id(id)
                .px(theme.padding_x)
                .py(theme.padding_y)
                .rounded(theme.radius)
                .bg(if primary {
                    theme.control_selected
                } else {
                    theme.control_background
                })
                .text_color(if primary {
                    theme.control_selected_text
                } else {
                    theme.text
                })
                .child(label)
        };
        let prompt = div()
            .m(theme.padding_y)
            .p(theme.padding_x)
            .flex()
            .flex_col()
            .gap(theme.gap)
            .rounded(theme.radius)
            .bg(theme.menu_background)
            .shadow(vec![theme.menu_shadow()])
            .child(format!("Move “{name}” to the trash?"))
            .child(
                div()
                    .flex()
                    .gap(theme.gap)
                    .child(
                        button("file-tree-trash-confirm", "Move to trash", true).on_click(
                            cx.listener(|tree, _: &ClickEvent, _, cx| tree.confirm_trash(cx)),
                        ),
                    )
                    .child(button("file-tree-trash-cancel", "Cancel", false).on_click(
                        cx.listener(|tree, _: &ClickEvent, _, cx| tree.cancel_trash(cx)),
                    )),
            );
        Some(prompt.into_any_element())
    }
}

/// A hairline under each ancestor folder's icon, for a row `depth` deep.
fn indent_guides(depth: usize, inset: gpui::Pixels, ui: &UiTheme) -> Vec<AnyElement> {
    (0..depth)
        .map(|level| {
            let left = inset + ui.space_md + ui.tree_indent * level as f32 + ui.icon_size / 2.;
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(left)
                .w(ui.indent_guide_width)
                .bg(ui.indent_guide)
                .into_any_element()
        })
        .collect()
}
