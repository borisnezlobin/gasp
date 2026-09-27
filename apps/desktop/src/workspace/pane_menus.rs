//! The menus a pane's controls open: the list of tabs, the note's `⋯`
//! menu and the note's right-click menu. Only commands something can run
//! are listed, so no item does nothing.

use std::path::{Path, PathBuf};

use gpui::{App, ClipboardItem, Context, Entity, Focusable, Window};

use super::Workspace;
use super::pane::{Pane, PaneMenu};
use super::pane_tree::Direction;
use crate::icons::IconName;
use crate::keymap::RunCommand;
use crate::ui::{MenuAnchor, MenuItem};

type Command = (&'static str, &'static str, IconName);

/// The note menu's commands in groups, with Copy path and Open in default
/// app going in before group [`FILE_GROUP_AT`].
const MORE_GROUPS: [&[Command]; 5] = [
    &[
        ("note.rename", "Rename", IconName::PencilSimple),
        ("note.delete", "Move to trash", IconName::Trash),
        (
            "file-tree.reveal-active",
            "Reveal in file tree",
            IconName::FolderOpen,
        ),
    ],
    &[
        (
            "pane.split-right",
            "Split right",
            IconName::SquareSplitHorizontal,
        ),
        (
            "pane.split-down",
            "Split down",
            IconName::SquareSplitVertical,
        ),
    ],
    &[
        ("app.export", "Export to PDF", IconName::FilePdf),
        ("app.print", "Print", IconName::Printer),
    ],
    &[
        ("find.open", "Find", IconName::MagnifyingGlass),
        ("find.replace", "Replace", IconName::Swap),
        (
            "outline.jump-to-heading",
            "Jump to heading",
            IconName::ListBullets,
        ),
        ("sidebar.backlinks", "Backlinks", IconName::ArrowSquareIn),
    ],
    &[("tab.close", "Close tab", IconName::X)],
];
const FILE_GROUP_AT: usize = 3;

/// A tab's right-click menu: closing, then splitting.
const TAB_CLOSE_ITEMS: [Command; 3] = [
    ("tab.close", "Close", IconName::X),
    ("tab.close-others", "Close others", IconName::Square),
    (
        "tab.close-right",
        "Close to the right",
        IconName::ArrowRight,
    ),
];

const TAB_SPLIT_ITEMS: [Command; 2] = [
    (
        "pane.split-right",
        "Split right",
        IconName::SquareSplitHorizontal,
    ),
    (
        "pane.split-down",
        "Split down",
        IconName::SquareSplitVertical,
    ),
];

/// The tab menu's Move submenu, one item per side.
const TAB_MOVE_ITEMS: [(Command, Direction); 4] = [
    (
        (
            "pane.move-tab-left",
            "Pane on the left",
            IconName::ArrowLeft,
        ),
        Direction::Left,
    ),
    (
        (
            "pane.move-tab-right",
            "Pane on the right",
            IconName::ArrowRight,
        ),
        Direction::Right,
    ),
    (
        ("pane.move-tab-up", "Pane above", IconName::ArrowUp),
        Direction::Up,
    ),
    (
        ("pane.move-tab-down", "Pane below", IconName::ArrowDown),
        Direction::Down,
    ),
];

/// The note's right-click menu before the Format submenu, in groups, in
/// the order native text menus use.
const EDIT_GROUPS: [&[Command]; 3] = [
    &[
        ("edit.undo", "Undo", IconName::ArrowCounterClockwise),
        ("edit.redo", "Redo", IconName::ArrowClockwise),
    ],
    &[
        ("edit.cut", "Cut", IconName::Scissors),
        ("edit.copy", "Copy", IconName::Copy),
        ("edit.paste", "Paste", IconName::Clipboard),
        (
            "edit.paste-plain",
            "Paste as plain text",
            IconName::ClipboardText,
        ),
    ],
    &[("select.all", "Select all", IconName::SelectionAll)],
];

