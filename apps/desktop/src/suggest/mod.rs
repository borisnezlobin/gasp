//! Suggestions while typing: note names after `[[`, a note's headings
//! after `[[Note#`, and tags after `#`, from the workspace's
//! [`VaultIndex`], and emoji and symbols after `:` from [`emoji`]. They
//! show in one popover under the text, drawn by [`crate::ui::suggestions`].
//!
//! Up and Down move the highlight, Enter or Tab accept it, and Escape
//! hides the list until the cursor leaves what it was completing. The
//! editor keeps the keyboard throughout, so typing keeps filtering.

pub mod emoji;
pub mod trigger;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use gasp_core::document::Selection;
use gasp_core::pipeline::InputContext;
use gasp_core::transaction::{ChangeSet, Origin, Transaction};
use gpui::{
    AnyElement, AppContext, Context, Corner, Entity, IntoElement, ParentElement, Pixels,
    SharedString, Subscription, Window, anchored, point, px,
};

use self::trigger::{Trigger, TriggerKind, find_trigger};
use crate::editor::EditorView;
use crate::frame::FrameLayout;
use crate::outline::{Heading, headings, headings_in};
use crate::picker::fuzzy::{Candidate, Matcher, Query};
use crate::ui::suggestions::{list_height, scroll_to_show, scrolled};
use crate::ui::{ListHandlers, SuggestionRow, suggestion_list};
use crate::vault_index::VaultIndex;

/// The most suggestions a list holds; the popover scrolls through them.
const MAX_SUGGESTIONS: usize = 50;

/// The command an accepted suggestion is recorded as, for undo.
pub const ACCEPT_COMMAND: &str = "suggest.accept";

/// One suggestion and the text accepting it inserts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub row: SuggestionRow,
    pub insert: String,
}

/// The open list.
#[derive(Clone, Debug)]
pub struct OpenSuggestions {
    pub trigger: Trigger,
    pub items: Vec<Suggestion>,
    pub highlighted: usize,
    pub first_visible: usize,
    /// Wheel distance not yet worth a whole row.
    scroll_rest: Pixels,
}

impl OpenSuggestions {
    pub fn rows(&self) -> Vec<SuggestionRow> {
        self.items.iter().map(|item| item.row.clone()).collect()
    }
}

/// The editor's suggestion state.
#[derive(Default)]
pub struct SuggestState {
    pub(crate) index: Option<Entity<VaultIndex>>,
    pub(crate) open: Option<OpenSuggestions>,
    /// Where the query of a list closed with Escape starts. It stays
    /// closed while the cursor is still completing there.
    dismissed: Option<usize>,
    /// Headings of other notes, by vault-relative path, read in the
    /// background the first time they're asked for.
    headings: HashMap<String, Vec<Heading>>,
    loading: HashSet<String>,
    _observe_index: Option<Subscription>,
}

impl EditorView {
    /// Gives the editor the vault's notes and tags to suggest from.
    pub fn set_vault_index(&mut self, index: Entity<VaultIndex>, cx: &mut Context<Self>) {
        let observe = cx.observe(&index, |view, _, cx| {
            view.refresh_suggestions(cx);
            // An image that was missing may be in the vault now, or the
            // first scan may have just found it.
            view.retry_missing_images(cx);
        });
        self.suggest.index = Some(index);
        self.suggest._observe_index = Some(observe);
        self.suggest.headings.clear();
        self.retry_missing_images(cx);
    }

    /// The open suggestion list, if any.
    pub fn suggestions(&self) -> Option<&OpenSuggestions> {
        self.suggest.open.as_ref()
    }

    /// Works out what the cursor is completing and what to suggest. Runs
    /// after every edit and cursor move; it reads only the cursor's line
    /// until there is something to complete.
    pub(crate) fn refresh_suggestions(&mut self, cx: &mut Context<Self>) {
        let trigger = self.current_trigger();
        if self.suggest.dismissed.is_some()
            && self.suggest.dismissed != trigger.as_ref().map(Trigger::query_start)
        {
            self.suggest.dismissed = None;
        }
        let trigger = trigger.filter(|t| Some(t.query_start()) != self.suggest.dismissed);
        let items = match &trigger {
            Some(trigger) => self.suggestions_for(trigger, cx),
            None => Vec::new(),
        };
        let was_open = self.suggest.open.is_some();
        self.suggest.open = trigger.filter(|_| !items.is_empty()).map(|trigger| {
            let same_list = self.suggest.open.as_ref().is_some_and(|open| {
                open.trigger.query == trigger.query && open.trigger.kind == trigger.kind
            });
            let (highlighted, first_visible) = match (&self.suggest.open, same_list) {
                (Some(open), true) => (open.highlighted.min(items.len() - 1), open.first_visible),
                _ => (0, 0),
            };
            OpenSuggestions {
                trigger,
                items,
                highlighted,
                first_visible,
                scroll_rest: px(0.),
            }
        });
        if was_open || self.suggest.open.is_some() {
            cx.notify();
        }
    }

