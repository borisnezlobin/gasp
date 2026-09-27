//! Showing, hiding and focusing the left panel, including hover reveal
//! through the rules engine.

use editor_config::EventKind;
use gpui::{Context, Window};

use super::Workspace;
use super::sidebar::ExecutorClock;

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

    fn run_rule_commands(
        &mut self,
        commands: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for command in commands {
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