const FORMAT_ITEMS: [Command; 9] = [
    ("format.bold", "Bold", IconName::TextB),
    ("format.italic", "Italic", IconName::TextItalic),
    ("format.underline", "Underline", IconName::TextUnderline),
    (
        "format.strikethrough",
        "Strikethrough",
        IconName::TextStrikethrough,
    ),
    ("format.highlight", "Highlight", IconName::HighlighterCircle),
    ("format.code", "Code", IconName::Code),
    ("format.math-inline", "Math", IconName::Function),
    ("format.comment", "Comment", IconName::ChatText),
    ("format.link", "Link", IconName::Link),
];

const INSERT_ITEMS: [Command; 3] = [
    (
        "footnote.insert-or-jump",
        "Insert footnote",
        IconName::TextSuperscript,
    ),
    ("note.import-image", "Insert image", IconName::Image),
    ("template.insert", "Insert template", IconName::Stamp),
];

impl Workspace {
    /// Whether anything runs `id`: the workspace, a feature wired in with
    /// [`Workspace::on_command`], or the editor.
    pub fn can_run(&self, id: &str) -> bool {
        self.extra_commands.contains_key(id) || super::handles(id) || crate::commands::handles(id)
    }

    /// Makes `pane` active, then runs `id` there: the workspace's own
    /// commands directly, the rest through the focused note.
    pub(crate) fn run_in_pane(
        &mut self,
        pane: &Entity<Pane>,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_pane(pane, window, cx);
        if !self.run_command(id, window, cx) {
            let action = RunCommand {
                id: id.to_owned().into(),
            };
            window.dispatch_action(Box::new(action), cx);
        }
    }

