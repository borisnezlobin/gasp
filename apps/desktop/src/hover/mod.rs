//! Hover previews: resting the pointer on a wikilink or a link to a note
//! shows that note in a popover, rendered by a read-only [`EditorView`]
//! and scrolled to the linked heading; resting it on a footnote reference
//! shows the footnote. The popover opens after a short pause, or at once
//! while Mod is held, and closes once the pointer has left both the link
//! and the popover.
//!
//! The previewed note is read and parsed on a background thread, and
//! previews are kept by path and modification time, so hovering the same
//! link again costs nothing.

pub mod popover;
pub mod resolve;

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use editor_config::Config;
use editor_config::settings::SymbolMode;
use editor_core::footnotes::{find_def, parse_footnotes};
use gpui::{AppContext, Context, Entity, Pixels, Point, Subscription, Task, Window};

use self::resolve::{NoteLink, resolve};
use crate::editor::{EditorEvent, EditorView};
use crate::line_layout::Hit;
use crate::outline::headings_in;
use crate::preview::links::{HoverTarget, hover_target_at};
use crate::preview::source::Source;

/// Previews kept for hovering again.
const CACHED_PREVIEWS: usize = 8;

/// What the popover shows.
pub enum PreviewContent {
    /// The note is being read; nothing shows yet, so a quick load never
    /// flashes an empty popover.
    Loading,
    Note {
        title: String,
        folder: String,
        /// The link to follow to open the note at the heading.
        link: String,
        view: Entity<EditorView>,
    },
    /// The link names a note that doesn't exist yet.
    Missing { link: NoteLink },
    /// A footnote's text.
    Footnote { view: Entity<EditorView> },
    /// Something to say instead, such as a footnote with no definition.
    Message(String),
    /// What the grammar checker found, with its fixes.
    Flag(editor_prose::Flag),
}

/// An open popover and the link it belongs to.
pub struct OpenPreview {
    pub target: HoverTarget,
    /// The link's range in this note, which the popover hangs from.
    pub range: Range<usize>,
    pub content: PreviewContent,
}

/// A preview kept for hovering again.
struct Cached {
    path: PathBuf,
    modified: Option<SystemTime>,
    view: Entity<EditorView>,
    _events: Subscription,
}

/// The editor's hover state.
#[derive(Default)]
pub struct HoverState {
    /// The link under the pointer, and its range.
    over: Option<(HoverTarget, Range<usize>)>,
    pub(crate) open: Option<OpenPreview>,
    /// Opens or closes the popover when it fires.
    timer: Option<Task<()>>,
    load: Option<Task<()>>,
    in_popover: bool,
    cache: Vec<Cached>,
}

/// A note read and parsed off the main thread.
struct Loaded {
    path: PathBuf,
    modified: Option<SystemTime>,
    /// `None` when the cached preview is still current.
    source: Option<Source>,
}

impl EditorView {
    /// The open preview, if any.
    pub fn hover_preview(&self) -> Option<&OpenPreview> {
        self.hover.open.as_ref()
    }

    /// What a hover at `position` would preview: the link under the text
    /// there, not merely the nearest one.
    pub(crate) fn hover_target_at_point(
        &self,
        position: Point<Pixels>,
    ) -> Option<(HoverTarget, Range<usize>)> {
        let frame = self.frame.as_ref()?;
        let (placed, piece) = frame.piece_at(position)?;
        let offset = match piece.hit {
            Hit::Text => frame.offset_at(position)?,
            _ => placed.visual.start + piece.range.start,
        };
        hover_target_at(self.source.tree(), offset)
            .or_else(|| {
                let problem = self.footnote_problem_at(offset)?;
                let target = HoverTarget::Problem {
                    message: problem.message.clone(),
                };
                Some((target, problem.range.clone()))
            })
            .or_else(|| {
                let flag = self.flag_at(offset)?;
                Some((HoverTarget::Flag, flag.range.clone()))
            })
    }