    fn current_trigger(&self) -> Option<Trigger> {
        let range = self.selected_range();
        if !range.is_empty() || self.marked.is_some() || self.read_only {
            return None;
        }
        let cursor = range.start;
        let line = self.source.line_of(cursor);
        let line_range = self.source.line_range(line);
        let text = self.source.text();
        let trigger = find_trigger(
            &text[line_range.start..cursor],
            &text[cursor..line_range.end],
            line_range.start,
        )?;
        // Notes and tags come from the vault; emoji need nothing but the
        // catalogue.
        if trigger.kind != TriggerKind::Emoji && self.suggest.index.is_none() {
            return None;
        }
        let context = self.source.tree().context_at(cursor);
        allowed_in(&trigger.kind, context).then_some(trigger)
    }

    fn suggestions_for(&mut self, trigger: &Trigger, cx: &mut Context<Self>) -> Vec<Suggestion> {
        if trigger.kind == TriggerKind::Emoji {
            return emoji_suggestions(&trigger.query);
        }
        let Some(index) = self.suggest.index.clone() else {
            return Vec::new();
        };
        match &trigger.kind {
            TriggerKind::Note => note_suggestions(index.read(cx), &trigger.query),
            TriggerKind::Tag => tag_suggestions(index.read(cx), &trigger.query),
            TriggerKind::Heading { note } => {
                let found = self.headings_of(note, &index, cx);
                heading_suggestions(&found.unwrap_or_default(), &trigger.query)
            }
            TriggerKind::Emoji => Vec::new(),
        }
    }

    /// The headings of the note a link names: this note's from its tree,
    /// another's from the cache, or `None` while it's being read.
    fn headings_of(
        &mut self,
        note: &str,
        index: &Entity<VaultIndex>,
        cx: &mut Context<Self>,
    ) -> Option<Vec<Heading>> {
        if note.trim().is_empty() {
            return Some(headings_in(self.source.tree(), self.source.text()));
        }
        let index = index.read(cx);
        let path = index.find_note(note)?.path.clone();
        if let Some(found) = self.suggest.headings.get(&path) {
            return Some(found.clone());
        }
        let file = index.root().join(&path);
        self.load_headings(path, file, cx);
        None
    }

    fn load_headings(&mut self, path: String, file: PathBuf, cx: &mut Context<Self>) {
        if !self.suggest.loading.insert(path.clone()) {
            return;
        }
        let read = cx.background_spawn(async move {
            let text = std::fs::read_to_string(file).unwrap_or_default();
            headings(&text)
        });
        cx.spawn(async move |view, cx| {
            let found = read.await;
            view.update(cx, |view, cx| {
                view.suggest.loading.remove(&path);
                view.suggest.headings.insert(path, found);
                view.refresh_suggestions(cx);
            })
            .ok();
        })
        .detach();
    }

    /// Moves the highlight by `step` rows, wrapping around.
    pub fn move_suggestion(&mut self, step: isize, cx: &mut Context<Self>) {
        let Some(open) = self.suggest.open.as_mut() else {
            return;
        };
        let count = open.items.len() as isize;
        open.highlighted = (open.highlighted as isize + step).rem_euclid(count) as usize;
        let rows = crate::ui::ui_theme(cx).suggestion_rows;
        open.first_visible = scroll_to_show(open.first_visible, open.highlighted, rows);
        cx.notify();
    }

    /// Scrolls the list by `distance` (positive towards its end), a
    /// whole row at a time. The highlight follows the pointer when it's
    /// on row `pointer` of those showing, and otherwise stays among the
    /// rows showing, so Enter accepts one you can see.
    pub fn scroll_suggestions(
        &mut self,
        distance: Pixels,
        pointer: Option<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let theme = crate::ui::ui_theme(cx);
        let Some(open) = self.suggest.open.as_mut() else {
            return;
        };
        open.scroll_rest += distance;
        let rows = (open.scroll_rest / theme.menu_row_height).trunc();
        open.scroll_rest -= theme.menu_row_height * rows;
        let visible = theme.suggestion_rows;
        let first = scrolled(open.first_visible, rows as isize, open.items.len(), visible);
        if first == open.first_visible {
            return;
        }
        open.first_visible = first;
        let last_shown = (first + visible).min(open.items.len()) - 1;
        open.highlighted = match pointer {
            Some(row) => (first + row).min(last_shown),
            None => open.highlighted.clamp(first, last_shown),
        };
        cx.notify();
    }