    /// Shows `path` in the file tree: the panel opens, folders above it
    /// expand, and a folder opens too. With `focus` the tree takes the
    /// keyboard.
    pub fn reveal_in_tree(
        &mut self,
        path: &Path,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tree) = self.file_tree.clone() else {
            return;
        };
        self.show_left_panel(cx);
        tree.update(cx, |tree, cx| {
            if path.is_dir()
                && let Ok(relative) = path.strip_prefix(tree.root())
            {
                let mut folders = tree.expanded_folders();
                folders.push(relative.to_path_buf());
                tree.set_expanded_folders(folders, cx);
            }
            tree.reveal(path, cx);
        });
        if focus {
            window.focus(&tree.read(cx).focus_handle(cx));
        }
    }

    pub(crate) fn open_pane_menu(
        &mut self,
        pane: &Entity<Pane>,
        kind: PaneMenu,
        anchor: MenuAnchor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let items = match kind {
            PaneMenu::TabList => self.tab_list_items(pane, cx),
            PaneMenu::More => self.more_items(pane, cx),
            PaneMenu::Editor => self.editor_items(pane, cx),
            PaneMenu::Tab(index) => self.tab_items(pane, index, cx),
        };
        if !items.is_empty() {
            pane.update(cx, |pane, cx| pane.show_menu(items, anchor, window, cx));
        }
    }

    /// A menu item that runs `id` in `pane`, when something can run it.
    fn pane_command(
        &self,
        pane: &Entity<Pane>,
        id: &'static str,
        cx: &mut Context<Self>,
    ) -> Option<MenuItem> {
        if !self.can_run(id) {
            return None;
        }
        let workspace = cx.entity().downgrade();
        let pane = pane.downgrade();
        Some(MenuItem::command(id, cx).with_handler(move |window, cx| {
            let (Some(workspace), Some(pane)) = (workspace.upgrade(), pane.upgrade()) else {
                return;
            };
            workspace.update(cx, |workspace, cx| {
                workspace.run_in_pane(&pane, id, window, cx)
            });
        }))
    }

    fn tab_list_items(&self, pane: &Entity<Pane>, cx: &mut Context<Self>) -> Vec<MenuItem> {
        let workspace = cx.entity().downgrade();
        let (titles, active) = {
            let pane = pane.read(cx);
            let titles: Vec<String> = pane.tabs().iter().map(|tab| tab.title(cx)).collect();
            (titles, pane.active_index())
        };
        let mut items: Vec<MenuItem> = titles
            .into_iter()
            .enumerate()
            .map(|(index, title)| {
                let workspace = workspace.clone();
                let target = pane.downgrade();
                MenuItem::action(title, move |window, cx| {
                    let (Some(workspace), Some(pane)) = (workspace.upgrade(), target.upgrade())
                    else {
                        return;
                    };
                    workspace.update(cx, |workspace, cx| {
                        workspace.activate_pane(&pane, window, cx);
                        workspace.activate_tab(index, window, cx);
                    });
                })
                .checked(index == active)
            })
            .collect();
        items.push(MenuItem::Separator);
        let reopen = self
            .pane_command(pane, "tab.reopen", cx)
            .map(|item| item.disabled(self.closed_tabs.is_empty()));
        items.extend(self.pane_command(pane, "tab.new", cx));
        items.extend(reopen);
        items.extend(self.pane_command(pane, "tab.close", cx));
        items.extend(self.pane_command(pane, "pane.close", cx));
        items
    }

    /// A tab's right-click menu. The tab is active by the time it opens,
    /// so each item runs on the active tab.
    fn tab_items(
        &self,
        pane: &Entity<Pane>,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Vec<MenuItem> {
        let (count, path) = {
            let pane = pane.read(cx);
            let path = pane.tabs().get(index).and_then(|tab| tab.path(cx));
            (pane.len(), path.map(Path::to_path_buf))
        };
        let disabled = |id: &str| match id {
            "tab.close-others" => count < 2,
            "tab.close-right" => index + 1 >= count,
            _ => false,
        };
        let mut items = Vec::new();
        for &(id, label, icon) in TAB_CLOSE_ITEMS.iter().chain(&TAB_SPLIT_ITEMS) {
            if id == TAB_SPLIT_ITEMS[0].0 {
                items.push(MenuItem::Separator);
            }
            items.extend(self.pane_command(pane, id, cx).map(|item| {
                item.with_label(label)
                    .with_icon(icon)
                    .disabled(disabled(id))
            }));
        }
        items.push(self.move_tab_menu(pane, count, cx));
        items.push(MenuItem::Separator);
        if path.is_some() {
            items.extend(
                self.pane_command(pane, "file-tree.reveal-active", cx)
                    .map(|item| {
                        item.with_label("Reveal in file tree")
                            .with_icon(IconName::FolderOpen)
                    }),
            );
        }
        items.extend(path.map(copy_path_item));
        tidy_separators(items)
    }

    /// Move to another pane: a side with no pane splits one off, which a
    /// pane's only tab can't do.
    fn move_tab_menu(&self, pane: &Entity<Pane>, count: usize, cx: &mut Context<Self>) -> MenuItem {
        let items = TAB_MOVE_ITEMS
            .iter()
            .filter_map(|&((id, label, icon), side)| {
                let stuck = count < 2 && self.panes.beside(pane, side).is_none();
                self.pane_command(pane, id, cx)
                    .map(|item| item.with_label(label).with_icon(icon).disabled(stuck))
            })
            .collect();
        MenuItem::submenu("Move to", items).with_icon(IconName::Columns)
    }

    fn more_items(&self, pane: &Entity<Pane>, cx: &mut Context<Self>) -> Vec<MenuItem> {
        let path = pane
            .read(cx)
            .active_tab()
            .and_then(|tab| tab.path(cx))
            .map(Path::to_path_buf);
        let mut items = Vec::new();
        for (index, group) in MORE_GROUPS.iter().enumerate() {
            if let (FILE_GROUP_AT, Some(path)) = (index, path.clone()) {
                items.extend(file_items(path));
                items.push(MenuItem::Separator);
            }
            for &(id, label, icon) in *group {
                items.extend(
                    self.pane_command(pane, id, cx)
                        .map(|item| item.with_label(label).with_icon(icon)),
                );
            }
            items.push(MenuItem::Separator);
        }
        tidy_separators(items)
    }

    fn editor_items(&self, pane: &Entity<Pane>, cx: &App) -> Vec<MenuItem> {
        let command = |(id, label, icon): Command| {
            MenuItem::command(id, cx).with_label(label).with_icon(icon)
        };
        let available = EditAvailability::of(pane, cx);
        let mut items: Vec<MenuItem> = Vec::new();
        for group in EDIT_GROUPS {
            items.extend(
                group
                    .iter()
                    .map(|&item| command(item).disabled(!available.allows(item.0))),
            );
            items.push(MenuItem::Separator);
        }
        let format = FORMAT_ITEMS.into_iter().map(command).collect();
        items.push(MenuItem::submenu("Format", format).with_icon(IconName::TextAa));
        items.extend(INSERT_ITEMS.into_iter().map(command));
        items
    }
}

