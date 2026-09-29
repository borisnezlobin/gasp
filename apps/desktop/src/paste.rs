//! Smart paste and friends: images on the clipboard become files in the
//! attachments folder plus an embed, a URL pasted over a selection becomes
//! a link, copy and cut with nothing selected take the whole line, and
//! dropped or imported files are copied in and embedded or linked.
//!
//! The editor needs to know which note it shows to save attachments next
//! to it. `EditorView` has no field for that, so the workspace registers a
//! [`PasteContext`] per editor with [`set_paste_context`]. The contexts live
//! in a GPUI global keyed by the editor's entity id and are dropped with the
//! editor.

use std::collections::HashMap;
use std::io;
use std::ops::Range;
use std::path::{Component, Path, PathBuf};

use editor_config::Settings;
use editor_config::settings::FileSettings;
use editor_core::document::Selection;
use editor_core::motion;
use editor_core::transaction::{ChangeSet, Origin, Transaction};
use gpui::{
    App, ClipboardEntry, ClipboardItem, Context, Entity, EntityId, ExternalPaths, Global, Image,
    ImageFormat, PathPromptOptions, Window,
};

pub use editor_vault::attachments::{
    attachments_dir, embed, next_attachment_name, save_attachment,
};

use crate::editor::EditorView;

/// Clipboard metadata that marks a whole line copied with nothing selected.
const WHOLE_LINE: &str = "whole-line";

/// File extensions embedded as images when dropped or imported.
const IMAGE_EXTENSIONS: [&str; 10] = [
    "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "tif", "tiff", "avif",
];

/// Where the editor's note lives and where its attachments go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteContext {
    /// The note's file; `None` for a note that hasn't been saved yet.
    pub note_path: Option<PathBuf>,
    /// The `files.attachments-folder` setting. Relative paths are relative
    /// to the note's folder.
    pub attachments: String,
}

impl Default for PasteContext {
    fn default() -> Self {
        Self {
            note_path: None,
            attachments: FileSettings::default().attachments_folder,
        }
    }
}

impl PasteContext {
    pub fn new(note_path: Option<PathBuf>, settings: &Settings) -> Self {
        Self {
            note_path,
            attachments: settings.files.attachments_folder.clone(),
        }
    }

    /// The folder attachments are saved into, when the note has a path.
    pub fn attachments_dir(&self) -> Option<PathBuf> {
        let note = self.note_path.as_deref()?;
        Some(attachments_dir(note, &self.attachments))
    }

    fn note_stem(&self) -> String {
        self.note_path
            .as_deref()
            .and_then(Path::file_stem)
            .map_or_else(
                || "Untitled".to_owned(),
                |stem| stem.to_string_lossy().into_owned(),
            )
    }
}

#[derive(Default)]
struct PasteContexts(HashMap<EntityId, PasteContext>);

impl Global for PasteContexts {}

/// Tells `editor` which note it shows. Call it when the editor opens a
/// note, and again when the note is renamed or the setting changes.
pub fn set_paste_context(editor: &Entity<EditorView>, context: PasteContext, cx: &mut App) {
    let id = editor.entity_id();
    let contexts = cx.default_global::<PasteContexts>();
    let is_new = contexts.0.insert(id, context).is_none();
    if is_new {
        cx.observe_release(editor, move |_, cx| {
            cx.default_global::<PasteContexts>().0.remove(&id);
        })
        .detach();
    }
}

/// The context registered for the editor with this id, or the default.
pub fn paste_context(editor: EntityId, cx: &App) -> PasteContext {
    cx.try_global::<PasteContexts>()
        .and_then(|contexts| contexts.0.get(&editor).cloned())
        .unwrap_or_default()
}

pub fn image_extension(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Webp => "webp",
        ImageFormat::Gif => "gif",
        ImageFormat::Svg => "svg",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Tiff => "tiff",
    }
}

/// Whether `text` is a single URL.
pub fn is_url(text: &str) -> bool {
    ["http://", "https://", "mailto:"]
        .iter()
        .any(|scheme| text.starts_with(scheme) && text.len() > scheme.len())
        && !text.contains(char::is_whitespace)
}

