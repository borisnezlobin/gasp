//! Export. `app.export` opens a small dialog with two choices:
//!
//! - PDF asks where to save, writes the note through Typst and then shows
//!   the file with Open and Show in folder.
//! - HTML makes the article for the owner's website and shows its source,
//!   with Copy HTML (what the website's editor takes), Save… and a preview
//!   in the browser styled by the site's article stylesheet.
//!
//! Printing, with its preview and settings, is `crate::print`; it shares
//! the save flow and file names here.
//!
//! Wiring: on `app.export` the workspace calls [`export`] and hosts the
//! returned dialog in its modal slot until it emits `DismissEvent`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Context as _;
use gasp_export::html::{HtmlExport, HtmlOptions, export_html, standalone_page};
use gasp_export::pdf::{PdfOptions, evict_memory, export_pdf, fonts_for, warm_up};
use gpui::{
    AnyElement, App, AppContext, ClickEvent, ClipboardItem, Context, DismissEvent, Div, Entity,
    EventEmitter, FocusHandle, Focusable, KeyBinding, SharedString, Task, Window, actions, div,
    prelude::*,
};

use crate::icons::{IconName, icon};
use crate::theme::UiTheme;
use crate::ui::button::Button;

/// The key context the dialog sets.
pub const EXPORT_CONTEXT: &str = "ExportDialog";

/// Memoized Typst results older than this many exports are freed.
const TYPST_CACHE_EXPORTS: usize = 4;

/// Lines of HTML source the preview shows; the rest is copied and saved
/// but not laid out.
const PREVIEW_LINES: usize = 200;

/// What the preview leaves out, said under it while nothing else is.
const SHORTENED_NOTE: &str =
    "Equations and images are shortened here. Copy HTML takes them in full.";

/// Where the website files an article, shown under its title.
const ARTICLE_URL: &str = "borisnezlobin.com/writing/";

actions!(
    export_dialog,
    [SelectNext, SelectPrevious, Confirm, Cancel, SaveAs]
);

/// The dialog's keys: arrows move, Enter takes the main action (export,
/// Copy HTML, Open), Mod-S saves the article, Escape closes.
pub fn bind_keys(cx: &mut App) {
    let context = Some(EXPORT_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("down", SelectNext, context),
        KeyBinding::new("up", SelectPrevious, context),
        KeyBinding::new("enter", Confirm, context),
        KeyBinding::new("secondary-s", SaveAs, context),
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

    fn label(self) -> &'static str {
        match self {
            ExportFormat::Pdf => "PDF",
            ExportFormat::Html => "HTML",
        }
    }

    fn detail(self) -> &'static str {
        match self {
            ExportFormat::Pdf => "A4 pages, ready to print or send",
            ExportFormat::Html => "An article for your website",
        }
    }

    fn icon(self) -> IconName {
        match self {
            ExportFormat::Pdf => IconName::FilePdf,
            ExportFormat::Html => IconName::FileHtml,
        }
    }
}

/// A written PDF.
pub struct PdfFile {
    pub bytes: Vec<u8>,
    pub pages: usize,
}

/// The note's PDF, with the default export settings and the installed
/// fonts they name. Images are looked for beside the note, then from
/// `vault_root`, as Obsidian finds them.
pub fn pdf_file(
    text: &str,
    note_path: Option<&Path>,
    vault_root: Option<&Path>,
) -> anyhow::Result<PdfFile> {
    let options = PdfOptions::default();
    let fonts = fonts_for(&options);
    let exported = export_pdf(text, note_path, vault_root, &options, &fonts);
    evict_memory(TYPST_CACHE_EXPORTS);
    let exported = exported?;
    Ok(PdfFile {
        bytes: exported.pdf,
        pages: exported.pages,
    })
}

/// The message for a file that couldn't be written: its name only, so it
/// fits the dialog.
fn couldnt_write(path: &Path) -> String {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    format!("Couldn’t write {name}")
}

/// Writes finished PDF bytes to `destination`.
pub(crate) fn write_bytes(destination: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    std::fs::write(destination, bytes).with_context(|| couldnt_write(destination))
}

/// The note's PDF bytes.
pub fn pdf_bytes(
    text: &str,
    note_path: Option<&Path>,
    vault_root: Option<&Path>,
) -> anyhow::Result<Vec<u8>> {
    Ok(pdf_file(text, note_path, vault_root)?.bytes)
}

