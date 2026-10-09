//! The update notices: what a check says, how often it says it, and how
//! an update in progress shows. Nothing here reaches the network or runs
//! `hdiutil`; the checks' answers and the install's progress are handed
//! straight to the notices.

#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::path::PathBuf;

use gasp_desktop::keymap::RunCommand;
use gasp_desktop::notices::{self, Notice, NoticeKind};
use gasp_desktop::update::install::{InstallError, Prepared};
use gasp_desktop::update::release::{Release, running_version};
use gasp_desktop::update::verify::Refusal;
use gasp_desktop::update::{
    self, CheckOutcome, INSTALL_COMMAND, InstallProgress, NOTES_COMMAND, RESTART_COMMAND,
};
use gpui::{
    Context, IntoElement, ParentElement, Render, Styled, TestAppContext, VisualTestContext, Window,
    div, px,
};
use semver::Version;

fn release(version: &str) -> Release {
    Release {
        version: Version::parse(version).unwrap(),
        url: "https://example.com/Gasp.dmg".into(),
        notes: "https://example.com/notes".into(),
        published: "2026-10-01T09:00:00Z".into(),
        size: 1,
        sha256: None,
    }
}

fn window(cx: &mut TestAppContext) -> &mut VisualTestContext {
    cx.update(update::register_commands);
    cx.add_empty_window()
}

fn shown(cx: &mut VisualTestContext) -> Vec<Notice> {
    cx.update(|window, cx| notices::shown_in(window.window_handle(), cx))
        .into_iter()
        .map(|(_, notice)| notice)
        .collect()
}

fn dismiss_all(cx: &mut VisualTestContext) {
    let ids: Vec<u64> = cx
        .update(|window, cx| notices::shown_in(window.window_handle(), cx))
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    cx.update(|_, cx| ids.into_iter().for_each(|id| notices::dismiss(id, cx)));
}

#[gpui::test]
fn checking_from_the_menu_outside_an_app_bundle_says_why_not(cx: &mut TestAppContext) {
    let cx = window(cx);
    cx.dispatch_action(RunCommand {
        id: "app.check-for-updates".into(),
    });
    let shown = shown(cx);
    assert_eq!(shown.len(), 1);
    assert_eq!(
        shown[0].message.as_ref(),
        "Gasp can only update itself when it runs as an installed app."
    );
}

/// The palette runs commands inside its window's update, and the check
/// reads every window's settings: it waits for the update to end rather
/// than read its own window mid-update, which GPUI refuses.
#[gpui::test]
fn checking_from_inside_a_vault_window_reads_its_settings(cx: &mut TestAppContext) {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("a.md"), "# a\n").unwrap();
    cx.update(update::register_commands);
    let root = vault.path().to_path_buf();
    let (workspace, cx) = cx.add_window_view(move |window, cx| {
        gasp_desktop::workspace::Workspace::new(&root, window, cx)
    });
    cx.run_until_parked();
    workspace.update_in(cx, |_, window, cx| {
        window.dispatch_action(
            Box::new(RunCommand {
                id: "app.check-for-updates".into(),
            }),
            cx,
        );
    });
    cx.run_until_parked();
    let shown = shown(cx);
    assert_eq!(
        shown
            .last()
            .map(|notice| notice.message.to_string())
            .as_deref(),
        Some("Gasp can only update itself when it runs as an installed app.")
    );
}

#[gpui::test]
fn a_newer_version_is_offered_with_its_notes(cx: &mut TestAppContext) {
    let cx = window(cx);
    cx.update(|_, cx| update::report(CheckOutcome::Newer(release("9.0.0")), false, cx));
    let shown = shown(cx);
    assert_eq!(shown.len(), 1);
    let offer = &shown[0];
    assert_eq!(offer.kind, NoticeKind::Offer);
    assert_eq!(offer.message.as_ref(), "Gasp 9.0.0 is out.");
    let action = offer.action.as_ref().unwrap();
    assert_eq!(
        (action.label.as_ref(), action.command.as_ref()),
        ("Update", INSTALL_COMMAND)
    );
    let link = offer.link.as_ref().unwrap();
    assert_eq!(
        (link.label.as_ref(), link.command.as_ref()),
        ("See what’s new", NOTES_COMMAND)
    );
}

#[gpui::test]
fn a_dismissed_offer_waits_for_the_next_launch_unless_asked_for(cx: &mut TestAppContext) {
    let cx = window(cx);
    cx.update(|_, cx| update::report(CheckOutcome::Newer(release("9.0.0")), false, cx));
    dismiss_all(cx);
    cx.update(|_, cx| update::report(CheckOutcome::Newer(release("9.0.0")), false, cx));
    assert!(shown(cx).is_empty());
    cx.update(|_, cx| update::report(CheckOutcome::Newer(release("9.1.0")), false, cx));
    assert_eq!(shown(cx)[0].message.as_ref(), "Gasp 9.1.0 is out.");
    dismiss_all(cx);
    cx.update(|_, cx| update::report(CheckOutcome::Newer(release("9.1.0")), true, cx));
    assert_eq!(shown(cx).len(), 1);
}

