//! Export and print: a corpus note becomes a PDF, and the export dialog
//! saves one where the save prompt says.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use editor_desktop::export_ui::{self, ExportDialog, ExportFormat};
use gpui::{
    Context, DismissEvent, Entity, IntoElement, Render, TestAppContext, VisualTestContext, Window,
    div, prelude::*,
};

/// Stands in for the workspace's modal slot.
struct Host {
    dialog: Entity<ExportDialog>,
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
) -> (Entity<ExportDialog>, &'a mut VisualTestContext) {
    cx.update(export_ui::bind_keys);
    let text = text.to_owned();
    let (host, window) = cx.add_window_view(move |window, cx| Host {
        dialog: export_ui::export(text, note, window, cx),
    });
    let dialog = host.read_with(window, |host, _| host.dialog.clone());
    window.run_until_parked();
    (dialog, window)
}

fn corpus_note() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/corpus/Course Notes/Classical Mechanics/Harmonic Oscillator.md")
}

fn assert_pdf(path: &Path) {
    let bytes = std::fs::read(path).unwrap();
    assert!(bytes.len() > 1000, "only {} bytes", bytes.len());
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn a_corpus_note_exports_to_pdf() {
    let note = corpus_note();
    let text = std::fs::read_to_string(&note).unwrap();
    let out = tempfile::tempdir().unwrap();
    let destination = out.path().join("note.pdf");
    export_ui::write_pdf(&text, Some(&note), &destination).unwrap();
    assert_pdf(&destination);
}

#[gpui::test]
fn the_dialog_saves_a_pdf_where_the_prompt_says(cx: &mut TestAppContext) {
    let out = tempfile::tempdir().unwrap();
    let destination = out.path().join("chosen.pdf");
    let (dialog, window) = open_dialog(
        cx,
        "# Title\n\nSome text with $x^2$.",
        Some(PathBuf::from("Lemma.md")),
    );
    let dismissed = Rc::new(Cell::new(false));
    let seen = dismissed.clone();
    window.update(|_, cx| {
        cx.subscribe(&dialog, move |_, _: &DismissEvent, _| seen.set(true))
            .detach();
    });
    dialog.read_with(window, |dialog, _| {
        assert_eq!(dialog.selected_format(), ExportFormat::Pdf)
    });
    // HTML is disabled, so moving down stays on PDF.
    window.simulate_keystrokes("down enter");
    assert!(window.did_prompt_for_new_path());
    let chosen = destination.clone();
    window.simulate_new_path_selection(move |_| Some(chosen));
    window.run_until_parked();
    assert_pdf(&destination);
    assert!(dismissed.get());
}

#[gpui::test]
fn cancelling_the_save_prompt_writes_nothing(cx: &mut TestAppContext) {
    let (dialog, window) = open_dialog(cx, "text", None);
    window.simulate_keystrokes("enter");
    window.simulate_new_path_selection(|_| None);
    window.run_until_parked();
    dialog.read_with(window, |dialog, _| {
        assert_eq!(*dialog.state(), export_ui::ExportState::Choosing)
    });
}