/// Exports the note and writes the PDF to `destination`; returns its page
/// count.
pub fn write_pdf(
    text: &str,
    note_path: Option<&Path>,
    vault_root: Option<&Path>,
    destination: &Path,
) -> anyhow::Result<usize> {
    let file = pdf_file(text, note_path, vault_root)?;
    std::fs::write(destination, file.bytes).with_context(|| couldnt_write(destination))?;
    Ok(file.pages)
}

/// The note as an article for the website, its images found as the
/// PDF's are.
pub fn html_article(text: &str, note_path: Option<&Path>, vault_root: Option<&Path>) -> HtmlExport {
    export_html(text, note_path, vault_root, &HtmlOptions::default())
}

/// Gets Typst ready in the background the first time the dialog opens, so
/// the export itself doesn't pay for it.
fn warm_up_once(cx: &mut App) {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    cx.background_spawn(async { warm_up(&PdfOptions::default()) })
        .detach();
}

fn note_stem(note_path: Option<&Path>) -> String {
    note_path.and_then(Path::file_stem).map_or_else(
        || "Untitled".to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    )
}

/// `<note>.pdf`, or `Untitled.pdf`.
pub fn suggested_file_name(note_path: Option<&Path>) -> String {
    format!("{}.pdf", note_stem(note_path))
}

/// Where the print PDF goes: the system's temporary folder.
pub fn print_path(note_path: Option<&Path>) -> PathBuf {
    std::env::temp_dir().join(suggested_file_name(note_path))
}

pub(crate) fn save_folder(note_path: Option<&Path>) -> PathBuf {
    note_path
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default()
}

/// Asks where to save `name`, then writes it with `write` on a background
/// thread. Resolves to the saved path and `write`'s answer, or `None` when
/// the save was cancelled.
pub(crate) fn save_with<T: Send + 'static>(
    folder: &Path,
    name: &str,
    write: impl FnOnce(&Path) -> anyhow::Result<T> + Send + 'static,
    cx: &mut App,
) -> Task<anyhow::Result<Option<(PathBuf, T)>>> {
    let chosen = cx.prompt_for_new_path(folder, Some(name));
    let executor = cx.background_executor().clone();
    cx.spawn(async move |_| {
        let chosen = chosen
            .await
            .map_err(anyhow::Error::from)
            .and_then(|answer| answer)
            .context("The save dialog didn’t open")?;
        let Some(destination) = chosen else {
            return Ok(None);
        };
        executor
            .spawn(async move {
                let answer = write(&destination)?;
                Ok(Some((destination, answer)))
            })
            .await
    })
}

