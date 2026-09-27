//! The native menu bar (macOS). Every item dispatches the same
//! `RunCommand` action its key does, so menus show the current shortcut.

use editor_config::commands::BUILTIN_COMMANDS;
use gpui::{App, KeyBinding, Menu, MenuItem, OsAction, SystemMenuType, actions};

use crate::keymap::{Quit, RunCommand};

actions!(
    editor,
    [
        HideApp,
        HideOtherApps,
        ShowAllApps,
        MinimizeWindow,
        ZoomWindow
    ]
);

/// The app's name in the menu bar.
pub const APP_NAME: &str = "Editor";

/// One entry in a menu table.
enum Entry {
    Command(&'static str),
    Separator,
}

use Entry::{Command, Separator};

const FILE: &[Entry] = &[
    Command("note.new"),
    Command("daily.open"),
    Command("tab.new"),
    Separator,
    Command("vault.open"),
    Separator,
    Command("note.rename"),
    Command("note.delete"),
    Separator,
    Command("app.print"),
    Command("app.export"),
    Command("sync.now"),
    Separator,
    Command("tab.reopen"),
    Command("tab.close"),
    Command("pane.close"),
];

const EDIT: &[Entry] = &[
    Command("edit.undo"),
    Command("edit.redo"),
    Separator,
    Command("edit.cut"),
    Command("edit.copy"),
    Command("edit.paste"),
    Command("edit.paste-plain"),
    Command("select.all"),
    Separator,
    Command("template.insert"),
    Separator,
    Command("find.open"),
    Command("find.replace"),
    Command("search.open"),
];

const VIEW: &[Entry] = &[
    Command("sidebar.files.toggle"),
    Command("file-tree.focus"),
    Separator,
    Command("sidebar.right.toggle"),
    Command("sidebar.backlinks"),
    Command("sidebar.outgoing-links"),
    Command("sidebar.outline"),
    Command("sidebar.tags"),
    Separator,
    Command("pane.split-right"),
    Command("pane.split-down"),
    Command("pane.move-tab-left"),
    Command("pane.move-tab-right"),
    Command("pane.move-tab-up"),
    Command("pane.move-tab-down"),
    Command("pane.close"),
    Separator,
    Command("view.zoom-in"),
    Command("view.zoom-out"),
    Command("view.zoom-reset"),
    Command("view.toggle-readable-width"),
];

const GO: &[Entry] = &[
    Command("history.back"),
    Command("history.forward"),
    Separator,
    Command("switcher.open"),
    Command("outline.jump-to-heading"),
    Command("link.follow"),
    Separator,
    Command("tab.next"),
    Command("tab.previous"),
    Separator,
    Command("pane.focus-left"),
    Command("pane.focus-right"),
    Command("pane.focus-up"),
    Command("pane.focus-down"),
];

/// Edit commands the OS also knows, so text fields in native dialogs get
/// them too.
const OS_ACTIONS: [(&str, OsAction); 6] = [
    ("edit.undo", OsAction::Undo),
    ("edit.redo", OsAction::Redo),
    ("edit.cut", OsAction::Cut),
    ("edit.copy", OsAction::Copy),
    ("edit.paste", OsAction::Paste),
    ("select.all", OsAction::SelectAll),
];

/// Sets the menu bar. Only commands `is_available` accepts appear, so the
/// menus never offer something that does nothing.
pub fn set_app_menus(cx: &mut App, is_available: &dyn Fn(&str) -> bool) {
    register_window_actions(cx);
    cx.set_menus(app_menus(is_available));
}

/// Whether a command has a handler in this build: the editor's, the
/// workspace's, or one in `extra`.
pub fn built_in_available<'a>(extra: &'a [&'a str]) -> impl Fn(&str) -> bool + 'a {
    move |id| crate::commands::handles(id) || super::handles(id) || extra.contains(&id)
}

/// The menus, in menu-bar order.
pub fn app_menus(is_available: &dyn Fn(&str) -> bool) -> Vec<Menu> {
    vec![
        app_menu(is_available),
        menu("File", FILE, is_available),
        menu("Edit", EDIT, is_available),
        menu("View", VIEW, is_available),
        menu("Go", GO, is_available),
        window_menu(),
    ]
}

fn app_menu(is_available: &dyn Fn(&str) -> bool) -> Menu {
    let mut items = Vec::new();
    if is_available("settings.open") {
        items.push(command_item("settings.open"));
        items.push(MenuItem::separator());
    }
    items.extend([
        MenuItem::os_submenu("Services", SystemMenuType::Services),
        MenuItem::separator(),
        MenuItem::action(format!("Hide {APP_NAME}"), HideApp),
        MenuItem::action("Hide others", HideOtherApps),
        MenuItem::action("Show all", ShowAllApps),
        MenuItem::separator(),
        MenuItem::action(format!("Quit {APP_NAME}"), Quit),
    ]);
    Menu {
        name: APP_NAME.into(),
        items,
    }
}

fn window_menu() -> Menu {
    Menu {
        name: "Window".into(),
        items: vec![
            MenuItem::action("Minimize", MinimizeWindow),
            MenuItem::action("Zoom", ZoomWindow),
        ],
    }
}

fn menu(name: &'static str, entries: &[Entry], is_available: &dyn Fn(&str) -> bool) -> Menu {
    let mut items: Vec<MenuItem> = Vec::new();
    for entry in entries {
        match entry {
            Command(id) if is_available(id) => items.push(command_item(id)),
            Command(_) => {}
            Separator if matches!(items.last(), Some(MenuItem::Action { .. })) => {
                items.push(MenuItem::separator())
            }
            Separator => {}
        }
    }
    if matches!(items.last(), Some(MenuItem::Separator)) {
        items.pop();
    }
    Menu {
        name: name.into(),
        items,
    }
}

fn command_item(id: &'static str) -> MenuItem {
    let title = command_title(id);
    let action = RunCommand { id: id.into() };
    match OS_ACTIONS.iter().find(|(os_id, _)| *os_id == id) {
        Some((_, os_action)) => MenuItem::os_action(title, action, *os_action),
        None => MenuItem::action(title, action),
    }
}

/// The command's title from the registry.
pub fn command_title(id: &str) -> String {
    BUILTIN_COMMANDS
        .iter()
        .find(|spec| spec.id == id)
        .map_or_else(|| id.to_owned(), |spec| spec.title.to_owned())
}

/// App and window actions the menus use, with the macOS keys for them.
fn register_window_actions(cx: &mut App) {
    cx.on_action(|_: &HideApp, cx| cx.hide());
    cx.on_action(|_: &HideOtherApps, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAllApps, cx| cx.unhide_other_apps());
    cx.on_action(|_: &MinimizeWindow, cx| with_active_window(cx, gpui::Window::minimize_window));
    cx.on_action(|_: &ZoomWindow, cx| with_active_window(cx, gpui::Window::zoom_window));
    bind_window_keys(cx);
}

/// The macOS keys for hiding and minimising, which the OS reserves.
pub fn bind_window_keys(cx: &mut App) {
    if cfg!(target_os = "macos") {
        cx.bind_keys([
            KeyBinding::new("cmd-h", HideApp, None),
            KeyBinding::new("alt-cmd-h", HideOtherApps, None),
            KeyBinding::new("cmd-m", MinimizeWindow, None),
        ]);
    }
}

fn with_active_window(cx: &mut App, act: fn(&gpui::Window)) {
    if let Some(window) = cx.active_window() {
        window.update(cx, |_, window, _| act(window)).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item_names(menu: &Menu) -> Vec<String> {
        menu.items
            .iter()
            .map(|item| match item {
                MenuItem::Action { name, .. } => name.to_string(),
                MenuItem::Separator => "-".to_owned(),
                _ => "*".to_owned(),
            })
            .collect()
    }

    #[test]
    fn menus_leave_out_commands_with_no_handler() {
        let menus = app_menus(&built_in_available(&[]));
        let file = item_names(&menus[1]);
        assert!(file.contains(&"New note".to_owned()));
        assert!(!file.contains(&"Sync now".to_owned()));
        assert!(!file.contains(&"Print".to_owned()));
        assert_ne!(file.last().map(String::as_str), Some("-"));
        let with_sync = app_menus(&built_in_available(&["sync.now"]));
        assert!(item_names(&with_sync[1]).contains(&"Sync now".to_owned()));
    }

    #[test]
    fn menu_items_carry_their_command() {
        let menus = app_menus(&built_in_available(&[]));
        let back = menus[4].items.iter().find_map(|item| match item {
            MenuItem::Action { action, .. } => {
                action.as_any().downcast_ref::<RunCommand>().cloned()
            }
            _ => None,
        });
        assert_eq!(back.unwrap().id.as_ref(), "history.back");
    }
}