    /// The pointer moved in the text. `now` opens a preview without the
    /// pause, as while Mod is held.
    pub(crate) fn hover_moved(
        &mut self,
        over: Option<(HoverTarget, Range<usize>)>,
        now: bool,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let same = over.as_ref().map(|(_, range)| range) == self.hover.over.as_ref().map(|o| &o.1);
        let open_here = self
            .hover
            .open
            .as_ref()
            .is_some_and(|open| Some(&open.range) == over.as_ref().map(|(_, range)| range));
        if same && (open_here || !now) {
            if open_here {
                self.hover.timer = None;
            }
            return;
        }
        self.hover.over = over.clone();
        match over {
            Some(_) if open_here => self.hover.timer = None,
            Some((target, range)) => {
                self.close_preview(cx);
                let delay = if now {
                    Duration::ZERO
                } else {
                    crate::ui::ui_theme(cx).hover_preview_delay
                };
                self.hover.timer = Some(cx.spawn(async move |view, cx| {
                    cx.background_executor().timer(delay).await;
                    view.update(cx, |view, cx| view.open_preview(target, range, cx))
                        .ok();
                }));
            }
            None if self.hover.open.is_some() => self.close_preview_soon(cx),
            None => self.hover.timer = None,
        }
    }

    /// Mod was pressed or released while the pointer rests on a link.
    pub(crate) fn hover_modifiers_changed(&mut self, mod_held: bool, cx: &mut Context<Self>) {
        let waiting = self.hover.open.is_none() && self.hover.over.is_some();
        if mod_held && waiting {
            let over = self.hover.over.take();
            self.hover_moved(over, true, cx);
        }
    }

