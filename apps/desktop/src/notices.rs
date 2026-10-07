//! Notices: one line about something that happened out of sight, such as
//! a note that couldn't be saved or one that went to the trash, shown at
//! the window's bottom right over the notes.
//!
//! A notice that reports something done leaves after
//! `UiTheme::notice_duration`; one about a problem, or one offering
//! something, stays until it's dismissed, so a failed save can't slip by. A notice can carry one
//! follow-up, which is a command, so the keyboard reaches it through the
//! palette too, and a quieter second one beside it, such as a link. A
//! notice about work under way can show how far along it is, and
//! [`replace`] changes it in place as the work goes on. Anything with an
//! `App` can post one.

use gpui::{
    AnyElement, AnyWindowHandle, App, Global, SharedString, Window, div, prelude::*, relative,
};

use crate::icons::{IconName, icon};
use crate::keymap::RunCommand;
use crate::plain_errors::PlainReason;
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
    /// A quieter line under the message: for a problem, why it happened
    /// and what to do.
    pub detail: Option<SharedString>,
    pub action: Option<NoticeAction>,
    /// A quieter follow-up shown before the action.
    pub link: Option<NoticeAction>,
    /// How far along the work it describes is, in percent.
    pub progress: Option<u8>,
}

impl Notice {
    pub fn done(message: impl Into<SharedString>) -> Notice {
        Notice {
            kind: NoticeKind::Done,
            message: message.into(),
            detail: None,
            action: None,
            link: None,
            progress: None,
        }
    }

    pub fn problem(message: impl Into<SharedString>) -> Notice {
        Notice {
            kind: NoticeKind::Problem,
            message: message.into(),
            detail: None,
            action: None,
            link: None,
            progress: None,
        }
    }

    pub fn offer(message: impl Into<SharedString>) -> Notice {
        Notice {
            kind: NoticeKind::Offer,
            message: message.into(),
            detail: None,
            action: None,
            link: None,
            progress: None,
        }
    }

