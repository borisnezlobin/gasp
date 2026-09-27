//! Export and print. `app.export` opens a small dialog whose PDF choice
//! asks where to save and writes the note through Typst; HTML export isn't
//! built yet and shows as disabled. `app.print` writes the PDF to a
//! temporary file and opens it in the system viewer, which prints it. A
//! real print preview with live settings is a later phase.
//!
//! Wiring: on `app.export` the workspace calls [`export`] and hosts the
//! returned dialog in its modal slot until it emits `DismissEvent`; on
//! `app.print` it calls [`print`].

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use editor_export::pdf::{PdfOptions, evict_memory, export_pdf};
use gpui::{
    App, AppContext, ClickEvent, Context, DismissEvent, Entity, EventEmitter, FocusHandle,
    Focusable, KeyBinding, SharedString, Task, Window, actions, div, prelude::*,
};

use crate::icons::{IconName, icon};
use crate::theme::UiTheme;

/// The key context the dialog sets.
pub const EXPORT_CONTEXT: &str = "ExportDialog";

/// Memoized Typst results older than this many exports are freed.
const TYPST_CACHE_EXPORTS: usize = 4;

actions!(export_dialog, [SelectNext, SelectPrevious, Confirm, Cancel]);

/// The dialog's keys: arrows move, Enter exports, Escape closes.
pub fn bind_keys(cx: &mut App) {
    let context = Some(EXPORT_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("down", SelectNext, context),
        KeyBinding::new("up", SelectPrevious, context),
        KeyBinding::new("enter", Confirm, context),
        KeyBinding::new("escape", Cancel, context),
    ]);
}

/// An export format in the dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Pdf,
    Html,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 2] = [ExportFormat::Pdf, ExportFormat::Html];

    /// HTML export is a later phase.
    pub fn is_available(self) -> bool {
        self == ExportFormat::Pdf
    }

    fn label(self) -> &'static str {
        match self {
            ExportFormat::Pdf => "PDF",
            ExportFormat::Html => "HTML",
        }
    }

    fn detail(self) -> &'static str {
        match self {
            ExportFormat::Pdf => "Save a PDF of this note",
            ExportFormat::Html => "Coming later",
        }
    }

    fn icon(self) -> IconName {
        match self {
            ExportFormat::Pdf => IconName::FilePdf,
            ExportFormat::Html => IconName::FileHtml,
        }
    }
}

/// The note's PDF, with the default export settings.
pub fn pdf_bytes(text: &str, note_path: Option<&Path>) -> anyhow::Result<Vec<u8>> {
    let exported = export_pdf(text, note_path, None, &PdfOptions::default(), &[]);
    evict_memory(TYPST_CACHE_EXPORTS);
    Ok(exported?.pdf)
}

/// Exports the note and writes the PDF to `destination`.
pub fn write_pdf(text: &str, note_path: Option<&Path>, destination: &Path) -> anyhow::Result<()> {
    let bytes = pdf_bytes(text, note_path)?;
    std::fs::write(destination, bytes)
        .with_context(|| format!("could not write {}", destination.display()))
}

