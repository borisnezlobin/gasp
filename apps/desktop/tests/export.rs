//! Export and print: a corpus note becomes a PDF, and the export dialog
//! saves a PDF where the save prompt says, then offers to open it; its
//! HTML choice makes the website article, copies it and saves it.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use editor_desktop::export_ui::{self, ExportDialog, ExportFormat, ExportState};
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

/// Records whether the dialog asked to close.
fn watch_dismissal(
    dialog: &Entity<ExportDialog>,
    window: &mut VisualTestContext,
) -> Rc<Cell<bool>> {
    let dismissed = Rc::new(Cell::new(false));
    let seen = dismissed.clone();
    window.update(|_, cx| {
        cx.subscribe(dialog, move |_, _: &DismissEvent, _| seen.set(true))
            .detach();
    });
    dismissed
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
    let pages = export_ui::write_pdf(&text, Some(&note), None, &destination).unwrap();
    assert!(pages > 1);
    assert_pdf(&destination);
}

#[gpui::test]
fn the_dialog_saves_a_pdf_and_offers_to_open_it(cx: &mut TestAppContext) {
    let out = tempfile::tempdir().unwrap();
    let destination = out.path().join("chosen.pdf");
    let (dialog, window) = open_dialog(
        cx,
        "# Title\n\nSome text with $x^2$.",
        Some(PathBuf::from("Lemma.md")),
    );
    let dismissed = watch_dismissal(&dialog, window);
    dialog.read_with(window, |dialog, _| {
        assert_eq!(dialog.selected_format(), ExportFormat::Pdf)
    });
    window.simulate_keystrokes("enter");
    assert!(window.did_prompt_for_new_path());
    let chosen = destination.clone();
    window.simulate_new_path_selection(move |_| Some(chosen));
    window.run_until_parked();
    assert_pdf(&destination);
    dialog.read_with(window, |dialog, _| match dialog.state() {
        ExportState::Saved(saved) => {
            assert_eq!(saved.path, destination);
            assert_eq!(saved.detail, "1 page");
        }
        _ => panic!("the dialog should show the saved file"),
    });
    // The dialog stays until it's closed, so the file can be opened.
    assert!(!dismissed.get());
    window.simulate_keystrokes("escape");
    assert!(dismissed.get());
}

#[gpui::test]
fn cancelling_the_save_prompt_writes_nothing(cx: &mut TestAppContext) {
    let (dialog, window) = open_dialog(cx, "text", None);
    window.simulate_keystrokes("enter");
    window.simulate_new_path_selection(|_| None);
    window.run_until_parked();
    dialog.read_with(window, |dialog, _| {
        assert!(matches!(dialog.state(), ExportState::Choosing))
    });
}

#[gpui::test]
fn the_html_choice_makes_an_article_to_copy_and_save(cx: &mut TestAppContext) {
    let note = "---\ntitle: Waves\n---\nA wave has speed $v = f\\lambda$.[^1]\n\n[^1]: Always.";
    let (dialog, window) = open_dialog(cx, note, Some(PathBuf::from("Wave note.md")));
    window.simulate_keystrokes("down");
    dialog.read_with(window, |dialog, _| {
        assert_eq!(dialog.selected_format(), ExportFormat::Html)
    });
    window.simulate_keystrokes("enter");
    window.run_until_parked();
    let html = dialog.read_with(window, |dialog, _| match dialog.state() {
        ExportState::Article(article) => {
            assert_eq!(article.export.title, "Waves");
            assert_eq!(article.export.slug, "waves");
            article.export.html.clone()
        }
        _ => panic!("the dialog should show the article"),
    });
    assert!(
        html.starts_with("<article>") && html.contains("<math>"),
        "{html}"
    );

    // Enter copies it.
    window.simulate_keystrokes("enter");
    let copied = window.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some(html.as_str()));

    // Mod-S saves it.
    let out = tempfile::tempdir().unwrap();
    let destination = out.path().join("waves.html");
    window.simulate_keystrokes("secondary-s");
    assert!(window.did_prompt_for_new_path());
    let chosen = destination.clone();
    window.simulate_new_path_selection(move |_| Some(chosen));
    window.run_until_parked();
    assert_eq!(std::fs::read_to_string(&destination).unwrap(), html);
    dialog.read_with(window, |dialog, _| {
        assert!(matches!(dialog.state(), ExportState::Saved(_)))
    });
}

#[test]
fn images_linked_from_the_vault_root_are_found() {
    let vault = tempfile::tempdir().unwrap();
    let figures = vault.path().join("Figures");
    std::fs::create_dir_all(&figures).unwrap();
    let image = corpus_note()
        .parent()
        .unwrap()
        .join("images/vector-159.png");
    std::fs::copy(image, figures.join("Diagram.png")).unwrap();
    let note = vault.path().join("Notes/Deep/Waves.md");
    std::fs::create_dir_all(note.parent().unwrap()).unwrap();
    let text = "# Waves\n\n![A diagram](Figures/Diagram.png)\n";
    let alone = export_ui::html_article(text, Some(&note), None);
    assert_eq!(alone.missing_images, ["Figures/Diagram.png"]);
    let found = export_ui::html_article(text, Some(&note), Some(vault.path()));
    assert!(
        found.missing_images.is_empty(),
        "{:?}",
        found.missing_images
    );
    let pdf = export_ui::pdf_file(text, Some(&note), Some(vault.path())).unwrap();
    assert!(pdf.bytes.len() > 1000);
}
