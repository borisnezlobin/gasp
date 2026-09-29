//! Notices: one line about something that happened out of sight, such as
//! a note that couldn't be saved or one that went to the trash, shown at
//! the window's bottom right over the notes.
//!
//! A notice that reports something done leaves after
//! `UiTheme::notice_duration`; one about a problem, or one offering
//! something, stays until it's dismissed, so a failed save can't slip by. A notice can carry one
//! follow-up, which is a command, so the keyboard reaches it through the
//! palette too. Anything with an `App` can post one.

use gpui::{AnyElement, AnyWindowHandle, App, Global, SharedString, Window, div, prelude::*};

use crate::icons::{IconName, icon};
use crate::keymap::RunCommand;
use crate::theme::UiTheme;
use crate::ui::{Button, IconButton, Selectable};

/// Notices shown at once; a new one pushes out the oldest.
pub const MAX_SHOWN: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeKind {
    /// Something finished, such as an export or a note going to the trash.
    Done,
    /// Something failed and the person should know.
    Problem,
    /// Something the person may want to do, such as importing settings.
    Offer,
}

/// A command a notice offers, with its button's label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoticeAction {
    pub label: SharedString,
    pub command: SharedString,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub message: SharedString,
    pub action: Option<NoticeAction>,
}

impl Notice {
    pub fn done(message: impl Into<SharedString>) -> Notice {
        Notice {
            kind: NoticeKind::Done,
            message: message.into(),
            action: None,
        }
    }

    pub fn problem(message: impl Into<SharedString>) -> Notice {
        Notice {
            kind: NoticeKind::Problem,
            message: message.into(),
            action: None,
        }
    }

    pub fn offer(message: impl Into<SharedString>) -> Notice {
        Notice {
            kind: NoticeKind::Offer,
            message: message.into(),
            action: None,
        }
    }

    /// Offers `command` on a button labelled `label`.
    pub fn with_action(
        mut self,
        label: impl Into<SharedString>,
        command: impl Into<SharedString>,
    ) -> Notice {
        self.action = Some(NoticeAction {
            label: label.into(),
            command: command.into(),
        });
        self
    }
}

struct Shown {
    id: u64,
    notice: Notice,
    /// The window it belongs to; `None` shows it in every window.
    window: Option<AnyWindowHandle>,
}

#[derive(Default)]
struct Notices {
    next_id: u64,
    shown: Vec<Shown>,
}

impl Global for Notices {}

/// Shows `notice` in every window. Returns its id, for [`dismiss`].
pub fn show(notice: Notice, cx: &mut App) -> u64 {
    post(notice, None, cx)
}

/// Shows `notice` in `window` only.
pub fn show_in(notice: Notice, window: &Window, cx: &mut App) -> u64 {
    post(notice, Some(window.window_handle()), cx)
}

fn post(notice: Notice, window: Option<AnyWindowHandle>, cx: &mut App) -> u64 {
    if notice.kind == NoticeKind::Problem {
        eprintln!("{}", notice.message);
    }
    let expires = notice.kind == NoticeKind::Done;
    let duration = crate::ui::ui_theme(cx).notice_duration;
    let notices = cx.default_global::<Notices>();
    notices
        .shown
        .retain(|shown| shown.notice.message != notice.message);
    let id = notices.next_id;
    notices.next_id += 1;
    notices.shown.push(Shown { id, notice, window });
    if notices.shown.len() > MAX_SHOWN {
        notices.shown.remove(0);
    }
    if expires {
        cx.spawn(async move |cx| {
            cx.background_executor().timer(duration).await;
            cx.update(|cx| dismiss(id, cx)).ok();
        })
        .detach();
    }
    id
}

/// Shows a problem in every window.
pub fn problem(message: impl Into<SharedString>, cx: &mut App) -> u64 {
    show(Notice::problem(message), cx)
}

/// Says `path` couldn't be opened, and why.
pub fn open_failed(path: &std::path::Path, error: impl std::fmt::Display, cx: &mut App) -> u64 {
    let name = path
        .file_stem()
        .unwrap_or(path.as_os_str())
        .to_string_lossy();
    problem(format!("Couldn’t open “{name}”: {error}"), cx)
}