/// `<note>.pdf`, or `Untitled.pdf`.
pub fn suggested_file_name(note_path: Option<&Path>) -> String {
    let stem = note_path.and_then(Path::file_stem).map_or_else(
        || "Untitled".to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    format!("{stem}.pdf")
}

/// Where the print PDF goes: the system's temporary folder.
pub fn print_path(note_path: Option<&Path>) -> PathBuf {
    std::env::temp_dir().join(suggested_file_name(note_path))
}

fn save_folder(note_path: Option<&Path>) -> PathBuf {
    note_path
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default()
}

/// Asks where to save, then exports the PDF there on a background thread.
/// Resolves to the saved path, or `None` when the save was cancelled.
pub fn save_pdf(
    text: String,
    note_path: Option<PathBuf>,
    cx: &mut App,
) -> Task<anyhow::Result<Option<PathBuf>>> {
    let folder = save_folder(note_path.as_deref());
    let name = suggested_file_name(note_path.as_deref());
    let chosen = cx.prompt_for_new_path(&folder, Some(&name));
    let executor = cx.background_executor().clone();
    cx.spawn(async move |_| {
        let Some(destination) = chosen.await?? else {
            return Ok(None);
        };
        executor
            .spawn(async move {
                write_pdf(&text, note_path.as_deref(), &destination)?;
                Ok(Some(destination))
            })
            .await
    })
}

/// Writes the PDF to a temporary file and opens it in the system viewer,
/// where it can be printed.
pub fn print(
    text: String,
    note_path: Option<PathBuf>,
    cx: &mut App,
) -> Task<anyhow::Result<PathBuf>> {
    let destination = print_path(note_path.as_deref());
    let writing = cx.background_spawn(async move {
        write_pdf(&text, note_path.as_deref(), &destination)?;
        anyhow::Ok(destination)
    });
    cx.spawn(async move |cx| {
        let destination = writing.await?;
        cx.update(|cx| cx.open_with_system(&destination))?;
        Ok(destination)
    })
}

/// Opens the export dialog for the note's text. The workspace hosts the
/// returned view as a modal until it emits `DismissEvent`.
pub fn export(
    editor_text: String,
    note_path: Option<PathBuf>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<ExportDialog> {
    let dialog = cx.new(|cx| ExportDialog::new(editor_text, note_path, cx));
    window.focus(&dialog.focus_handle(cx));
    dialog
}

/// What the dialog is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportState {
    Choosing,
    Exporting,
    Failed(String),
}

/// The export dialog.
pub struct ExportDialog {
    text: String,
    note_path: Option<PathBuf>,
    selected: usize,
    state: ExportState,
    focus_handle: FocusHandle,
    theme: UiTheme,
    export_task: Option<Task<()>>,
}

impl EventEmitter<DismissEvent> for ExportDialog {}

impl Focusable for ExportDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ExportDialog {
    pub fn new(text: String, note_path: Option<PathBuf>, cx: &mut Context<Self>) -> Self {
        Self {
            text,
            note_path,
            selected: 0,
            state: ExportState::Choosing,
            focus_handle: cx.focus_handle(),
            theme: crate::ui::ui_theme(cx),
            export_task: None,
        }
    }

    pub fn selected_format(&self) -> ExportFormat {
        ExportFormat::ALL[self.selected]
    }

    pub fn state(&self) -> &ExportState {
        &self.state
    }

    /// Moves to the next available format, wrapping.
    fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
        let count = ExportFormat::ALL.len();
        let mut index = self.selected;
        for _ in 0..count {
            index = if forward {
                (index + 1) % count
            } else {
                (index + count - 1) % count
            };
            if ExportFormat::ALL[index].is_available() {
                break;
            }
        }
        if ExportFormat::ALL[index].is_available() {
            self.selected = index;
            cx.notify();
        }
    }

    /// Exports in the selected format, then closes.
    pub fn confirm(&mut self, cx: &mut Context<Self>) {
        let format = self.selected_format();
        if !format.is_available() || self.state == ExportState::Exporting {
            return;
        }
        self.state = ExportState::Exporting;
        cx.notify();
        let saving = save_pdf(self.text.clone(), self.note_path.clone(), cx);
        self.export_task = Some(cx.spawn(async move |this, cx| {
            let result = saving.await;
            this.update(cx, |dialog, cx| dialog.finish(result, cx)).ok();
        }));
    }

    fn finish(&mut self, result: anyhow::Result<Option<PathBuf>>, cx: &mut Context<Self>) {
        match result {
            Ok(_) => {
                self.state = ExportState::Choosing;
                cx.emit(DismissEvent);
            }
            Err(error) => self.state = ExportState::Failed(format!("{error:#}")),
        }
        cx.notify();
    }

    fn choose(&mut self, index: usize, cx: &mut Context<Self>) {
        if ExportFormat::ALL[index].is_available() {
            self.selected = index;
            self.confirm(cx);
        }
    }

    fn render_choice(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = &self.theme;
        let format = ExportFormat::ALL[index];
        let available = format.is_available();
        let (text, detail_color, icon_color) = if available {
            (ui.text, ui.text_detail, ui.icon)
        } else {
            (ui.text_faint, ui.text_faint, ui.icon_disabled)
        };
        let detail: SharedString = if available && self.state == ExportState::Exporting {
            "Exporting…".into()
        } else {
            format.detail().into()
        };
        let selected = index == self.selected;
        div()
            .id(("export-format", index))
            .flex()
            .items_center()
            .gap(ui.space_md + ui.space_xs)
            .h(ui.row_height)
            .px(ui.row_padding_x)
            .rounded(ui.row_radius)
            .when(selected, |row| row.bg(ui.row_selected))
            .when(available && !selected, |row| {
                row.hover(|style| style.bg(ui.row_hover))
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.choose(index, cx)))
            .child(
                icon(format.icon())
                    .flex_none()
                    .size(ui.icon_size - gpui::px(2.))
                    .text_color(icon_color),
            )
            .child(div().flex_none().text_color(text).child(format.label()))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .text_size(ui.small_font_size)
                    .text_color(detail_color)
                    .child(crate::ui::truncated(detail).grow()),
            )
    }
}

impl Render for ExportDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = self.theme.clone();
        let error = match &self.state {
            ExportState::Failed(message) => Some(message.clone()),
            _ => None,
        };
        let choices: Vec<_> = (0..ExportFormat::ALL.len())
            .map(|index| self.render_choice(index, cx).into_any_element())
            .collect();
        crate::ui::dialog(&ui)
            .key_context(EXPORT_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.step(true, cx)))
            .on_action(cx.listener(|this, _: &SelectPrevious, _, cx| this.step(false, cx)))
            .on_action(cx.listener(|this, _: &Confirm, _, cx| this.confirm(cx)))
            .on_action(cx.listener(|_, _: &Cancel, _, cx| cx.emit(DismissEvent)))
            .w(ui.small_dialog_width)
            .p(ui.dialog_padding)
            .child(
                div()
                    .px(ui.row_padding_x)
                    .pt(ui.space_md)
                    .pb(ui.space_md)
                    .text_size(ui.font_size + gpui::px(2.))
                    .child("Export this note"),
            )
            .children(choices)
            .when_some(error, |dialog, error| {
                dialog.child(
                    div()
                        .px(ui.row_padding_x)
                        .py(ui.space_md)
                        .text_size(ui.small_font_size)
                        .text_color(ui.error)
                        .child(error),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_follow_the_note() {
        assert_eq!(
            suggested_file_name(Some(Path::new("a/Lemma.md"))),
            "Lemma.pdf"
        );
        assert_eq!(suggested_file_name(None), "Untitled.pdf");
        assert!(print_path(None).ends_with("Untitled.pdf"));
    }

    #[test]
    fn only_pdf_is_available() {
        assert!(ExportFormat::Pdf.is_available());
        assert!(!ExportFormat::Html.is_available());
    }
}
