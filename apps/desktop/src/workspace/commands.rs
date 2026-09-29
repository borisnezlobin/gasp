//! The commands the workspace runs, looked up by id.
//!
//! Commands with no feature yet (such as `sync.now` or `search.open`)
//! aren't here, so their keys do nothing rather than pretend to work until
//! something registers them with [`Workspace::on_command`].

use gpui::{Context, Window};

use super::Workspace;
use super::pane_tree::{Axis, Direction};
use crate::keymap::RunCommand;

type Handler = fn(&mut Workspace, &mut Window, &mut Context<Workspace>);

const TAB_GO_PREFIX: &str = "tab.go-";

const HANDLERS: [(&str, Handler); 34] = [
    ("tab.new", |ws, window, cx| ws.new_tab(window, cx)),
    ("tab.close", |ws, window, cx| {
        ws.close_active_tab(window, cx)
    }),
    ("tab.reopen", |ws, window, cx| {
        ws.reopen_closed_tab(window, cx)
    }),
    ("tab.next", |ws, window, cx| ws.cycle_tab(1, window, cx)),
    ("tab.previous", |ws, window, cx| {
        ws.cycle_tab(-1, window, cx)
    }),
    ("pane.split-right", |ws, window, cx| {
        ws.split_pane(Axis::Row, true, window, cx);
    }),
    ("pane.split-down", |ws, window, cx| {
        ws.split_pane(Axis::Column, true, window, cx);
    }),
    ("pane.close", |ws, window, cx| {
        ws.close_active_pane(window, cx)
    }),
    ("pane.focus-left", |ws, window, cx| {
        ws.focus_pane_toward(Direction::Left, window, cx)
    }),
    ("pane.focus-right", |ws, window, cx| {
        ws.focus_pane_toward(Direction::Right, window, cx)
    }),
    ("pane.focus-up", |ws, window, cx| {
        ws.focus_pane_toward(Direction::Up, window, cx)
    }),
    ("pane.focus-down", |ws, window, cx| {
        ws.focus_pane_toward(Direction::Down, window, cx)
    }),
    ("pane.move-tab-left", |ws, window, cx| {
        ws.move_active_tab(Direction::Left, window, cx)
    }),
    ("pane.move-tab-right", |ws, window, cx| {
        ws.move_active_tab(Direction::Right, window, cx)
    }),
    ("pane.move-tab-up", |ws, window, cx| {
        ws.move_active_tab(Direction::Up, window, cx)
    }),
    ("pane.move-tab-down", |ws, window, cx| {
        ws.move_active_tab(Direction::Down, window, cx)
    }),
    ("tab.close-others", |ws, window, cx| {
        let pane = ws.active_pane.clone();
        let keep = pane.read(cx).active_index();
        ws.close_other_tabs(&pane, keep, window, cx)
    }),
    ("tab.close-right", |ws, window, cx| {
        let pane = ws.active_pane.clone();
        let index = pane.read(cx).active_index();
        ws.close_tabs_right(&pane, index, window, cx)
    }),
    ("history.back", |ws, window, cx| {
        ws.navigate_history(false, window, cx)
    }),
    ("history.forward", |ws, window, cx| {
        ws.navigate_history(true, window, cx)
    }),
    ("note.new", |ws, window, cx| {
        if let Err(error) = ws.new_note(window, cx) {
            crate::notices::problem(format!("Couldn’t make a note: {error}"), cx);
        }
    }),
    ("note.rename", |ws, window, cx| ws.focus_title(window, cx)),
    ("note.delete", |ws, window, cx| {
        ws.delete_active_note(window, cx)
    }),
    ("sidebar.files.toggle", |ws, window, cx| {
        ws.toggle_left_panel(window, cx)
    }),
    ("sidebar.files.show", |ws, _, cx| ws.show_left_panel(cx)),
    ("sidebar.files.hide", |ws, window, cx| {
        ws.hide_left_panel(window, cx)
    }),
    ("toolbar.focus", |ws, window, cx| {
        ws.focus_toolbars(window, cx)
    }),
    ("file-tree.focus", |ws, window, cx| {
        ws.focus_left_panel(window, cx)
    }),
    ("vault.open", |ws, window, cx| {
        ws.prompt_for_vault(window, cx)
    }),
    ("vault.switch", |ws, window, cx| {
        ws.show_left_panel(cx);
        ws.open_vault_menu(window, cx)
    }),
    ("help.shortcuts", |ws, window, cx| {
        ws.toggle_help(window, cx)
    }),
    ("file-tree.new-folder", |ws, window, cx| {
        ws.new_folder_in_tree(window, cx)
    }),
    ("file-tree.sort", |ws, window, cx| {
        ws.show_left_panel(cx);
        ws.open_sort_menu(window, cx)
    }),
    ("file-tree.collapse-all", |ws, _, cx| ws.collapse_tree(cx)),
];

/// Whether the workspace runs `id` itself.
pub fn handles(id: &str) -> bool {
    HANDLERS.iter().any(|(name, _)| *name == id) || tab_number(id).is_some()
}

/// The N in `tab.go-N`.
fn tab_number(id: &str) -> Option<usize> {
    let number: usize = id.strip_prefix(TAB_GO_PREFIX)?.parse().ok()?;
    (1..=9).contains(&number).then_some(number)
}

impl Workspace {
    /// Runs a command by id. Returns false when nothing here knows it.
    pub fn run_command(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(handler) = self.extra_commands.get(id).cloned() {
            handler(self, window, cx);
            return true;
        }
        if let Some(number) = tab_number(id) {
            self.go_to_tab(number, window, cx);
            return true;
        }
        match HANDLERS.iter().find(|(name, _)| *name == id) {
            Some((_, handler)) => {
                handler(self, window, cx);
                true
            }
            None => false,
        }
    }

    pub(crate) fn on_run_command(
        &mut self,
        action: &RunCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.run_command(&action.id, window, cx) {
            cx.propagate();
        }
    }
}

#[cfg(test)]
mod tests {
    use gasp_config::commands::BUILTIN_COMMANDS;

    use super::*;

    #[test]
    fn tab_numbers_parse() {
        assert_eq!(tab_number("tab.go-1"), Some(1));
        assert_eq!(tab_number("tab.go-9"), Some(9));
        assert_eq!(tab_number("tab.go-0"), None);
        assert_eq!(tab_number("tab.new"), None);
    }

    #[test]
    fn every_handled_command_exists_in_the_registry() {
        for (id, _) in HANDLERS {
            assert!(
                BUILTIN_COMMANDS.iter().any(|spec| spec.id == id),
                "{id} isn’t a built-in command"
            );
        }
    }
}