/// Which editing commands have something to act on in a pane's note.
struct EditAvailability {
    undo: bool,
    redo: bool,
    selection: bool,
    clipboard: bool,
}

impl EditAvailability {
    fn of(pane: &Entity<Pane>, cx: &App) -> EditAvailability {
        let editor = pane.read(cx).active_editor();
        let editor = editor.as_ref().map(|editor| editor.read(cx));
        let clipboard = cx
            .read_from_clipboard()
            .is_some_and(|item| !item.entries().is_empty());
        EditAvailability {
            undo: editor.is_some_and(|editor| editor.can_undo()),
            redo: editor.is_some_and(|editor| editor.can_redo()),
            selection: editor.is_some_and(|editor| !editor.selected_range().is_empty()),
            clipboard,
        }
    }

    /// Undo and Redo need history, Cut and Copy a selection, and the
    /// pastes something on the clipboard.
    fn allows(&self, id: &str) -> bool {
        match id {
            "edit.undo" => self.undo,
            "edit.redo" => self.redo,
            "edit.cut" | "edit.copy" => self.selection,
            "edit.paste" | "edit.paste-plain" => self.clipboard,
            _ => true,
        }
    }
}

/// Copy path and Open in default app, for the note at `path`.
fn file_items(path: PathBuf) -> Vec<MenuItem> {
    vec![
        copy_path_item(path.clone()),
        MenuItem::action("Open in default app", move |_, cx| {
            cx.open_with_system(&path)
        })
        .with_icon(IconName::ArrowSquareOut),
    ]
}

fn copy_path_item(path: PathBuf) -> MenuItem {
    MenuItem::action("Copy path", move |_, cx| {
        let text = path.to_string_lossy().into_owned();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    })
    .with_icon(IconName::Copy)
}

/// Drops separators at either end and doubled ones, left behind by
/// commands that aren't available.
fn tidy_separators(items: Vec<MenuItem>) -> Vec<MenuItem> {
    let mut tidy: Vec<MenuItem> = Vec::with_capacity(items.len());
    for item in items {
        let separator = matches!(item, MenuItem::Separator);
        let after_separator = matches!(tidy.last(), None | Some(MenuItem::Separator));
        if !(separator && after_separator) {
            tidy.push(item);
        }
    }
    if matches!(tidy.last(), Some(MenuItem::Separator)) {
        tidy.pop();
    }
    tidy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separators_never_double_or_dangle() {
        let items = vec![
            MenuItem::Separator,
            MenuItem::action("a", |_, _| {}),
            MenuItem::Separator,
            MenuItem::Separator,
            MenuItem::action("b", |_, _| {}),
            MenuItem::Separator,
        ];
        let labels: Vec<String> = tidy_separators(items)
            .iter()
            .map(|item| item.label().map_or("-".into(), ToString::to_string))
            .collect();
        assert_eq!(labels, ["a", "-", "b"]);
    }

    #[test]
    fn every_menu_command_exists() {
        use editor_config::commands::BUILTIN_COMMANDS;
        let ids = MORE_GROUPS
            .iter()
            .flat_map(|group| group.iter())
            .map(|(id, ..)| *id)
            .chain(
                EDIT_GROUPS
                    .iter()
                    .flat_map(|group| group.iter())
                    .map(|(id, ..)| *id),
            )
            .chain(FORMAT_ITEMS.iter().map(|(id, ..)| *id))
            .chain(INSERT_ITEMS.iter().map(|(id, ..)| *id))
            .chain(TAB_CLOSE_ITEMS.iter().map(|(id, ..)| *id))
            .chain(TAB_SPLIT_ITEMS.iter().map(|(id, ..)| *id))
            .chain(TAB_MOVE_ITEMS.iter().map(|((id, ..), _)| *id));
        for id in ids {
            assert!(BUILTIN_COMMANDS.iter().any(|spec| spec.id == id), "{id}");
        }
    }
}