/// Asks where to save, then exports the PDF there on a background thread.
/// Resolves to the saved path and its page count, or `None` when the save
/// was cancelled.
pub fn save_pdf(
    text: String,
    note_path: Option<PathBuf>,
    vault_root: Option<PathBuf>,
    cx: &mut App,
) -> Task<anyhow::Result<Option<(PathBuf, usize)>>> {
    let folder = save_folder(note_path.as_deref());
    let name = suggested_file_name(note_path.as_deref());
    save_with(
        &folder,
        &name,
        move |destination| {
            write_pdf(
                &text,
                note_path.as_deref(),
                vault_root.as_deref(),
                destination,
            )
        },
        cx,
    )
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

/// "12 KB", "1.4 MB".
fn byte_size(bytes: usize) -> String {
    const KB: f64 = 1024.;
    let bytes = bytes as f64;
    if bytes < KB * KB {
        format!("{} KB", (bytes / KB).ceil())
    } else {
        format!("{:.1} MB", bytes / KB / KB)
    }
}

/// The article's source as the preview shows it: embedded images and the
/// MathML inside equations shortened (they are the bulk of the text but
/// can't be read) and at most [`PREVIEW_LINES`] lines. Also returns the
/// number of lines left out.
pub fn preview_lines(html: &str) -> (Vec<SharedString>, usize) {
    let lines: Vec<&str> = html.lines().collect();
    let shown = lines
        .iter()
        .take(PREVIEW_LINES)
        .map(|line| SharedString::from(shorten_math(&shorten_data_uris(line))))
        .collect();
    (shown, lines.len().saturating_sub(PREVIEW_LINES))
}

/// `<math …>…</math>` with its content replaced by an ellipsis.
fn shorten_math(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = rest.find("<math") {
        let Some(open_end) = rest[start..].find('>').map(|end| start + end + 1) else {
            break;
        };
        let Some(close) = rest[open_end..].find("</math>").map(|at| open_end + at) else {
            break;
        };
        out.push_str(&rest[..open_end]);
        out.push('…');
        rest = &rest[close..];
        out.push_str("</math>");
        rest = &rest["</math>".len()..];
    }
    out.push_str(rest);
    out
}

fn shorten_data_uris(line: &str) -> String {
    let mut out = String::with_capacity(line.len().min(4096));
    let mut rest = line;
    while let Some(at) = rest.find(";base64,") {
        let data = at + ";base64,".len();
        out.push_str(&rest[..data]);
        out.push('…');
        let end = rest[data..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=')))
            .map_or(rest.len(), |end| data + end);
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// A finished article, ready to copy or save.
pub struct Article {
    pub export: HtmlExport,
    lines: Vec<SharedString>,
    hidden_lines: usize,
    /// What the last action did, such as "Copied to the clipboard."
    status: Option<SharedString>,
}

impl Article {
    fn new(export: HtmlExport) -> Self {
        let (lines, hidden_lines) = preview_lines(&export.html);
        Self {
            export,
            lines,
            hidden_lines,
            status: None,
        }
    }

    /// Problems worth knowing before publishing.
    fn warning(&self) -> Option<String> {
        let images = &self.export.missing_images;
        let math = &self.export.failed_math;
        match (images.len(), math.len()) {
            (0, 0) => None,
            (1, _) => Some(format!("The image {} couldn’t be found.", images[0])),
            (count, _) if count > 1 => Some(format!("{count} images couldn’t be found.")),
            (_, 1) => Some("One equation couldn’t be typeset and shows as LaTeX.".to_owned()),
            (_, count) => Some(format!(
                "{count} equations couldn’t be typeset and show as LaTeX."
            )),
        }
    }
}

/// A file the dialog wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    pub path: PathBuf,
    /// A fact about the file, such as "6 pages".
    pub detail: String,
}

/// What the dialog is doing.
pub enum ExportState {
    Choosing,
    Exporting(ExportFormat),
    Article(Box<Article>),
    Saved(Saved),
    Failed(String),
}

impl ExportState {
    fn is_busy(&self) -> bool {
        matches!(self, ExportState::Exporting(_))
    }
}

/// The export dialog.
pub struct ExportDialog {
    text: String,
    note_path: Option<PathBuf>,
    /// Where images are looked for when they aren't beside the note.
    vault_root: Option<PathBuf>,
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
        warm_up_once(cx);
        Self {
            text,
            note_path,
            vault_root: None,
            selected: 0,
            state: ExportState::Choosing,
            focus_handle: cx.focus_handle(),
            theme: crate::ui::ui_theme(cx),
            export_task: None,
        }
    }

    /// Finds images from the vault's root too, as the note's links do.
    pub fn with_vault_root(mut self, vault_root: impl Into<PathBuf>) -> Self {
        self.vault_root = Some(vault_root.into());
        self
    }

    pub fn selected_format(&self) -> ExportFormat {
        ExportFormat::ALL[self.selected]
    }

    pub fn state(&self) -> &ExportState {
        &self.state
    }

    /// Moves the selection, wrapping; only while choosing.
    fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
        if !matches!(self.state, ExportState::Choosing | ExportState::Failed(_)) {
            return;
        }
        let count = ExportFormat::ALL.len();
        self.selected = if forward {
            (self.selected + 1) % count
        } else {
            (self.selected + count - 1) % count
        };
        cx.notify();
    }

    /// Enter: the main action of whatever the dialog shows.
    pub fn confirm(&mut self, cx: &mut Context<Self>) {
        match &self.state {
            ExportState::Choosing | ExportState::Failed(_) => self.start_export(cx),
            ExportState::Article(_) => self.copy_html(cx),
            ExportState::Saved(saved) => cx.open_with_system(&saved.path),
            ExportState::Exporting(_) => {}
        }
    }

    fn start_export(&mut self, cx: &mut Context<Self>) {
        let format = self.selected_format();
        self.state = ExportState::Exporting(format);
        cx.notify();
        match format {
            ExportFormat::Pdf => self.export_pdf(cx),
            ExportFormat::Html => self.export_html(cx),
        }
    }

    fn export_pdf(&mut self, cx: &mut Context<Self>) {
        let saving = save_pdf(
            self.text.clone(),
            self.note_path.clone(),
            self.vault_root.clone(),
            cx,
        );
        self.export_task = Some(cx.spawn(async move |this, cx| {
            let result = saving.await.map(|saved| {
                saved.map(|(path, pages)| Saved {
                    path,
                    detail: if pages == 1 {
                        "1 page".to_owned()
                    } else {
                        format!("{pages} pages")
                    },
                })
            });
            this.update(cx, |dialog, cx| dialog.finish_save(result, cx))
                .ok();
        }));
    }

    fn export_html(&mut self, cx: &mut Context<Self>) {
        let text = self.text.clone();
        let (note_path, vault_root) = (self.note_path.clone(), self.vault_root.clone());
        let exporting = cx.background_spawn(async move {
            html_article(&text, note_path.as_deref(), vault_root.as_deref())
        });
        self.export_task = Some(cx.spawn(async move |this, cx| {
            let export = exporting.await;
            this.update(cx, |dialog, cx| {
                dialog.state = ExportState::Article(Box::new(Article::new(export)));
                cx.notify();
            })
            .ok();
        }));
    }

    fn finish_save(&mut self, result: anyhow::Result<Option<Saved>>, cx: &mut Context<Self>) {
        self.state = match result {
            Ok(Some(saved)) => ExportState::Saved(saved),
            Ok(None) => ExportState::Choosing,
            Err(error) => {
                // The row has room for the outermost message; the log
                // keeps the whole chain.
                eprintln!("export failed: {error:#}");
                ExportState::Failed(error.to_string())
            }
        };
        cx.notify();
    }

    /// Puts the article's HTML on the clipboard.
    pub fn copy_html(&mut self, cx: &mut Context<Self>) {
        let ExportState::Article(article) = &mut self.state else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(article.export.html.clone()));
        article.status = Some(
            format!(
                "Copied {} of HTML to the clipboard.",
                byte_size(article.export.html.len())
            )
            .into(),
        );
        cx.notify();
    }

    /// Asks where to save the article, then writes it.
    pub fn save_html(&mut self, cx: &mut Context<Self>) {
        let ExportState::Article(article) = &self.state else {
            return;
        };
        let html = article.export.html.clone();
        let name = format!("{}.html", article.export.slug);
        let folder = save_folder(self.note_path.as_deref());
        let size = byte_size(html.len());
        let saving = save_with(
            &folder,
            &name,
            move |destination| {
                std::fs::write(destination, html).with_context(|| couldnt_write(destination))
            },
            cx,
        );
        self.export_task = Some(cx.spawn(async move |this, cx| {
            let result = saving.await;
            this.update(cx, |dialog, cx| dialog.finish_html_save(result, size, cx))
                .ok();
        }));
    }

    fn finish_html_save(
        &mut self,
        result: anyhow::Result<Option<(PathBuf, ())>>,
        size: String,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(Some((path, ()))) => {
                self.state = ExportState::Saved(Saved {
                    path,
                    detail: format!("{size} of HTML"),
                });
            }
            Ok(None) => {}
            Err(error) => {
                if let ExportState::Article(article) = &mut self.state {
                    article.status = Some(format!("{error:#}").into());
                }
            }
        }
        cx.notify();
    }

    /// Writes the article as a full page to the temporary folder and opens
    /// it in the browser, styled as the website will show it.
    fn preview_in_browser(&mut self, cx: &mut Context<Self>) {
        let ExportState::Article(article) = &self.state else {
            return;
        };
        let page = standalone_page(&article.export);
        let path = std::env::temp_dir().join(format!("{}.html", article.export.slug));
        match std::fs::write(&path, page) {
            Ok(()) => cx.open_with_system(&path),
            Err(error) => {
                if let ExportState::Article(article) = &mut self.state {
                    article.status = Some(format!("Couldn’t write the preview: {error}").into());
                }
                cx.notify();
            }
        }
    }

    fn choose(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.state.is_busy() {
            return;
        }
        self.selected = index;
        self.start_export(cx);
    }

    fn heading(&self, text: impl Into<SharedString>) -> Div {
        let ui = &self.theme;
        div()
            .px(ui.row_padding_x)
            .pt(ui.space_md)
            .pb(ui.space_md)
            .text_size(ui.font_size + gpui::px(2.))
            .child(crate::ui::truncated(text.into()))
    }

    /// A line of small text under the content. It keeps its height when
    /// empty, so a message appearing never moves the rest.
    fn status_line(&self, text: Option<SharedString>, color: gpui::Hsla) -> Div {
        let ui = &self.theme;
        div()
            .h(ui.text_line_height)
            .px(ui.row_padding_x)
            .text_size(ui.small_font_size)
            .text_color(color)
            .when_some(text, |line, text| line.child(crate::ui::truncated(text)))
    }

    fn button_row(&self) -> Div {
        let ui = &self.theme;
        div()
            .flex()
            .items_center()
            .justify_end()
            .gap(ui.space_md)
            .px(ui.row_padding_x)
            .pt(ui.space_md)
            .pb(ui.space_sm)
    }

    fn render_choice(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = &self.theme;
        let format = ExportFormat::ALL[index];
        let busy = self.state.is_busy();
        let exporting = matches!(self.state, ExportState::Exporting(active) if active == format);
        let failure = match &self.state {
            ExportState::Failed(message) if index == self.selected => Some(message.clone()),
            _ => None,
        };
        let detail_color = if failure.is_some() {
            ui.error
        } else {
            ui.text_detail
        };
        let detail: SharedString = match (exporting, format, failure) {
            (_, _, Some(message)) => message.into(),
            (true, ExportFormat::Pdf, _) => "Writing the PDF…".into(),
            (true, ExportFormat::Html, _) => "Making the article…".into(),
            (false, _, _) => format.detail().into(),
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
            .when(!selected && !busy, |row| {
                row.hover(|style| style.bg(ui.row_hover))
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.choose(index, cx)))
            .child(
                icon(format.icon())
                    .flex_none()
                    .size(ui.icon_size - gpui::px(2.))
                    .text_color(ui.icon),
            )
            .child(div().flex_none().text_color(ui.text).child(format.label()))
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

    fn render_choosing(&self, cx: &mut Context<Self>) -> AnyElement {
        let choices: Vec<_> = (0..ExportFormat::ALL.len())
            .map(|index| self.render_choice(index, cx).into_any_element())
            .collect();
        div()
            .flex()
            .flex_col()
            .child(self.heading(format!("Export {}", note_stem(self.note_path.as_deref()))))
            .children(choices)
            .into_any_element()
    }

    fn render_article(&self, article: &Article, cx: &mut Context<Self>) -> AnyElement {
        let ui = &self.theme;
        let url = format!("{ARTICLE_URL}{}", article.export.slug);
        let (status, color) = match (&article.status, article.warning()) {
            (Some(status), _) => (Some(status.clone()), ui.text_detail),
            (None, Some(warning)) => (Some(warning.into()), ui.error),
            (None, None) => (Some(SHORTENED_NOTE.into()), ui.text_faint),
        };
        div()
            .flex()
            .flex_col()
            .child(self.heading(article.export.title.clone()).pb(gpui::px(0.)))
            .child(
                div()
                    .px(ui.row_padding_x)
                    .pb(ui.space_md)
                    .text_size(ui.small_font_size)
                    .text_color(ui.text_detail)
                    .child(crate::ui::truncated(url)),
            )
            .child(self.render_source(article))
            .child(self.status_line(status, color).mt(ui.space_sm))
            .child(
                self.button_row()
                    .child(
                        Button::new("export-preview", "Preview in browser")
                            .quiet()
                            .on_click(cx.listener(|this, _, _, cx| this.preview_in_browser(cx))),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("export-save-html", "Save…")
                            .on_click(cx.listener(|this, _, _, cx| this.save_html(cx))),
                    )
                    .child(
                        Button::new("export-copy-html", "Copy HTML")
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| this.copy_html(cx))),
                    ),
            )
            .into_any_element()
    }

    /// The article's source in a scrolling box, in the code font.
    fn render_source(&self, article: &Article) -> impl IntoElement {
        let ui = &self.theme;
        let lines = article.lines.iter().cloned().map(|line| {
            // An empty line still takes a line's height.
            let line = if line.is_empty() { " ".into() } else { line };
            div().child(line)
        });
        let hidden = (article.hidden_lines > 0).then(|| {
            div()
                .pt(ui.space_sm)
                .font_family(ui.font_family.clone())
                .text_color(ui.text_faint)
                .child(format!(
                    "The preview stops here; {} more lines are copied and saved.",
                    article.hidden_lines
                ))
        });
        div()
            .id("export-html-source")
            .mx(ui.row_padding_x)
            .h(ui.source_preview_height)
            .overflow_y_scroll()
            .p(ui.space_md)
            .rounded(ui.row_radius)
            .bg(ui.source_background)
            .font_family(ui.code_font_family.clone())
            .text_size(ui.small_font_size)
            .text_color(ui.text)
            .children(lines)
            .children(hidden)
    }

    fn render_saved(&self, saved: &Saved) -> AnyElement {
        let ui = &self.theme;
        let name = saved
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let folder = saved
            .path
            .parent()
            .and_then(Path::file_name)
            .map(|folder| folder.to_string_lossy().into_owned());
        let caption = match folder {
            Some(folder) => format!("{}, saved in {folder}", saved.detail),
            None => saved.detail.clone(),
        };
        let path = saved.path.clone();
        let reveal = saved.path.clone();
        div()
            .flex()
            .flex_col()
            .child(self.heading(name).pb(gpui::px(0.)))
            .child(
                div()
                    .px(ui.row_padding_x)
                    .text_size(ui.small_font_size)
                    .text_color(ui.text_detail)
                    .child(crate::ui::truncated(caption)),
            )
            .child(
                self.button_row()
                    .pt(ui.space_lg)
                    .child(
                        Button::new("export-reveal", "Show in folder")
                            .on_click(move |_, _, cx| cx.reveal_path(&reveal)),
                    )
                    .child(
                        Button::new("export-open", "Open")
                            .primary()
                            .on_click(move |_, _, cx| cx.open_with_system(&path)),
                    ),
            )
            .into_any_element()
    }
}