    /// Adds a quieter line under the message.
    pub fn with_detail(mut self, detail: impl Into<SharedString>) -> Notice {
        self.detail = Some(detail.into());
        self
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

    /// Offers `command` on a quiet button labelled `label`, before the
    /// action.
    pub fn with_link(
        mut self,
        label: impl Into<SharedString>,
        command: impl Into<SharedString>,
    ) -> Notice {
        self.link = Some(NoticeAction {
            label: label.into(),
            command: command.into(),
        });
        self
    }

    /// Shows a bar `percent` of the way along.
    pub fn with_progress(mut self, percent: u8) -> Notice {
        self.progress = Some(percent.min(100));
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
        dismiss_after(id, duration, cx);
    }
    id
}

fn dismiss_after(id: u64, duration: std::time::Duration, cx: &mut App) {
    cx.spawn(async move |cx| {
        cx.background_executor().timer(duration).await;
        cx.update(|cx| dismiss(id, cx)).ok();
    })
    .detach();
}

/// Puts `notice` where notice `id` is, keeping its place and its window,
/// or shows it in every window when `id` is gone or `None`. Returns the
/// id it's shown under.
pub fn replace(id: Option<u64>, notice: Notice, cx: &mut App) -> u64 {
    let Some(id) = id.filter(|id| is_shown(*id, cx)) else {
        return show(notice, cx);
    };
    let expires = notice.kind == NoticeKind::Done;
    if let Some(shown) = cx
        .global_mut::<Notices>()
        .shown
        .iter_mut()
        .find(|shown| shown.id == id)
    {
        shown.notice = notice;
    }
    if expires {
        let duration = crate::ui::ui_theme(cx).notice_duration;
        dismiss_after(id, duration, cx);
    }
    id
}

/// Whether notice `id` is still shown.
pub fn is_shown(id: u64, cx: &App) -> bool {
    cx.try_global::<Notices>()
        .is_some_and(|notices| notices.shown.iter().any(|shown| shown.id == id))
}

/// Shows a problem in every window.
pub fn problem(message: impl Into<SharedString>, cx: &mut App) -> u64 {
    show(Notice::problem(message), cx)
}

/// A problem saying `what` failed, with the plain reason for `error`
/// under it. The error's own wording goes to the log.
pub fn failure(what: impl Into<SharedString>, error: impl PlainReason) -> Notice {
    let what = what.into();
    eprintln!("{what}: {error}");
    Notice::problem(what).with_detail(error.plain_reason())
}

/// Shows that `what` failed, and why in plain words.
pub fn failed(what: impl Into<SharedString>, error: impl PlainReason, cx: &mut App) -> u64 {
    show(failure(what, error), cx)
}

/// Says `path` couldn't be opened, and why.
pub fn open_failed(path: &std::path::Path, error: impl PlainReason, cx: &mut App) -> u64 {
    let name = path
        .file_stem()
        .unwrap_or(path.as_os_str())
        .to_string_lossy();
    failed(format!("Couldn’t open “{name}”"), error, cx)
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
    let action = notice
        .action
        .map(|action| command_button(("notice-action", id as usize), id, action, false));
    let link = notice
        .link
        .map(|link| command_button(("notice-link", id as usize), id, link, true));
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
        .relative()
        .occlude()
        .child(icon(glyph).flex_none().size(ui.icon_size).text_color(tint))
        .child(render_text(notice.message, notice.detail, ui))
        .children(link)
        .children(action)
        .child(
            IconButton::new(format!("notice-dismiss-{id}"), IconName::X)
                .small()
                .tooltip("Dismiss")
                .on_click(move |_, _, cx| dismiss(id, cx)),
        )
        .children(notice.progress.map(|percent| render_progress(percent, ui)))
        .into_any_element()
}

/// The message, and under it the detail in a smaller, quieter line.
fn render_text(message: SharedString, detail: Option<SharedString>, ui: &UiTheme) -> gpui::Div {
    let headline = div().when(detail.is_some(), |headline| headline.font_weight(ui.strong_weight));
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(ui.space_xs)
        .child(headline.child(message))
        .children(detail.map(|detail| {
            div()
                .text_size(ui.small_font_size)
                .text_color(ui.text_muted)
                .child(detail)
        }))
}

/// A button that takes the notice away and runs `action`'s command; the
/// link shows quietly, the action on a fill.
fn command_button(
    element_id: (&'static str, usize),
    id: u64,
    action: NoticeAction,
    quiet: bool,
) -> Button {
    let command = action.command;
    let button = Button::new(element_id, action.label);
    let button = if quiet { button.quiet() } else { button };
    button.on_click(move |_, window, cx| {
        dismiss(id, cx);
        window.dispatch_action(
            Box::new(RunCommand {
                id: command.clone(),
            }),
            cx,
        );
    })
}

/// A thin bar along the card's bottom edge, inside its padding and clear
/// of its rounded corners, so the card keeps its size as the bar fills.
fn render_progress(percent: u8, ui: &UiTheme) -> AnyElement {
    div()
        .absolute()
        .left(ui.menu_radius)
        .right(ui.menu_radius)
        .bottom(ui.space_xs)
        .h(ui.notice_progress_height)
        .rounded_full()
        .bg(ui.menu_separator)
        .child(
            div()
                .h_full()
                .w(relative(f32::from(percent) / 100.))
                .rounded_full()
                .bg(ui.accent),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_says_what_failed_and_why_in_plain_words() {
        let error = std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "stream did not contain valid UTF-8",
        );
        let notice = failure("Couldn’t open “CR5”", error);
        assert_eq!(notice.kind, NoticeKind::Problem);
        assert_eq!(notice.message.as_ref(), "Couldn’t open “CR5”");
        let detail = notice.detail.expect("a reason under the headline");
        assert_eq!(detail.as_ref(), "It isn’t a file Gasp can read.");
        assert!(!detail.contains("UTF-8"));
    }

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

    #[gpui::test]
    fn replacing_a_notice_keeps_its_place(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let first = show(Notice::offer("Downloading").with_progress(10), cx);
            show(Notice::problem("Something else"), cx);
            let same = replace(
                Some(first),
                Notice::offer("Downloading").with_progress(60),
                cx,
            );
            assert_eq!(same, first);
            let notices = cx.global::<Notices>();
            assert_eq!(notices.shown.len(), 2);
            assert_eq!(notices.shown[0].notice.progress, Some(60));
            dismiss(first, cx);
            assert!(!is_shown(first, cx));
            let again = replace(Some(first), Notice::offer("Ready"), cx);
            assert_ne!(again, first);
            assert!(is_shown(again, cx));
        });
    }
}