#[gpui::test]
fn only_a_check_from_the_menu_says_nothing_changed(cx: &mut TestAppContext) {
    let cx = window(cx);
    cx.update(|_, cx| update::report(CheckOutcome::UpToDate, false, cx));
    cx.update(|_, cx| update::report(CheckOutcome::Failed, false, cx));
    assert!(shown(cx).is_empty());
    cx.update(|_, cx| update::report(CheckOutcome::UpToDate, true, cx));
    assert_eq!(
        shown(cx)[0].message.to_string(),
        format!("Gasp {} is up to date.", running_version())
    );
    cx.update(|_, cx| update::report(CheckOutcome::Failed, true, cx));
    let shown = shown(cx);
    assert_eq!(shown.len(), 1, "the update notice changes in place");
    assert_eq!(shown[0].kind, NoticeKind::Problem);
}

#[gpui::test]
fn an_update_in_progress_changes_one_notice_in_place(cx: &mut TestAppContext) {
    let cx = window(cx);
    cx.update(|_, cx| update::report(CheckOutcome::Newer(release("9.0.0")), false, cx));
    for percent in [0, 40, 100] {
        cx.update(|_, cx| update::apply_progress(InstallProgress::Downloaded(percent), cx));
        let shown = shown(cx);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].message.as_ref(), "Downloading Gasp 9.0.0…");
        assert_eq!(shown[0].progress, Some(percent));
    }
    cx.update(|_, cx| update::apply_progress(InstallProgress::Verifying, cx));
    assert_eq!(shown(cx)[0].message.as_ref(), "Checking Gasp 9.0.0…");
    let staged = PathBuf::from("/nowhere/.Gasp-update.app");
    cx.update(|_, cx| {
        update::apply_progress(InstallProgress::Finished(Ok(Prepared::Staged(staged))), cx)
    });
    let shown = shown(cx);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].message.as_ref(), "Gasp 9.0.0 is ready.");
    assert_eq!(shown[0].progress, None);
    let action = shown[0].action.as_ref().unwrap();
    assert_eq!(
        (action.label.as_ref(), action.command.as_ref()),
        ("Restart to update", RESTART_COMMAND)
    );
}

#[gpui::test]
fn a_refused_update_says_why_in_plain_words(cx: &mut TestAppContext) {
    let cx = window(cx);
    cx.update(|_, cx| update::report(CheckOutcome::Newer(release("9.0.0")), false, cx));
    let refused = Err(InstallError::Refused(Refusal::OtherDeveloper));
    cx.update(|_, cx| update::apply_progress(InstallProgress::Finished(refused), cx));
    let shown = shown(cx);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].kind, NoticeKind::Problem);
    assert_eq!(shown[0].message.as_ref(), "Gasp didn’t update");
    assert_eq!(
        shown[0].detail.as_ref().map(|detail| detail.as_ref()),
        Some("The new app isn’t signed by Gasp’s developer.")
    );
}

/// A window that shows nothing but the notices.
struct NoticeHost;

impl Render for NoticeHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .relative()
            .children(notices::render(window, px(8.), cx))
    }
}

fn redraw(cx: &mut VisualTestContext) {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

#[gpui::test]
fn the_progress_bar_never_moves_the_notice(cx: &mut TestAppContext) {
    let (_, cx) = cx.add_window_view(|_, _| NoticeHost);
    cx.update(|_, cx| update::report(CheckOutcome::Newer(release("9.0.0")), false, cx));
    redraw(cx);
    let offer = cx.debug_bounds("notice-offer").unwrap();
    for label in ["button-Update", "button-See what’s new"] {
        let button = cx.debug_bounds(label).unwrap();
        assert!(offer.contains(&button.origin), "{label} sits on the card");
    }
    cx.update(|_, cx| update::apply_progress(InstallProgress::Downloaded(0), cx));
    redraw(cx);
    let started = cx.debug_bounds("notice-offer").unwrap();
    for percent in [37, 100] {
        cx.update(|_, cx| update::apply_progress(InstallProgress::Downloaded(percent), cx));
        redraw(cx);
        assert_eq!(cx.debug_bounds("notice-offer"), Some(started));
    }
    cx.update(|_, cx| update::apply_progress(InstallProgress::Verifying, cx));
    redraw(cx);
    assert_eq!(cx.debug_bounds("notice-offer"), Some(started));
}

#[gpui::test]
fn see_whats_new_opens_the_release_page(cx: &mut TestAppContext) {
    let window = window(cx);
    window.update(|_, cx| update::report(CheckOutcome::Newer(release("9.0.0")), false, cx));
    window.dispatch_action(RunCommand {
        id: NOTES_COMMAND.into(),
    });
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("https://example.com/notes")
    );
}