/// The text a plain-text paste inserts: a URL over a one-line selection
/// links the selection; anything else is pasted as is.
pub fn text_to_paste(selected: &str, pasted: &str) -> String {
    let url = pasted.trim();
    let links =
        is_url(url) && !selected.is_empty() && !selected.contains('\n') && !is_url(selected);
    if links {
        format!("[{selected}]({url})")
    } else {
        pasted.to_owned()
    }
}

pub fn is_image_path(path: &Path) -> bool {
    path.extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .is_some_and(|ext| IMAGE_EXTENSIONS.contains(&ext.as_str()))
}

/// `path` relative to the folder `base`, when both are absolute and share
/// a root; otherwise `path` unchanged.
pub fn relative_path(path: &Path, base: &Path) -> PathBuf {
    let path_parts: Vec<Component> = path.components().collect();
    let base_parts: Vec<Component> = base.components().collect();
    let shared = path_parts
        .iter()
        .zip(&base_parts)
        .take_while(|(a, b)| a == b)
        .count();
    let shares_root = shared > 0 && path.has_root() && base.has_root();
    if !shares_root {
        return path.to_path_buf();
    }
    let ups = base_parts.len() - shared;
    let mut relative: PathBuf = std::iter::repeat_n(Component::ParentDir, ups).collect();
    relative.extend(&path_parts[shared..]);
    relative
}

/// A Markdown link to `path`, relative to the note's folder when there is
/// one, with forward slashes and angle brackets around paths with spaces.
pub fn markdown_link(path: &Path, note_dir: Option<&Path>) -> String {
    let name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let target = note_dir.map_or_else(|| path.to_path_buf(), |dir| relative_path(path, dir));
    let target = target.to_string_lossy().replace('\\', "/");
    if target.contains(' ') {
        format!("[{name}](<{target}>)")
    } else {
        format!("[{name}]({target})")
    }
}

/// The Markdown for dropped or imported files: images are copied into the
/// attachments folder and embedded, other files are linked. One file per
/// line.
pub fn files_markup(paths: &[PathBuf], context: &PasteContext) -> io::Result<String> {
    let note_dir = context.note_path.as_deref().and_then(Path::parent);
    let mut lines = Vec::with_capacity(paths.len());
    for path in paths {
        let line = match (is_image_path(path), context.attachments_dir()) {
            (true, Some(dir)) => embed(&copy_attachment(path, &dir, &context.note_stem())?),
            (true, None) => format!("!{}", markdown_link(path, note_dir)),
            (false, _) => markdown_link(path, note_dir),
        };
        lines.push(line);
    }
    Ok(lines.join("\n"))
}

fn copy_attachment(source: &Path, dir: &Path, note_stem: &str) -> io::Result<String> {
    let extension = source.extension().map_or_else(
        || "png".to_owned(),
        |ext| ext.to_string_lossy().to_lowercase(),
    );
    let bytes = std::fs::read(source)?;
    save_attachment(dir, note_stem, &extension, &bytes)
}

/// Whether a clipboard item holds a whole line copied with no selection.
fn is_whole_line(item: &ClipboardItem) -> bool {
    item.entries().iter().any(|entry| match entry {
        ClipboardEntry::String(string) => {
            string.metadata_json::<String>().as_deref() == Some(WHOLE_LINE)
        }
        ClipboardEntry::Image(_) => false,
    })
}

fn clipboard_image(item: &ClipboardItem) -> Option<&Image> {
    item.entries().iter().find_map(|entry| match entry {
        ClipboardEntry::Image(image) => Some(image),
        ClipboardEntry::String(_) => None,
    })
}

impl EditorView {
    fn paste_context(&self, cx: &Context<Self>) -> PasteContext {
        paste_context(cx.entity_id(), cx)
    }

    /// The selection, or the whole line under the cursor (with its line
    /// break) when nothing is selected. The flag says which.
    fn selection_or_line(&self) -> (Range<usize>, bool) {
        let range = self.selected_range();
        if range.is_empty() {
            (motion::line_at(self.doc(), range.start), true)
        } else {
            (range, false)
        }
    }

