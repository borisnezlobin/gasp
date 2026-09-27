//! Print: `app.print` opens the print dialog over the note; the dialog
//! lays the note out into previewed pages, lays it out again when a
//! setting changes, saves the PDF where the save prompt says, prints on
//! Enter and closes on Escape or Cancel.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use editor_config::{Platform, RuleSet};
use editor_desktop::features;
use editor_desktop::keymap::all_bindings;
use editor_desktop::print::{self, Control, Paper, Pending, PrintDialog, PrintSink, Status};
use editor_desktop::workspace::{OpenIn, Workspace};
use gpui::{
    Context, DismissEvent, Entity, Focusable, IntoElement, Modifiers, Render, TestAppContext,
    VisualTestContext, Window, div, prelude::*,
};

/// Stands in for the workspace's modal slot.
struct Host {
    dialog: Entity<PrintDialog>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.dialog.clone())
    }
}

fn open_dialog<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
    note: Option<PathBuf>,
) -> (Entity<PrintDialog>, &'a mut VisualTestContext) {
    cx.update(|cx| {
        print::bind_keys(cx);
        editor_desktop::ui::focus_visible::install(cx);
    });
    let text = text.to_owned();
    let (host, window) = cx.add_window_view(move |window, cx| {
        let dialog = cx.new(|cx| PrintDialog::new(text, note, None, window, cx));
        window.focus(&dialog.focus_handle(cx));
        Host { dialog }
    });
    let dialog = host.read_with(window, |host, _| host.dialog.clone());
    (dialog, window)
}

/// Records whether the dialog asked to close.
fn watch_dismissal(dialog: &Entity<PrintDialog>, window: &mut VisualTestContext) -> Rc<Cell<bool>> {
    let dismissed = Rc::new(Cell::new(false));
    let seen = dismissed.clone();
    window.update(|_, cx| {
        cx.subscribe(dialog, move |_, _: &DismissEvent, _| seen.set(true))
            .detach();
    });
    dismissed
}

/// Lets a debounced layout start and finish.
fn settle(window: &mut VisualTestContext) {
    window.executor().advance_clock(Duration::from_secs(1));
    window.run_until_parked();
}

fn page_count(dialog: &Entity<PrintDialog>, window: &mut VisualTestContext) -> Option<usize> {
    dialog.read_with(window, |dialog, _| {
        dialog.preview().map(|preview| preview.pages.len())
    })
}

fn footer(window: &mut VisualTestContext) -> bool {
    window.run_until_parked();
    window.debug_bounds("print-footer").is_some()
}

const SHORT_NOTE: &str = "# Title\n\nSome text with $x^2$.";

/// Enough paragraphs at line height 2 to run onto a second page.
fn two_page_note() -> String {
    (1..=40)
        .map(|n| format!("Paragraph {n} of the note."))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Stands in for the modal slot before anything opens in it.
struct LateHost {
    dialog: Option<Entity<PrintDialog>>,
}

impl Render for LateHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().children(self.dialog.clone())
    }
}

#[gpui::test]
fn the_preview_shows_a_placeholder_then_the_pages(cx: &mut TestAppContext) {
    let (host, window) = cx.add_window_view(|_, _| LateHost { dialog: None });
    let text = two_page_note();
    let dialog = window.update(|window, cx| {
        let note = Some(PathBuf::from("Pages.md"));
        let dialog = cx.new(|cx| PrintDialog::new(text, note, None, window, cx));
        host.update(cx, |host, cx| {
            host.dialog = Some(dialog.clone());
            cx.notify();
        });
        dialog
    });
    // Before the layout finishes there are no pages; the preview shows a
    // page-shaped placeholder.
    dialog.read_with(window, |dialog, _| {
        assert!(dialog.is_laying_out());
        assert!(dialog.preview().is_none());
    });
    assert!(window.debug_bounds("print-placeholder").is_some());

    window.run_until_parked();
    assert_eq!(page_count(&dialog, window), Some(2));
    dialog.read_with(window, |dialog, _| {
        let preview = dialog.preview().unwrap();
        assert!(preview.pdf.starts_with(b"%PDF"));
        assert!(!dialog.is_laying_out());
    });
    assert!(footer(window));
    assert!(window.debug_bounds("print-placeholder").is_none());
    assert!(window.debug_bounds("print-page-2").is_some());
}

#[gpui::test]
fn changing_a_setting_lays_the_note_out_again(cx: &mut TestAppContext) {
    let (dialog, window) = open_dialog(cx, SHORT_NOTE, None);
    window.run_until_parked();
    assert_eq!(page_count(&dialog, window), Some(1));

    // Paper is the first setting; Right picks the next paper size.
    window.simulate_keystrokes("right");
    dialog.read_with(window, |dialog, _| {
        assert_eq!(dialog.settings().paper, Paper::Letter);
        assert!(dialog.is_laying_out());
        // The old pages stay until the new ones are drawn.
        assert_eq!(dialog.preview().unwrap().settings.paper, Paper::A4);
    });
    settle(window);
    dialog.read_with(window, |dialog, _| {
        let preview = dialog.preview().unwrap();
        assert_eq!(preview.settings.paper, Paper::Letter);
        assert!((preview.pages[0].aspect - 11. / 8.5).abs() < 0.01);
    });

    // Down twice reaches page numbers; Space turns them off.
    window.simulate_keystrokes("down down space");
    dialog.read_with(window, |dialog, _| {
        assert_eq!(dialog.focused_control(), Control::PageNumbers);
        assert!(!dialog.settings().page_numbers);
    });
    settle(window);
    dialog.read_with(window, |dialog, _| {
        assert!(!dialog.preview().unwrap().settings.page_numbers)
    });
}

