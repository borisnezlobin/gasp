//! The in-window dialog for platforms with no native one (Linux). Arrows
//! and Tab move between the answers, Enter or Space picks one, and Escape
//! picks Cancel.

use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, KeyDownEvent, PromptButton, PromptHandle,
    PromptLevel, PromptResponse, RenderablePromptHandle, SharedString, Window, div, prelude::*,
};

use crate::ui::{Button, ui_theme};

pub struct PromptView {
    focus_handle: FocusHandle,
    message: SharedString,
    detail: Option<SharedString>,
    answers: Vec<PromptButton>,
    selected: usize,
}

impl EventEmitter<PromptResponse> for PromptView {}

impl Focusable for PromptView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// Uses [`PromptView`] for `window.prompt` where the OS has no dialog of
/// its own. macOS and Windows keep their native dialogs.
pub fn use_in_window_prompts(cx: &mut App) {
    if cfg!(any(target_os = "linux", target_os = "freebsd")) {
        cx.set_prompt_builder(build_prompt);
    }
}

fn build_prompt(
    _: PromptLevel,
    message: &str,
    detail: Option<&str>,
    answers: &[PromptButton],
    handle: PromptHandle,
    window: &mut Window,
    cx: &mut App,
) -> RenderablePromptHandle {
    let view = cx.new(|cx| PromptView::new(message, detail, answers.to_vec(), cx));
    handle.with_view(view, window, cx)
}

impl PromptView {
    pub fn new(
        message: &str,
        detail: Option<&str>,
        answers: Vec<PromptButton>,
        cx: &mut Context<Self>,
    ) -> Self {
        PromptView {
            focus_handle: cx.focus_handle(),
            message: message.to_owned().into(),
            detail: detail.map(|detail| detail.to_owned().into()),
            answers,
            selected: 0,
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// The answer Escape picks: Cancel if there is one, or else the last.
    fn cancel_index(&self) -> usize {
        self.answers
            .iter()
            .position(|answer| matches!(answer, PromptButton::Cancel(_)))
            .unwrap_or(self.answers.len().saturating_sub(1))
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.answers.len().max(1) as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(count) as usize;
        cx.notify();
    }

    pub fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let backward = event.keystroke.modifiers.shift;
        match event.keystroke.key.as_str() {
            "tab" => self.step(if backward { -1 } else { 1 }, cx),
            "down" | "right" => self.step(1, cx),
            "up" | "left" => self.step(-1, cx),
            "enter" | "space" => cx.emit(PromptResponse(self.selected)),
            "escape" => cx.emit(PromptResponse(self.cancel_index())),
            _ => return,
        }
        cx.stop_propagation();
    }

    /// The answer drawn as the one the dialog expects: the first, unless
    /// it's Cancel.
    fn is_primary(&self, index: usize) -> bool {
        index == 0 && !matches!(self.answers[index], PromptButton::Cancel(_))
    }

    fn render_answer(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let button = Button::new(("answer", index), self.answers[index].label().clone())
            .focused(index == self.selected)
            .on_click(cx.listener(move |_, _, _, cx| cx.emit(PromptResponse(index))));
        if self.is_primary(index) {
            button.primary()
        } else {
            button
        }
    }
}

impl Render for PromptView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = ui_theme(cx);
        let answers: Vec<_> = (0..self.answers.len())
            .map(|index| self.render_answer(index, cx).into_any_element())
            .collect();
        let dialog = crate::ui::dialog(&ui)
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .gap(ui.space_md)
            .w(ui.small_dialog_width)
            .p(ui.space_xl)
            .child(
                div()
                    .text_size(ui.font_size + gpui::px(1.))
                    .child(self.message.clone()),
            )
            .children(
                self.detail
                    .clone()
                    .map(|detail| div().text_color(ui.text_muted).child(detail)),
            )
            // One answer under another, full width, so long answers such
            // as "Use the version on disk" never wrap into a ragged row.
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(ui.space_sm)
                    .pt(ui.space_md)
                    .children(answers),
            );
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .px(ui.surface_gap)
            .pt(ui.dialog_top_offset)
            .bg(ui.backdrop)
            .child(dialog)
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Entity, TestAppContext};

    use super::*;

    #[gpui::test]
    fn keys_move_between_answers_and_pick_one(cx: &mut TestAppContext) {
        let answers = vec![
            PromptButton::new("Keep mine"),
            PromptButton::new("Use theirs"),
            PromptButton::cancel("Cancel"),
        ];
        let (view, cx) = cx.add_window_view(|window, cx| {
            let view = PromptView::new("Changed on disk", None, answers, cx);
            window.focus(&view.focus_handle);
            view
        });
        let picked = std::rc::Rc::new(std::cell::Cell::new(None));
        let seen = picked.clone();
        cx.update(|_, cx| {
            cx.subscribe(
                &view,
                move |_: Entity<PromptView>, event: &PromptResponse, _| seen.set(Some(event.0)),
            )
            .detach()
        });
        cx.simulate_keystrokes("down down down");
        assert_eq!(view.read_with(cx, |view, _| view.selected()), 0);
        cx.simulate_keystrokes("shift-tab enter");
        assert_eq!(picked.get(), Some(2));
        cx.simulate_keystrokes("right escape");
        assert_eq!(picked.get(), Some(2));
        cx.simulate_keystrokes("left left enter");
        assert_eq!(picked.get(), Some(1));
    }
}