    fn write_clipboard(&self, range: Range<usize>, whole_line: bool, cx: &mut Context<Self>) {
        let mut text = self.doc().slice(range);
        let item = if whole_line {
            if !text.ends_with('\n') {
                text.push('\n');
            }
            ClipboardItem::new_string_with_json_metadata(text, WHOLE_LINE)
        } else {
            ClipboardItem::new_string(text)
        };
        cx.write_to_clipboard(item);
    }

    /// `edit.copy`: the selection, or the whole line when nothing is
    /// selected.
    pub(crate) fn copy_selection_or_line(&mut self, cx: &mut Context<Self>) {
        let (range, whole_line) = self.selection_or_line();
        if !range.is_empty() {
            self.write_clipboard(range, whole_line, cx);
        }
    }

    /// `edit.cut`: like copy, then deletes what was copied.
    pub(crate) fn cut_selection_or_line(&mut self, cx: &mut Context<Self>) {
        let (mut range, whole_line) = self.selection_or_line();
        if range.is_empty() {
            return;
        }
        self.write_clipboard(range.clone(), whole_line, cx);
        let is_last_line = !self.doc().slice(range.clone()).ends_with('\n');
        if whole_line && is_last_line && range.start > 0 {
            range.start = self.doc().prev_char_boundary(range.start);
        }
        self.paste_text_at(range, "", cx);
    }

    /// `edit.paste`: images become attachments, URLs over a selection
    /// become links, and whole lines go above the cursor's line.
    pub(crate) fn smart_paste(&mut self, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        if let Some(image) = clipboard_image(&item) {
            self.paste_image(&image.clone(), cx);
            return;
        }
        let Some(text) = item.text() else {
            return;
        };
        let selection = self.selected_range();
        // A whole line goes above the caret's line, except into a cell,
        // where it's flattened into the cell like any pasted lines.
        let in_cell = self.grid_table(selection.start).is_some();
        if is_whole_line(&item) && selection.is_empty() && !in_cell {
            let line_start = motion::line_start(self.doc(), selection.start);
            let text = self.curl_pasted(&text, line_start);
            let cursor = selection.start + text.len();
            self.apply_paste(line_start..line_start, &text, cursor, cx);
            return;
        }
        let selected = self.doc().slice(selection.clone());
        let line = motion::line_at(self.doc(), selection.start);
        let line_was_empty = selection.is_empty() && self.doc().slice(line).trim().is_empty();
        let text = self.curl_pasted(&text, selection.start);
        self.paste_text_at(selection, &text_to_paste(&selected, &text), cx);
        self.offer_card_after_paste(line_was_empty, cx);
    }