/// Takes notice `id` away, if it's still shown.
pub fn dismiss(id: u64, cx: &mut App) {
    let Some(notices) = cx.try_global::<Notices>() else {
        return;
    };
    if notices.shown.iter().any(|shown| shown.id == id) {
        cx.global_mut::<Notices>()
            .shown
            .retain(|shown| shown.id != id);
    }
}

/// The notices `window` shows, oldest first, with their ids.
pub fn shown_in(window: AnyWindowHandle, cx: &App) -> Vec<(u64, Notice)> {
    let Some(notices) = cx.try_global::<Notices>() else {
        return Vec::new();
    };
    notices
        .shown
        .iter()
        .filter(|shown| shown.window.is_none_or(|owner| owner == window))
        .map(|shown| (shown.id, shown.notice.clone()))
        .collect()
}

/// Re-renders `view` whenever a notice comes or goes.
pub fn observe<V: 'static>(cx: &mut gpui::Context<V>) -> gpui::Subscription {
    cx.observe_global::<Notices>(|_, cx| cx.notify())
}

/// The window's notices stacked at its bottom right, `bottom` above its
/// edge, or nothing when there are none.
pub fn render(window: &Window, bottom: gpui::Pixels, cx: &mut App) -> Option<AnyElement> {
    let shown = shown_in(window.window_handle(), cx);
    if shown.is_empty() {
        return None;
    }
    let ui = crate::ui::ui_theme(cx);
    let cards = shown
        .into_iter()
        .map(|(id, notice)| render_card(id, notice, &ui))
        .collect::<Vec<_>>();
    Some(
        div()
            .absolute()
            .right(ui.surface_gap + ui.space_md)
            .bottom(bottom)
            .flex()
            .flex_col()
            .items_end()
            .gap(ui.space_sm)
            .w(ui.notice_width)
            .max_w_full()
            .children(cards)
            .into_any_element(),
    )
}

fn render_card(id: u64, notice: Notice, ui: &UiTheme) -> AnyElement {
    let (glyph, tint) = match notice.kind {
        NoticeKind::Done => (IconName::CheckCircle, ui.icon),
        NoticeKind::Problem => (IconName::WarningCircle, ui.error),
        NoticeKind::Offer => (IconName::Info, ui.icon),
    };
    let kind = match notice.kind {
        NoticeKind::Done => "done",
        NoticeKind::Problem => "problem",
        NoticeKind::Offer => "offer",
    };
    let action = notice.action.map(|action| {
        let command = action.command.clone();
        Button::new(("notice-action", id as usize), action.label).on_click(move |_, window, cx| {
            dismiss(id, cx);
            window.dispatch_action(
                Box::new(RunCommand {
                    id: command.clone(),
                }),
                cx,
            );
        })
    });
    crate::ui::popover(ui)
        .id(("notice", id as usize))
        .selector(move || format!("notice-{kind}"))
        .w_full()
        .flex_row()
        .items_center()
        .gap(ui.space_md)
        .py(ui.space_sm)
        .pl(ui.space_md)
        .pr(ui.space_sm)
        .occlude()
        .child(icon(glyph).flex_none().size(ui.icon_size).text_color(tint))
        .child(div().flex_1().min_w_0().child(notice.message))
        .children(action)
        .child(
            IconButton::new(format!("notice-dismiss-{id}"), IconName::X)
                .small()
                .tooltip("Dismiss")
                .on_click(move |_, _, cx| dismiss(id, cx)),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn a_repeated_message_shows_once_and_the_oldest_makes_room(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            for index in 0..MAX_SHOWN {
                show(Notice::problem(format!("problem {index}")), cx);
            }
            show(Notice::problem("problem 1"), cx);
            show(Notice::problem("problem 9"), cx);
            let notices = cx.global::<Notices>();
            let messages: Vec<&str> = notices
                .shown
                .iter()
                .map(|shown| shown.notice.message.as_ref())
                .collect();
            assert_eq!(messages, vec!["problem 2", "problem 1", "problem 9"]);
        });
    }
}