    /// The pointer entered or left the popover.
    pub(crate) fn hover_popover_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        self.hover.in_popover = hovered;
        if hovered {
            self.hover.timer = None;
        } else if self.hover.over.is_none() {
            self.close_preview_soon(cx);
        }
    }

    fn close_preview_soon(&mut self, cx: &mut Context<Self>) {
        let delay = crate::ui::ui_theme(cx).hover_preview_grace;
        self.hover.timer = Some(cx.spawn(async move |view, cx| {
            cx.background_executor().timer(delay).await;
            view.update(cx, |view, cx| {
                if !view.hover.in_popover && view.hover.over.is_none() {
                    view.close_preview(cx);
                }
            })
            .ok();
        }));
    }

    /// Closes the popover now, as on Escape, scrolling or typing.
    pub fn close_preview(&mut self, cx: &mut Context<Self>) {
        self.hover.timer = None;
        self.hover.load = None;
        self.hover.in_popover = false;
        if self.hover.open.take().is_some() {
            cx.notify();
        }
    }

    /// Forgets the pointer entirely, as when it leaves the editor.
    pub(crate) fn hover_left(&mut self, cx: &mut Context<Self>) {
        self.hover_moved(None, false, cx);
    }

    fn open_preview(&mut self, target: HoverTarget, range: Range<usize>, cx: &mut Context<Self>) {
        let content = match &target {
            HoverTarget::Footnote { label } => self.footnote_preview(label, cx),
            HoverTarget::Problem { message } => PreviewContent::Message(message.clone()),
            HoverTarget::Flag => match self.flag_at(range.start) {
                Some(flag) if flag.range == range => PreviewContent::Flag(flag.clone()),
                _ => return,
            },
            HoverTarget::Note { link } => match NoteLink::parse(link) {
                Some(link) => self.note_preview(link, cx),
                None => return,
            },
        };
        self.hover.open = Some(OpenPreview {
            target,
            range,
            content,
        });
        cx.notify();
    }

    /// A footnote's text in a small read-only view, or what's wrong with
    /// it. The text is short, so it's read here.
    fn footnote_preview(&mut self, label: &str, cx: &mut Context<Self>) -> PreviewContent {
        let parsed = parse_footnotes(self.source.text());
        let Some(def) = find_def(&parsed.defs, label) else {
            return PreviewContent::Message(format!("Footnote {label} has no definition yet."));
        };
        let body = dedent(&def.body);
        if body.trim().is_empty() {
            return PreviewContent::Message(format!("Footnote {label} has no text yet."));
        }
        let view = self.preview_view(Source::new(body.trim()), None, cx);
        PreviewContent::Footnote { view }
    }

    /// Starts reading the linked note, or says it doesn't exist.
    fn note_preview(&mut self, link: NoteLink, cx: &mut Context<Self>) -> PreviewContent {
        let index = self.suggest.index.clone();
        let from = crate::paste::paste_context(cx.entity_id(), cx).note_path;
        let Some(index) = index else {
            return PreviewContent::Missing { link };
        };
        let index = index.read(cx);
        let root = index.root().to_path_buf();
        let from_relative = from
            .as_deref()
            .and_then(|path| crate::vault_index::vault_relative(&root, path));
        let Some(found) = resolve(index, from_relative.as_deref(), &link) else {
            return PreviewContent::Missing { link };
        };
        let path = root.join(&found);
        let cached = self
            .hover
            .cache
            .iter()
            .find(|cached| cached.path == path)
            .map(|cached| cached.modified);
        let load = cx.background_spawn(read_note(path, cached));
        self.hover.load = Some(cx.spawn(async move |view, cx| {
            let loaded = load.await;
            view.update(cx, |view, cx| view.show_note(loaded, link, &found, cx))
                .ok();
        }));
        PreviewContent::Loading
    }

    fn show_note(
        &mut self,
        loaded: Option<Loaded>,
        link: NoteLink,
        found: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.hover.open.as_mut() else {
            return;
        };
        let Some(mut loaded) = loaded else {
            open.content = PreviewContent::Missing { link };
            return cx.notify();
        };
        let view = match loaded.source.take() {
            Some(source) => {
                let view = self.preview_view(source, Some(&loaded.path), cx);
                self.remember_preview(&loaded, view.clone(), cx);
                view
            }
            None => self.cached_preview(&loaded.path),
        };
        view.update(cx, |view, cx| {
            let heading = link.heading.as_deref().and_then(|h| view.heading_offset(h));
            view.scroll_to_offset(heading.unwrap_or(0), cx)
        });
        let (folder, file) = found.rsplit_once('/').unwrap_or(("", found));
        let content = PreviewContent::Note {
            title: file.strip_suffix(".md").unwrap_or(file).to_owned(),
            folder: folder.to_owned(),
            link: link.target(),
            view,
        };
        if let Some(open) = self.hover.open.as_mut() {
            open.content = content;
        }
        cx.notify();
    }

    fn cached_preview(&mut self, path: &Path) -> Entity<EditorView> {
        let at = self
            .hover
            .cache
            .iter()
            .position(|cached| cached.path == path)
            .expect("a preview is only reused while it's cached");
        let cached = self.hover.cache.remove(at);
        let view = cached.view.clone();
        self.hover.cache.push(cached);
        view
    }

    /// Keeps a preview for hovering again, following its links, dropping
    /// the one used longest ago once there are enough.
    fn remember_preview(
        &mut self,
        loaded: &Loaded,
        view: Entity<EditorView>,
        cx: &mut Context<Self>,
    ) {
        self.hover.cache.retain(|cached| cached.path != loaded.path);
        if self.hover.cache.len() >= CACHED_PREVIEWS {
            self.hover.cache.remove(0);
        }
        let events = cx.subscribe(&view, |this, _, event: &EditorEvent, cx| {
            if let EditorEvent::OpenLink(target) = event {
                this.close_preview(cx);
                cx.emit(EditorEvent::OpenLink(target.clone()));
            }
        });
        self.hover.cache.push(Cached {
            path: loaded.path.clone(),
            modified: loaded.modified,
            view,
            _events: events,
        });
    }

    /// A read-only view in this editor's theme, zoomed down a step so a
    /// glance shows more, with every Markdown symbol hidden.
    fn preview_view(
        &self,
        source: Source,
        path: Option<&Path>,
        cx: &mut Context<Self>,
    ) -> Entity<EditorView> {
        let ui = crate::ui::ui_theme(cx);
        let mut base_theme = self.base_theme.clone();
        base_theme.text_padding = ui.hover_preview_padding / ui.hover_preview_zoom;
        let mut symbols = self.symbols.clone();
        symbols.mode = SymbolMode::AlwaysHidden;
        let image_dirs: Vec<PathBuf> = path
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .into_iter()
            .chain(
                self.suggest
                    .index
                    .as_ref()
                    .map(|i| i.read(cx).root().to_path_buf()),
            )
            .collect();
        cx.new(|cx| {
            let mut view = EditorView::with_source(source, image_dirs, &Config::defaults(), cx);
            view.read_only = true;
            view.base_theme = base_theme;
            view.symbols = symbols;
            view.reveal = crate::preview::reveal::reveal_settings(&view.symbols);
            view.set_zoom(ui.hover_preview_zoom, cx);
            view
        })
    }

    /// Where the heading titled `title` starts, ignoring case. The tree is
    /// already parsed, so this only walks the headings.
    fn heading_offset(&self, title: &str) -> Option<usize> {
        headings_in(self.source.tree(), self.source.text())
            .into_iter()
            .find(|found| found.title.eq_ignore_ascii_case(title.trim()))
            .map(|found| found.offset)
    }

    /// Scrolls so the line holding `offset` is at the top, and keeps it
    /// there until the reader scrolls.
    pub fn scroll_to_offset(&mut self, offset: usize, cx: &mut Context<Self>) {
        let line = self.source.line_of(offset.min(self.source.text().len()));
        self.scroll_y = self.header_height + self.metrics.top_of(line);
        self.pinned_top = Some(offset);
        cx.notify();
    }

    /// The height the whole note takes, padding included, as measured so
    /// far; lines not yet laid out count at their estimate.
    pub fn content_height(&self) -> Pixels {
        self.header_height + self.metrics.total_height() + self.theme.text_padding * 2.
    }

    /// Opens the previewed note, as clicking the popover's title does.
    pub(crate) fn open_previewed(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let link = match self.hover.open.as_ref().map(|open| &open.content) {
            Some(PreviewContent::Note { link, .. }) => link.clone(),
            Some(PreviewContent::Missing { link }) => link.target(),
            _ => return,
        };
        self.close_preview(cx);
        cx.emit(EditorEvent::OpenLink(link));
    }

    /// Forgets kept previews, as when the theme changes.
    pub(crate) fn clear_preview_cache(&mut self) {
        self.hover.cache.clear();
        self.line_cache.clear();
    }
}

/// Reads and parses the note at `path`, unless `cached_modified` says the
/// kept preview is still current. `None` when the file can't be read.
async fn read_note(path: PathBuf, cached_modified: Option<Option<SystemTime>>) -> Option<Loaded> {
    let modified = std::fs::metadata(&path).ok()?.modified().ok();
    let current = cached_modified.is_some_and(|cached| cached.is_some() && cached == modified);
    let source = if current {
        None
    } else {
        Some(Source::new(&std::fs::read_to_string(&path).ok()?))
    };
    Some(Loaded {
        path,
        modified,
        source,
    })
}

/// A footnote body with its continuation lines' indent removed.
fn dedent(body: &str) -> String {
    body.lines()
        .map(|line| {
            line.strip_prefix("    ")
                .or_else(|| line.strip_prefix('\t'))
                .unwrap_or(line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footnote_continuation_lines_lose_their_indent() {
        assert_eq!(dedent(" First.\n    Second."), " First.\nSecond.");
    }
}