impl Render for ExportDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The theme can change while the dialog is open.
        self.theme = crate::ui::ui_theme(cx);
        let ui = self.theme.clone();
        let (body, width) = match &self.state {
            ExportState::Article(article) => {
                (self.render_article(article, cx), ui.wide_dialog_width)
            }
            ExportState::Saved(saved) => (self.render_saved(saved), ui.small_dialog_width),
            _ => (self.render_choosing(cx), ui.small_dialog_width),
        };
        crate::ui::dialog(&ui)
            .key_context(EXPORT_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.step(true, cx)))
            .on_action(cx.listener(|this, _: &SelectPrevious, _, cx| this.step(false, cx)))
            .on_action(cx.listener(|this, _: &Confirm, _, cx| this.confirm(cx)))
            .on_action(cx.listener(|this, _: &SaveAs, _, cx| this.save_html(cx)))
            .on_action(cx.listener(|_, _: &Cancel, _, cx| cx.emit(DismissEvent)))
            .w(width)
            .p(ui.dialog_padding)
            .child(body)
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
    fn the_preview_shortens_images_and_long_articles() {
        let html = "<img src=\"data:image/png;base64,iVBORw0KGgo=\" alt=\"\">\n".repeat(250);
        let (lines, hidden) = preview_lines(&html);
        assert_eq!(lines.len(), PREVIEW_LINES);
        assert_eq!(hidden, 50);
        assert_eq!(
            lines[0].as_ref(),
            "<img src=\"data:image/png;base64,…\" alt=\"\">"
        );
        let (lines, _) = preview_lines(
            "<p>So <math><mi>x</mi></math> and <math display=\"block\"><mn>1</mn></math>.</p>",
        );
        assert_eq!(
            lines[0].as_ref(),
            "<p>So <math>…</math> and <math display=\"block\">…</math>.</p>"
        );
    }

    #[test]
    fn sizes_read_as_people_say_them() {
        assert_eq!(byte_size(100), "1 KB");
        assert_eq!(byte_size(8 * 1024 + 1), "9 KB");
        assert_eq!(byte_size(3 * 1024 * 1024 / 2), "1.5 MB");
    }
}