#[gpui::test]
fn the_paper_menu_picks_a_size(cx: &mut TestAppContext) {
    let (dialog, window) = open_dialog(cx, SHORT_NOTE, None);
    window.run_until_parked();
    let bounds = window
        .debug_bounds("print-paper")
        .expect("the paper button");
    window.simulate_click(bounds.center(), Modifiers::none());
    window.run_until_parked();
    let item = window
        .debug_bounds("menu-item-A5 (148 × 210 mm)")
        .expect("the menu lists A5");
    window.simulate_click(item.center(), Modifiers::none());
    settle(window);
    dialog.read_with(window, |dialog, _| {
        assert_eq!(dialog.settings().paper, Paper::A5);
        assert_eq!(dialog.preview().unwrap().settings.paper, Paper::A5);
    });
}

#[gpui::test]
fn save_as_pdf_writes_the_pdf_where_the_prompt_says(cx: &mut TestAppContext) {
    let out = tempfile::tempdir().unwrap();
    let destination = out.path().join("chosen.pdf");
    let (dialog, window) = open_dialog(cx, SHORT_NOTE, Some("Lemma.md".into()));
    let dismissed = watch_dismissal(&dialog, window);
    window.run_until_parked();
    window.simulate_keystrokes("secondary-s");
    assert!(window.did_prompt_for_new_path());
    let chosen = destination.clone();
    window.simulate_new_path_selection(move |_| Some(chosen));
    window.run_until_parked();
    let bytes = std::fs::read(&destination).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
    dialog.read_with(window, |dialog, _| {
        assert_eq!(dialog.status(), Some(&Status::Saved(destination.clone())))
    });
    // The dialog stays, now offering Close.
    assert!(!dismissed.get());
    assert!(window.debug_bounds("button-Close").is_some());
}

#[gpui::test]
fn a_cancelled_save_writes_nothing(cx: &mut TestAppContext) {
    let (dialog, window) = open_dialog(cx, SHORT_NOTE, None);
    window.run_until_parked();
    let bounds = window.debug_bounds("button-Save as PDF…").unwrap();
    window.simulate_click(bounds.center(), Modifiers::none());
    assert!(window.did_prompt_for_new_path());
    window.simulate_new_path_selection(|_| None);
    window.run_until_parked();
    dialog.read_with(window, |dialog, _| assert_eq!(dialog.status(), None));
}

#[gpui::test]
fn escape_and_cancel_close_the_dialog(cx: &mut TestAppContext) {
    let (dialog, window) = open_dialog(cx, SHORT_NOTE, None);
    let dismissed = watch_dismissal(&dialog, window);
    window.run_until_parked();
    window.simulate_keystrokes("escape");
    assert!(dismissed.get());

    dismissed.set(false);
    let bounds = window.debug_bounds("button-Cancel").unwrap();
    window.simulate_click(bounds.center(), Modifiers::none());
    assert!(dismissed.get());
}

#[gpui::test]
fn enter_prints_once_the_pages_are_ready(cx: &mut TestAppContext) {
    let printed: Rc<RefCell<Vec<Vec<u8>>>> = Rc::default();
    let sink = printed.clone();
    cx.update(|cx| {
        cx.set_global(PrintSink(Rc::new(move |pdf| {
            sink.borrow_mut().push(pdf.to_vec())
        })))
    });
    let (dialog, window) = open_dialog(cx, SHORT_NOTE, None);
    let dismissed = watch_dismissal(&dialog, window);
    // A change starts a new layout; Enter waits for it, so what prints
    // has the new settings.
    window.simulate_keystrokes("right enter");
    dialog.read_with(window, |dialog, _| {
        assert_eq!(dialog.pending(), Some(Pending::Print))
    });
    assert!(!dismissed.get());
    assert!(footer(window));
    settle(window);
    assert!(dismissed.get());
    let printed = printed.borrow();
    assert_eq!(printed.len(), 1);
    assert!(printed[0].starts_with(b"%PDF"));
    // US Letter is 612 points wide.
    let text = String::from_utf8_lossy(&printed[0]);
    assert!(text.contains("612"), "the PDF should be on Letter paper");
}

// ---- Wired into a workspace ----

fn key_for(command: &str) -> String {
    all_bindings(&RuleSet::defaults(), Platform::current())
        .into_iter()
        .find(|binding| binding.command == command)
        .map(|binding| binding.keystroke)
        .unwrap_or_else(|| panic!("{command} has no key"))
}

#[gpui::test]
fn app_print_opens_the_print_dialog(cx: &mut TestAppContext) {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("Note.md"), SHORT_NOTE).unwrap();
    cx.update(|cx| {
        editor_desktop::actions::bind_keys(cx);
        features::bind_view_keys(cx);
    });
    let root = vault.path().to_path_buf();
    let (workspace, cx) = cx.add_window_view(move |window, cx| {
        let mut workspace = Workspace::new(&root, window, cx);
        features::install(&mut workspace, window, cx);
        workspace
    });
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace
                .open_path(Path::new("Note.md"), OpenIn::ActiveTab, window, cx)
                .unwrap()
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes(&key_for("app.print"));
    cx.run_until_parked();
    let dialog = cx
        .read(|cx| workspace.read(cx).active_modal::<PrintDialog>())
        .expect("the print dialog opens");
    dialog.read_with(cx, |dialog, _| {
        assert_eq!(dialog.preview().map(|preview| preview.pages.len()), Some(1))
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(
        cx.read(|cx| workspace.read(cx).active_modal::<PrintDialog>())
            .is_none()
    );
}
