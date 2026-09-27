//! The in-window dialog for platforms with no native one (Linux). Arrows
//! and Tab move between the answers, Enter or Space picks one, and Escape
//! picks Cancel.

use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, KeyDownEvent, PromptButton, PromptHandle,
    PromptLevel, PromptResponse, RenderablePromptHandle, SharedString, Window, div, prelude::*,
};

use crate::theme::Theme;

pub struct PromptView {
    focus_handle: FocusHandle,
    message: SharedString,
    detail: Option<SharedString>,
    answers: Vec<PromptButton>,
    selected: usize,
    theme: Theme,
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
            theme: Theme::default(),
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

    fn render_answer(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme.workspace;
        let selected = index == self.selected;
        div()
            .id(("answer", index))
            .flex()
            .justify_center()
            .px(theme.space_lg)
            .py(theme.space_sm)
            .rounded(theme.radius_md)
            .when(selected, |answer| {
                answer.bg(theme.accent).text_color(theme.on_accent)
            })
            .when(!selected, |answer| {
                answer
                    .bg(theme.list_hover_background)
                    .text_color(theme.text)
                    .hover(|style| style.bg(theme.hover_background))
            })
            .on_click(cx.listener(move |_, _, _, cx| cx.emit(PromptResponse(index))))
            .child(self.answers[index].label().clone())
    }
}

impl Render for PromptView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.workspace.clone();
        let answers: Vec<_> = (0..self.answers.len())
            .map(|index| self.render_answer(index, cx).into_any_element())
            .collect();
        let dialog = div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .flex()
            .flex_col()
            .gap(theme.space_md)
            .w(theme.launcher_width)
            .p(theme.space_xl)
            .rounded(theme.radius_lg)
            .bg(self.theme.background)
            .shadow(vec![gpui::BoxShadow {
                color: theme.shadow,
                offset: gpui::point(gpui::px(0.), theme.shadow_offset),
                blur_radius: theme.shadow_blur,
                spread_radius: gpui::px(0.),
            }])
            .child(div().text_color(theme.text).child(self.message.clone()))
            .children(
                self.detail
                    .clone()
                    .map(|detail| div().text_color(theme.text_muted).child(detail)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(theme.space_sm)
                    .pt(theme.space_sm)
                    .children(answers),
            );
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .pt(theme.modal_top_offset)
            .bg(theme.backdrop)
            .font_family(self.theme.body_font_family)
            .text_size(theme.ui_font_size)
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