    /// `edit.paste-plain`: the clipboard's text, exactly, straight quotes
    /// and all.
    pub(crate) fn paste_plain(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.paste_text_at(self.selected_range(), &text, cx);
        }
    }

    fn paste_image(&mut self, image: &Image, cx: &mut Context<Self>) {
        let context = self.paste_context(cx);
        let Some(dir) = context.attachments_dir() else {
            // Notices have no surface yet.
            eprintln!("save the note before pasting an image");
            return;
        };
        let extension = image_extension(image.format);
        match save_attachment(&dir, &context.note_stem(), extension, &image.bytes) {
            Ok(name) => self.paste_text_at(self.selected_range(), &embed(&name), cx),
            Err(error) => eprintln!("could not save the pasted image: {error}"),
        }
    }

    /// Replaces `range` with `text` as its own undo step, with the cursor
    /// after it.
    fn paste_text_at(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        // Pasting over several cells empties them and pastes into the
        // first; in a cell, pipes are escaped and line breaks are spaces.
        let over_cells = self.cell_block().is_some();
        let typed = self.cell_pasting(text, cx);
        let range = if over_cells {
            self.selected_range()
        } else {
            range
        };
        let text = typed.unwrap_or_else(|| text.to_owned());
        let cursor = range.start + text.len();
        self.apply_paste(range, &text, cursor, cx);
    }

    fn apply_paste(
        &mut self,
        range: Range<usize>,
        text: &str,
        cursor: usize,
        cx: &mut Context<Self>,
    ) {
        let transaction = Transaction::new(
            ChangeSet::replace(range, text),
            Origin::command("edit.paste"),
            self.now_ms(),
        )
        .with_selection(Selection::cursor(cursor));
        self.apply_transaction(transaction, cx);
    }

    /// Copies or links `paths` at `offset`.
    pub fn insert_files(&mut self, paths: &[PathBuf], offset: usize, cx: &mut Context<Self>) {
        let context = self.paste_context(cx);
        match files_markup(paths, &context) {
            Ok(markup) if !markup.is_empty() => self.paste_text_at(offset..offset, &markup, cx),
            Ok(_) => {}
            Err(error) => eprintln!("could not add the files: {error}"),
        }
    }

    /// Files dropped on the editor land where the pointer is.
    pub(crate) fn on_drop_paths(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
        let offset = self.offset_for_point(window.mouse_position(), window);
        self.insert_files(paths.paths(), offset, cx);
    }

    /// `note.import-image`: picks image files and embeds them at the cursor.
    pub(crate) fn import_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Insert".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            this.update(cx, |view, cx| {
                let cursor = view.cursor();
                view.insert_files(&paths, cursor, cx);
            })
            .ok();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_over_a_selection_become_links() {
        assert_eq!(
            text_to_paste("the docs", "https://example.com/a"),
            "[the docs](https://example.com/a)"
        );
        assert_eq!(
            text_to_paste("", "https://example.com"),
            "https://example.com"
        );
        assert_eq!(
            text_to_paste("two\nlines", "https://x.org"),
            "https://x.org"
        );
        assert_eq!(text_to_paste("word", "plain text"), "plain text");
        assert_eq!(text_to_paste("word", "https://a b"), "https://a b");
    }

    #[test]
    fn image_paths_are_recognised_by_extension() {
        assert!(is_image_path(Path::new("a/b.PNG")));
        assert!(is_image_path(Path::new("x.jpeg")));
        assert!(!is_image_path(Path::new("x.pdf")));
        assert!(!is_image_path(Path::new("png")));
    }

    #[test]
    fn links_are_relative_to_the_note() {
        let dir = Path::new("/vault/Notes");
        assert_eq!(
            markdown_link(Path::new("/vault/Notes/files/a.pdf"), Some(dir)),
            "[a.pdf](files/a.pdf)"
        );
        assert_eq!(
            markdown_link(Path::new("/vault/Other/my file.pdf"), Some(dir)),
            "[my file.pdf](<../Other/my file.pdf>)"
        );
        assert_eq!(
            markdown_link(Path::new("/tmp/a.zip"), None),
            "[a.zip](/tmp/a.zip)"
        );
    }

    #[test]
    fn dropped_images_are_copied_and_other_files_linked() {
        let vault = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let picture = outside.path().join("shot.PNG");
        let paper = outside.path().join("paper.pdf");
        std::fs::write(&picture, b"png bytes").unwrap();
        std::fs::write(&paper, b"pdf").unwrap();
        let context = PasteContext {
            note_path: Some(vault.path().join("Lemma.md")),
            attachments: "./images".to_owned(),
        };
        let markup = files_markup(&[picture.clone(), paper.clone(), picture], &context).unwrap();
        let lines: Vec<&str> = markup.lines().collect();
        assert_eq!(lines[0], "![[Lemma-1.png]]");
        assert!(lines[1].starts_with("[paper.pdf]("));
        assert_eq!(lines[2], "![[Lemma-2.png]]");
        let saved = std::fs::read(vault.path().join("images/Lemma-1.png")).unwrap();
        assert_eq!(saved, b"png bytes");
    }

    #[test]
    fn unsaved_notes_link_images_in_place() {
        let markup = files_markup(&[PathBuf::from("/pics/a.png")], &PasteContext::default());
        assert_eq!(markup.unwrap(), "![a.png](/pics/a.png)");
    }
}
