//! Embedded notes: `![[Note]]`, `![[Note#Heading]]` and `![[Note#^block]]`
//! drawn as a card holding the note's text, rendered read-only by a small
//! [`EditorView`] of its own, with a header that opens the note.
//!
//! A card is made only when its line is laid out, from the text the vault
//! index already holds, so nothing is read from disk. Cards are kept by
//! what they embed while the note is open (at most [`KEPT`]), follow the
//! source note whenever the index learns it changed, show at most
//! `embed.max-lines` lines, and stop at [`MAX_DEPTH`] or where a note
//! would embed itself, saying so.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gasp_config::Config;
use gasp_config::settings::SymbolMode;
use gasp_core::embed::{EmbedPart, embedded_text};
use gasp_core::syntax::parse;
use gpui::{
    AnyElement, AppContext, Bounds, ContentMask, Context, Entity, IntoElement, ParentElement,
    Pixels, Styled, Subscription, Window, div, px,
};

use crate::editor::{EditorEvent, EditorView};
use crate::frame::FrameLayout;
use crate::hover::resolve::{NoteLink, resolve};
use crate::line_layout::PieceContent;
use crate::preview::source::Source;
use crate::vault_index::VaultIndex;

/// How deeply embeds nest before a card says it stops.
pub const MAX_DEPTH: usize = 4;

/// Cards kept per editor; the ones unseen longest go first.
pub const KEPT: usize = 24;

/// What an embed names: the note as written and the part after `#`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EmbedKey {
    pub target: String,
    pub subpath: Option<String>,
}

impl EmbedKey {
    pub fn new(target: &str, subpath: Option<&str>) -> EmbedKey {
        EmbedKey {
            target: target.to_string(),
            subpath: subpath.map(str::to_string),
        }
    }

    /// The link that opens the note there, as the workspace follows it.
    pub fn link(&self) -> String {
        match &self.subpath {
            Some(subpath) => format!("{}#{subpath}", self.target),
            None => self.target.clone(),
        }
    }

    /// The card's title: the note's name, and the heading or block.
    pub fn title(&self) -> String {
        let name = self.target.rsplit('/').next().unwrap_or(&self.target);
        let name = name.strip_suffix(".md").unwrap_or(name);
        match (&self.subpath, name.is_empty()) {
            (Some(subpath), true) => subpath.clone(),
            (Some(subpath), false) => format!("{name} \u{203a} {subpath}"),
            (None, _) => name.to_string(),
        }
    }

    /// The name a missing note would be made with.
    pub fn name(&self) -> String {
        let name = self.target.rsplit('/').next().unwrap_or(&self.target);
        name.strip_suffix(".md").unwrap_or(name).to_string()
    }
}

/// A note and part of it, as the embed chain records them.
type Shown = (String, Option<String>);

/// What a card holds.
pub enum EmbedContent {
    /// The embedded text, drawn by a read-only view.
    Note {
        view: Entity<EditorView>,
        /// The note's text the view was made from.
        source: Arc<str>,
        path: String,
        _events: Subscription,
    },
    /// No note has that name.
    Missing,
    /// Something to say instead of the text.
    Message(String),
}

/// A card and when it was last laid out.
pub struct Embed {
    pub content: EmbedContent,
    /// The height of the part of the note the card shows.
    pub body_height: Pixels,
    /// Whether the note is longer than the card shows.
    pub clipped: bool,
    /// Whether the view still needs measuring at the card's width.
    needs_measure: bool,
    last_used: u64,
}

/// What a card looks like to line layout.
#[derive(Clone, Debug, PartialEq)]
pub enum EmbedLook {
    /// Not made yet: it's made before the frame is drawn.
    Pending,
    Note {
        body_height: Pixels,
        clipped: bool,
    },
    Missing,
    Message(String),
}

/// An editor's cards, and where it sits among embeds.
#[derive(Default)]
pub struct EmbedStore {
    embeds: HashMap<EmbedKey, Embed>,
    /// Embeds asked for by layout that aren't made yet.
    wanted: Vec<EmbedKey>,
    frame: u64,
    /// How deep this editor is inside other cards; 0 for a note's own.
    depth: usize,
    /// The notes and parts shown by the cards around this one, its own
    /// first, to catch a note embedding itself.
    chain: Vec<Shown>,
    /// This editor's note, vault-relative, for a card's view, whose
    /// workspace doesn't know it.
    origin: Option<String>,
}

