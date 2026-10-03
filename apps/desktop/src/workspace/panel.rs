//! Showing, hiding and focusing the left panel, including hover reveal
//! through the rules engine.

use gasp_config::EventKind;
use gpui::{App, Context, Pixels, Point, Window};

use super::sidebar::{ExecutorClock, PANEL_TARGET, PanelHolds};
use super::sidebar_chrome::{SORT_KEY, VAULT_KEY};
use super::{Drag, Workspace};

/// The command the hover rules hide the panel with.
const HIDE_COMMAND: &str = "sidebar.files.hide";

impl Workspace {
    pub(crate) fn toggle_left_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.left_panel.is_visible() {
            self.hide_left_panel(window, cx);
        } else {
            self.left_panel.toggle();
            cx.notify();
        }
    }

    pub(crate) fn show_left_panel(&mut self, cx: &mut Context<Self>) {
        self.left_panel.show();
        cx.notify();
    }

    /// Hides the panel, moving focus to the note if the panel had it.
    pub(crate) fn hide_left_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let had_focus = self
            .left_panel
            .focus_handle()
            .is_some_and(|focus| focus.contains_focused(window, cx));
        self.left_panel.hide();
        if self.left_panel.is_visible() {
            // Pinned open by the toggle while it was hover-revealed.
            self.left_panel.set_pinned(false);
        }
        if had_focus {
            self.focus_active(window, cx);
        }
        cx.notify();
    }

    /// Escape in the panel: back to the note. A panel that was only
    /// showing for now, revealed or covering the note, hides with it.
    pub fn leave_left_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.left_panel.is_passing() {
            self.hide_left_panel(window, cx);
        }
        self.focus_active(window, cx);
    }

    /// `file-tree.focus`: shows the panel and gives it the keyboard.
    pub(crate) fn focus_left_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.left_panel.show();
        if let Some(focus) = self.left_panel.focus_handle().cloned() {
            window.focus(&focus);
        }
        cx.notify();
    }

    /// The pointer entered or left a rule target.
    pub(crate) fn pointer_event(
        &mut self,
        kind: EventKind,
        target: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let clock = ExecutorClock::new(cx.background_executor().clone());
        let clock = self.rule_clock.get_or_insert(clock);
        let commands = self.left_panel.pointer_event(kind, target, clock);
        self.run_rule_commands(commands, window, cx);
        self.schedule_rule_tick(window, cx);
    }

    /// The pointer moved, was released or left the window at `pointer`:
    /// the panel and the bars shown on hover stay or start to hide.
    pub(crate) fn follow_pointer(
        &mut self,
        pointer: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.left_panel.set_pointer(pointer);
        self.refresh_panel_use(window, cx);
        self.follow_pointer_for_bars(pointer, window, cx);
    }

    /// Tells the rules if the panel came into use or went out of it, with
    /// the pointer where it was last seen.
    pub(crate) fn refresh_panel_use(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let in_use = self.left_panel.is_visible() && self.left_panel_holds(window, cx).any();
        if let Some(kind) = self.left_panel.set_in_use(in_use) {
            self.pointer_event(kind, PANEL_TARGET, window, cx);
        }
    }

    /// What keeps the panel from hiding now.
    pub fn left_panel_holds(&self, window: &Window, cx: &App) -> PanelHolds {
        let pointer = self
            .left_panel
            .pointer()
            .is_some_and(|pointer| self.left_panel.area().contains(pointer));
        let focus = self
            .left_panel
            .focus_handle()
            .is_some_and(|focus| focus.contains_focused(window, cx));
        let carried = cx.has_active_drag() && self.left_panel.in_use();
        PanelHolds {
            pointer,
            focus,
            menu: self.left_panel_menu_open(cx),
            drag: self.drag == Some(Drag::Sidebar) || carried,
        }
    }

    /// Whether a menu or prompt the panel opened is showing.
    fn left_panel_menu_open(&self, cx: &App) -> bool {
        let own = [SORT_KEY, VAULT_KEY]
            .iter()
            .any(|key| self.menu.is_open_at(key));
        let tree = self.file_tree.as_ref().is_some_and(|tree| {
            let tree = tree.read(cx);
            tree.context_menu_items().is_some() || !tree.pending_trash().is_empty()
        });
        own || tree
    }

    /// Runs what the rules asked for, except hiding the panel while it's
    /// in use: the hide was asked for when the pointer left, and the
    /// pointer may be back, or a menu may have opened, since.
    fn run_rule_commands(
        &mut self,
        commands: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for command in commands {
            if command == HIDE_COMMAND && self.left_panel_holds(window, cx).any() {
                self.left_panel.set_in_use(true);
                continue;
            }
            self.run_command(&command, window, cx);
        }
    }

    /// Wakes up when the next delayed rule is due.
    fn schedule_rule_tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(clock) = self.rule_clock.as_ref() else {
            return;
        };
        let Some(delay) = self.left_panel.next_deadline(clock) else {
            self.left_panel.tick = None;
            return;
        };
        let task = cx.spawn_in(window, async move |workspace, cx| {
            cx.background_executor().timer(delay).await;
            workspace
                .update_in(cx, |workspace, window, cx| {
                    let Some(clock) = workspace.rule_clock.as_ref() else {
                        return;
                    };
                    let commands = workspace.left_panel.tick(clock);
                    workspace.run_rule_commands(commands, window, cx);
                    workspace.schedule_rule_tick(window, cx);
                })
                .ok();
        });
        self.left_panel.tick = Some(task);
    }
}