    pub(crate) fn hover_suggestion(
        &mut self,
        index: usize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(open) = self.suggest.open.as_mut()
            && open.highlighted != index
        {
            open.highlighted = index;
            cx.notify();
        }
    }

    pub(crate) fn choose_suggestion(
        &mut self,
        index: usize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(open) = self.suggest.open.as_mut() {
            open.highlighted = index;
        }
        self.accept_suggestion(cx);
    }

    /// Hides the list until the cursor leaves what it was completing.
    pub fn dismiss_suggestions(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(open) = self.suggest.open.take() else {
            return false;
        };
        self.suggest.dismissed = Some(open.trigger.query_start());
        cx.notify();
        true
    }

    /// Puts the highlighted suggestion in the text. Links get their
    /// closing `]]` and the cursor lands after it; tags get a space when
    /// they end the line. Returns false when no list is open.
    pub fn accept_suggestion(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(open) = self.suggest.open.take() else {
            return false;
        };
        let trigger = &open.trigger;
        let chosen = &open.items[open.highlighted];
        let mut text = chosen.insert.clone();
        let cursor = match (&trigger.kind, trigger.closed) {
            (TriggerKind::Emoji, _) => trigger.replace.start + text.len(),
            (TriggerKind::Tag, _) => {
                let next = self.doc().char_after(trigger.replace.end);
                if next.is_none_or(|next| next == '\n' || next == '\r') {
                    text.push(' ');
                }
                trigger.replace.start + text.len()
            }
            (_, false) => {
                text.push_str("]]");
                trigger.replace.start + text.len()
            }
            (_, true) => trigger.close_end + text.len() - trigger.replace.len(),
        };
        // What was just completed stays closed, rather than offering the
        // same tag again while the cursor is still at its end.
        self.suggest.dismissed = Some(trigger.query_start());
        let changes = ChangeSet::replace(trigger.replace.clone(), text);
        let transaction = Transaction::new(changes, Origin::command(ACCEPT_COMMAND), self.now_ms())
            .with_selection(Selection::cursor(cursor));
        self.apply_transaction(transaction, cx);
        true
    }
}

impl EditorView {
    /// The popover for the open list, placed under the start of the query
    /// with its labels lined up with the typed text, or above the line
    /// when there's no room below in the note. `None` when the query is off screen.
    pub(crate) fn suggestion_popover(
        &self,
        frame: &FrameLayout,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let open = self.suggest.open.as_ref()?;
        let caret = frame.caret_bounds(open.trigger.anchor(), &self.theme)?;
        let theme = crate::ui::ui_theme(cx);
        let height = list_height(open.items.len(), &theme);
        // Rows with a glyph column line their names up with the query.
        let glyph_column = if open.items.iter().any(|item| item.row.glyph.is_some()) {
            theme.suggestion_glyph_width + theme.space_lg
        } else {
            px(0.)
        };
        // Below the line when it fits inside the note, so it never covers
        // the status bar; above it otherwise.
        let fits_below = caret.bottom() + theme.suggestion_gap + height <= frame.bounds.bottom();
        let x = caret.left() - theme.menu_padding - theme.menu_row_padding_x - glyph_column;
        let (corner, y) = if fits_below {
            (Corner::TopLeft, caret.bottom() + theme.suggestion_gap)
        } else {
            (Corner::BottomLeft, caret.top() - theme.suggestion_gap)
        };
        let list = suggestion_list(
            &open.rows(),
            open.first_visible,
            open.highlighted,
            &theme,
            cx,
            ListHandlers {
                choose: Self::choose_suggestion,
                hover: Self::hover_suggestion,
                scroll: Self::scroll_suggestions,
            },
        );
        Some(
            anchored()
                .anchor(corner)
                .position(point(x, y))
                .snap_to_window()
                .child(list)
                .into_any_element(),
        )
    }
}

/// Links complete in text and in links being edited; tags and emoji only
/// in text, never in math, code, URLs or frontmatter.
fn allowed_in(kind: &TriggerKind, context: InputContext) -> bool {
    match kind {
        TriggerKind::Tag | TriggerKind::Emoji => {
            matches!(context, InputContext::Text | InputContext::Table)
        }
        _ => matches!(
            context,
            InputContext::Text | InputContext::Link | InputContext::Table
        ),
    }
}