impl EmbedStore {
    /// How a card for `key` looks, noting it's in use.
    pub fn look(&mut self, key: &EmbedKey) -> EmbedLook {
        let frame = self.frame;
        let Some(embed) = self.embeds.get_mut(key) else {
            if !self.wanted.contains(key) {
                self.wanted.push(key.clone());
            }
            return EmbedLook::Pending;
        };
        embed.last_used = frame;
        match &embed.content {
            EmbedContent::Note { .. } => EmbedLook::Note {
                body_height: embed.body_height,
                clipped: embed.clipped,
            },
            EmbedContent::Missing => EmbedLook::Missing,
            EmbedContent::Message(message) => EmbedLook::Message(message.clone()),
        }
    }

    /// The view drawing `key`'s note, when it has one.
    pub fn view(&self, key: &EmbedKey) -> Option<&Entity<EditorView>> {
        match &self.embeds.get(key)?.content {
            EmbedContent::Note { view, .. } => Some(view),
            _ => None,
        }
    }

    /// The card for `key`, once it's made.
    pub fn get(&self, key: &EmbedKey) -> Option<&Embed> {
        self.embeds.get(key)
    }

    pub fn len(&self) -> usize {
        self.embeds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.embeds.is_empty()
    }

    pub fn clear(&mut self) {
        self.embeds.clear();
        self.wanted.clear();
    }

    /// Lets go of the cards unseen longest once there are too many.
    fn trim(&mut self) {
        while self.embeds.len() > KEPT {
            let oldest = self
                .embeds
                .iter()
                .min_by_key(|(_, embed)| embed.last_used)
                .map(|(key, _)| key.clone());
            let Some(oldest) = oldest else {
                return;
            };
            self.embeds.remove(&oldest);
        }
    }
}

/// Where a view's note is: its folder for images, and the vault's root.
fn image_dirs(root: &Path, path: &str) -> Vec<PathBuf> {
    let note = root.join(path);
    note.parent()
        .map(Path::to_path_buf)
        .into_iter()
        .chain(std::iter::once(root.to_path_buf()))
        .collect()
}

impl EditorView {
    /// This editor's note, vault-relative.
    fn embed_origin(
        &self,
        index: &VaultIndex,
        editor: gpui::EntityId,
        cx: &gpui::App,
    ) -> Option<String> {
        if let Some(origin) = &self.embeds.origin {
            return Some(origin.clone());
        }
        let path = crate::paste::paste_context(editor, cx).note_path?;
        crate::vault_index::vault_relative(index.root(), &path)
    }

    /// Makes the cards layout asked for, answering whether it made any,
    /// so the frame is laid out again with their heights.
    pub(crate) fn make_wanted_embeds(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.embeds.frame += 1;
        let wanted = std::mem::take(&mut self.embeds.wanted);
        let made = !wanted.is_empty();
        for key in wanted {
            let content = self.embed_content(&key, cx);
            let embed = Embed {
                content,
                body_height: px(0.),
                clipped: false,
                needs_measure: true,
                last_used: self.embeds.frame,
            };
            self.embeds.embeds.insert(key, embed);
        }
        let measured = self.measure_embeds(window, cx);
        self.embeds.trim();
        made || measured
    }

    /// What `key` shows, made from the vault index's copy of the note.
    fn embed_content(&mut self, key: &EmbedKey, cx: &mut Context<Self>) -> EmbedContent {
        if self.embeds.depth >= MAX_DEPTH {
            return EmbedContent::Message("Embeds nest too deep to show more here.".into());
        }
        let Some(index) = self.suggest.index.clone() else {
            return EmbedContent::Missing;
        };
        let found = {
            let index = index.read(cx);
            let origin = self.embed_origin(index, cx.entity_id(), cx);
            let link = NoteLink {
                note: key.target.clone(),
                heading: None,
            };
            resolve(index, origin.as_deref(), &link).map(|path| {
                let text = index.links().note(&path).map(|entry| entry.text.clone());
                (path, text, index.root().to_path_buf(), origin)
            })
        };
        let Some((path, text, root, origin)) = found else {
            return EmbedContent::Missing;
        };
        let shown: Shown = (path.clone(), key.subpath.clone());
        let mut chain = self.embeds.chain.clone();
        if chain.is_empty() {
            chain.push((origin.unwrap_or_default(), None));
        }
        if chain.contains(&shown) {
            let message = format!("“{}” embeds itself here, so it stops.", key.name());
            return EmbedContent::Message(message);
        }
        chain.push(shown);
        let text: Arc<str> = match text {
            Some(text) => text,
            None => Arc::from(std::fs::read_to_string(root.join(&path)).unwrap_or_default()),
        };
        let Some(excerpt) = excerpt(&text, key) else {
            return EmbedContent::Message(missing_part(key));
        };
        let view = self.embed_view(&excerpt, &root, &path, chain, &index, cx);
        let events = cx.subscribe(&view, |_, _, event: &EditorEvent, cx| {
            if let EditorEvent::OpenLink(target) = event {
                cx.emit(EditorEvent::OpenLink(target.clone()));
            }
        });
        EmbedContent::Note {
            view,
            source: text,
            path,
            _events: events,
        }
    }

    /// A read-only view of `excerpt` in this editor's look, one level
    /// deeper among embeds.
    fn embed_view(
        &self,
        excerpt: &str,
        root: &Path,
        path: &str,
        chain: Vec<Shown>,
        index: &Entity<VaultIndex>,
        cx: &mut Context<Self>,
    ) -> Entity<EditorView> {
        let mut base_theme = self.base_theme.clone();
        base_theme.text_padding = base_theme.space_lg;
        base_theme.background = gpui::transparent_black();
        let mut symbols = self.symbols.clone();
        symbols.mode = SymbolMode::AlwaysHidden;
        let depth = self.embeds.depth + 1;
        let zoom = self.zoom;
        let dirs = image_dirs(root, path);
        let index = index.clone();
        let origin = path.to_string();
        let source = Source::new(excerpt);
        cx.new(|cx| {
            let mut view = EditorView::with_source(source, dirs, &Config::defaults(), cx);
            view.read_only = true;
            view.embedded = true;
            view.readable_width = false;
            view.base_theme = base_theme;
            view.symbols = symbols;
            view.embeds.depth = depth;
            view.embeds.chain = chain;
            view.embeds.origin = Some(origin);
            view.set_vault_index(index, cx);
            view.set_zoom(zoom, cx);
            view
        })
    }

    /// Lays out the cards' views that haven't been at the card's width,
    /// so a card first shows at its real height. Answers whether any was.
    fn measure_embeds(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let width = self.column_width;
        let max = self.theme.body_line_height() * self.theme.embed.max_lines;
        let mut measured = false;
        for embed in self.embeds.embeds.values_mut() {
            let EmbedContent::Note { view, .. } = &embed.content else {
                continue;
            };
            let height = match embed.needs_measure {
                true => {
                    measured = true;
                    view.update(cx, |view, _| view.measure_as_embed(width, max, window))
                }
                false => view.read(cx).content_height(),
            };
            embed.needs_measure = false;
            if (height - embed.body_height).abs() > px(0.5) {
                measured = true;
            }
            embed.clipped = height > max;
            embed.body_height = height.min(max);
        }
        measured
    }

    /// Lays out this embedded view's lines at `width` until `max` is
    /// filled, and answers its whole height as far as it's known.
    fn measure_as_embed(&mut self, width: Pixels, max: Pixels, window: &Window) -> Pixels {
        let column = (width - self.theme.text_padding * 2.).max(px(1.));
        if column != self.column_width {
            self.column_width = column;
            self.remeasure();
        }
        let mut line = 0;
        while line < self.source.line_count() && self.content_height() < max {
            self.layout_doc_line(line, window);
            line += 1;
        }
        self.content_height()
    }