fn note_suggestions(index: &VaultIndex, query: &str) -> Vec<Suggestion> {
    index
        .match_notes(query, MAX_SUGGESTIONS)
        .into_iter()
        .map(|hit| {
            let note = index.note(hit.note);
            let insert = if index.name_is_shared(hit.note) {
                note.path_without_extension()
            } else {
                note.name()
            };
            let folder = note.folder();
            Suggestion {
                row: SuggestionRow {
                    label: SharedString::from(note.name().to_owned()),
                    positions: hit.name_positions,
                    detail: (!folder.is_empty()).then(|| folder.to_owned().into()),
                    indent: 0,
                    glyph: None,
                },
                insert: insert.to_owned(),
            }
        })
        .collect()
}

/// Tags matching `query`, leaving out the one being typed: once the note
/// saves, the half-typed tag is in the index with a single use.
fn tag_suggestions(index: &VaultIndex, query: &str) -> Vec<Suggestion> {
    index
        .match_tags(query, MAX_SUGGESTIONS)
        .into_iter()
        .filter(|hit| hit.uses > 1 || hit.tag != query)
        .map(|hit| Suggestion {
            row: SuggestionRow {
                label: format!("#{}", hit.tag).into(),
                positions: hit.positions.iter().map(|p| p + 1).collect(),
                detail: None,
                indent: 0,
                glyph: None,
            },
            insert: hit.tag,
        })
        .collect()
}

/// Headings in document order while nothing is typed, else best first.
/// Rows indent by level below the note's top level.
fn heading_suggestions(found: &[Heading], query: &str) -> Vec<Suggestion> {
    let top = found.iter().map(|h| h.level).min().unwrap_or(1);
    let query = Query::new(query);
    let mut matcher = Matcher::new();
    let mut hits: Vec<(i32, usize, Vec<usize>)> = found
        .iter()
        .enumerate()
        .filter_map(|(at, heading)| {
            let hit = matcher.score(&query, &Candidate::new(&heading.title))?;
            Some((hit.score, at, hit.positions))
        })
        .collect();
    if !query.is_empty() {
        hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    }
    hits.into_iter()
        .take(MAX_SUGGESTIONS)
        .map(|(_, at, positions)| {
            let heading = &found[at];
            Suggestion {
                row: SuggestionRow {
                    label: heading.title.clone().into(),
                    positions,
                    detail: None,
                    indent: usize::from(heading.level - top).min(3),
                    glyph: None,
                },
                insert: heading.title.clone(),
            }
        })
        .collect()
}

/// Emoji and symbols by name. A glyph goes in the row's glyph column; an
/// emoticon is too wide for it and shows on the right instead.
fn emoji_suggestions(query: &str) -> Vec<Suggestion> {
    emoji::search(query, MAX_SUGGESTIONS)
        .into_iter()
        .map(|hit| {
            let entry = hit.entry;
            let wide = entry.kind == emoji::EntryKind::Emoticon;
            let glyph = SharedString::from(entry.glyph);
            Suggestion {
                row: SuggestionRow {
                    label: SharedString::from(entry.name),
                    positions: hit.positions,
                    detail: wide.then(|| glyph.clone()),
                    indent: 0,
                    glyph: (!wide).then_some(glyph),
                },
                insert: entry.glyph.to_owned(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault_index::NoteScan;

    fn index() -> VaultIndex {
        let mut index = VaultIndex::default();
        for path in ["Wave Packets.md", "a/Topic.md", "b/Topic.md"] {
            index.upsert(NoteScan {
                path: path.into(),
                tags: vec!["physics".into()],
            });
        }
        index
    }

    #[test]
    fn shared_names_link_by_path() {
        let found = note_suggestions(&index(), "topic");
        let inserts: Vec<&str> = found.iter().map(|s| s.insert.as_str()).collect();
        assert_eq!(inserts, ["a/Topic", "b/Topic"]);
        assert_eq!(found[0].row.detail.as_ref().map(|d| d.as_ref()), Some("a"));
        let wave = note_suggestions(&index(), "wave");
        assert_eq!(wave[0].insert, "Wave Packets");
        assert_eq!(wave[0].row.detail, None);
    }

    #[test]
    fn tags_show_their_hash() {
        let found = tag_suggestions(&index(), "phy");
        assert_eq!(found[0].row.label.as_ref(), "#physics");
        assert_eq!(found[0].row.positions, [1, 2, 3]);
        assert_eq!(found[0].insert, "physics");
    }

    #[test]
    fn headings_keep_document_order_until_filtered() {
        let found = headings("# Top\n## Group velocity\n## Phase\n### Deep\n");
        let all = heading_suggestions(&found, "");
        let labels: Vec<&str> = all.iter().map(|s| s.row.label.as_ref()).collect();
        assert_eq!(labels, ["Top", "Group velocity", "Phase", "Deep"]);
        assert_eq!(all[3].row.indent, 2);
        let phase = heading_suggestions(&found, "pha");
        assert_eq!(phase[0].insert, "Phase");
    }
}