    /// The cards' views drawn over `frame`, each in the room its card
    /// keeps for it, clipped to the editor.
    pub(crate) fn embed_elements(
        &mut self,
        frame: &FrameLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if self.embeds.is_empty() {
            return Vec::new();
        }
        let mut placed = Vec::new();
        for line in &frame.lines {
            for row in &line.visual.rows {
                for piece in &row.pieces {
                    let PieceContent::Embed { key } = &piece.content else {
                        continue;
                    };
                    let Some(view) = self.embeds.view(key).cloned() else {
                        continue;
                    };
                    let origin =
                        gpui::point(frame.text_left + piece.x, line.top + row.top + piece.top);
                    let bounds = Bounds::new(origin, gpui::size(piece.width, piece.height));
                    placed.push((view, bounds));
                }
            }
        }
        let mask = ContentMask {
            bounds: frame.bounds,
        };
        placed
            .into_iter()
            .map(|(view, bounds)| {
                let mut element = div()
                    .w(bounds.size.width)
                    .h(bounds.size.height)
                    .overflow_hidden()
                    .child(view)
                    .into_any_element();
                window.with_content_mask(Some(mask.clone()), |window| {
                    element.prepaint_as_root(bounds.origin, bounds.size.into(), window, cx);
                });
                element
            })
            .collect()
    }

    /// Follows the notes the cards show after the vault index changed:
    /// a changed note's card shows its new text, and a card for a note
    /// that was missing is made again.
    pub(crate) fn refresh_embeds(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.suggest.index.clone() else {
            return;
        };
        let mut stale = Vec::new();
        let mut updates = Vec::new();
        {
            let index = index.read(cx);
            for (key, embed) in &self.embeds.embeds {
                match &embed.content {
                    EmbedContent::Note { source, path, .. } => {
                        let now = index.links().note(path).map(|entry| entry.text.clone());
                        match now {
                            Some(now) if Arc::ptr_eq(&now, source) || now == *source => {}
                            Some(now) => updates.push((key.clone(), now)),
                            None => stale.push(key.clone()),
                        }
                    }
                    _ => stale.push(key.clone()),
                }
            }
        }
        let changed = !stale.is_empty() || !updates.is_empty();
        for key in stale {
            self.embeds.embeds.remove(&key);
        }
        for (key, text) in updates {
            self.update_embed(&key, text, cx);
        }
        if changed {
            cx.notify();
        }
    }

    /// Shows `text`'s part for `key` in its card.
    fn update_embed(&mut self, key: &EmbedKey, text: Arc<str>, cx: &mut Context<Self>) {
        let Some(excerpt) = excerpt(&text, key) else {
            self.embeds.embeds.remove(key);
            return;
        };
        let Some(embed) = self.embeds.embeds.get_mut(key) else {
            return;
        };
        if let EmbedContent::Note { view, source, .. } = &mut embed.content {
            *source = text;
            view.update(cx, |view, cx| view.replace_all_text(&excerpt, cx));
            embed.needs_measure = true;
        }
    }
}

/// The part of `text` an embed of `key` shows.
fn excerpt(text: &str, key: &EmbedKey) -> Option<String> {
    let tree = parse(text);
    embedded_text(text, &tree, EmbedPart::of(key.subpath.as_deref()))
}

/// What a card says when the heading or block it names isn't there.
fn missing_part(key: &EmbedKey) -> String {
    let subpath = key.subpath.as_deref().unwrap_or_default();
    match subpath.strip_prefix('^') {
        Some(block) => format!("“{}” has no block ^{block}.", key.name()),
        None => format!("“{}” has no heading “{subpath}”.", key.name()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_names_its_note_and_part() {
        let key = EmbedKey::new("Folder/Lemma.md", Some("Proof"));
        assert_eq!(key.title(), "Lemma \u{203a} Proof");
        assert_eq!(key.link(), "Folder/Lemma.md#Proof");
        assert_eq!(key.name(), "Lemma");
        assert_eq!(EmbedKey::new("", Some("Proof")).title(), "Proof");
    }

    #[test]
    fn a_missing_part_is_named() {
        let block = EmbedKey::new("Lemma", Some("^claim"));
        assert_eq!(missing_part(&block), "“Lemma” has no block ^claim.");
    }
}
