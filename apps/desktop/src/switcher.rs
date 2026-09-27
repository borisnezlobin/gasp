//! The quick switcher (`switcher.open`, Mod+O): find a note by name or
//! path, or create one.
//!
//! File names rank above folder paths, and recently opened notes get a
//! boost. When nothing matches, the only row offers to create a note named
//! after the query.

use std::collections::HashSet;

use gpui::{
    AnyElement, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    ParentElement, Render, SharedString, Subscription, Window, div, prelude::*,
};

use crate::icons::{IconName, icon};
use crate::picker::fuzzy::{Candidate, Matcher, Query};
use crate::picker::{Confirmed, Picker, PickerDelegate, highlighted_text};
use crate::theme::PickerTheme;

/// Extra score for a match inside the file name rather than the folders.
const NAME_MATCH_BONUS: i32 = 50;
/// Extra score for the most recently opened note, falling by
/// [`RECENT_STEP`] per place in the recent list.
const RECENT_BOOST: i32 = 30;
const RECENT_STEP: i32 = 2;
const NOTE_EXTENSION: &str = ".md";

/// What the switcher asks its owner to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwitcherEvent {
    /// Open the note at this vault-relative path.
    Open { path: String, new_tab: bool },
    /// Create a note with this name (the query as typed, trimmed).
    Create(String),
}

/// A note the switcher can open.
#[derive(Clone, Debug)]
pub struct NoteEntry {
    /// Vault-relative, with `/` separators.
    pub path: String,
    /// The byte offset where the file name starts.
    name_start: usize,
    /// The char index where the file name starts.
    name_char_start: usize,
    recent_rank: Option<usize>,
    candidate: Candidate,
}

impl NoteEntry {
    fn new(path: String, recent_rank: Option<usize>) -> NoteEntry {
        let name_start = path.rfind('/').map_or(0, |slash| slash + 1);
        let candidate = Candidate::new(&path);
        NoteEntry {
            name_char_start: candidate.char_index_of_byte(name_start),
            name_start,
            path,
            recent_rank,
            candidate,
        }
    }

    /// The name shown: the file name without `.md`.
    pub fn name(&self) -> &str {
        let file = &self.path[self.name_start..];
        file.strip_suffix(NOTE_EXTENSION).unwrap_or(file)
    }

    /// The folder path, empty at the vault root.
    pub fn folder(&self) -> &str {
        self.path[..self.name_start].trim_end_matches('/')
    }
}

#[derive(Clone, Debug)]
struct NoteMatch {
    entry: usize,
    score: i32,
    /// Matched byte offsets in the path.
    positions: Vec<usize>,
}

/// A row of the switcher.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwitcherRow {
    Note(String),
    Create(String),
}

/// The switcher's items and matching.
pub struct SwitcherDelegate {
    entries: Vec<NoteEntry>,
    matches: Vec<NoteMatch>,
    create: Option<String>,
    matcher: Matcher,
}

impl SwitcherDelegate {
    /// Every path in `paths` and `recent` (most recent first), recent ones
    /// first and the rest by path.
    pub fn new(paths: Vec<String>, recent: Vec<String>) -> Self {
        let recent: Vec<String> = recent.into_iter().map(normalize).collect();
        let mut seen = HashSet::new();
        let mut entries: Vec<NoteEntry> = recent
            .iter()
            .cloned()
            .chain(paths.into_iter().map(normalize))
            .filter(|path| !path.is_empty() && seen.insert(path.clone()))
            .map(|path| {
                let rank = recent.iter().position(|recent| *recent == path);
                NoteEntry::new(path, rank)
            })
            .collect();
        entries.sort_by(|a, b| {
            let rank = |entry: &NoteEntry| entry.recent_rank.unwrap_or(usize::MAX);
            rank(a).cmp(&rank(b)).then_with(|| a.path.cmp(&b.path))
        });
        Self {
            entries,
            matches: Vec::new(),
            create: None,
            matcher: Matcher::new(),
        }
    }

    pub fn entries(&self) -> &[NoteEntry] {
        &self.entries
    }

    /// What row `index` holds.
    pub fn row(&self, index: usize) -> Option<SwitcherRow> {
        if let Some(found) = self.matches.get(index) {
            return Some(SwitcherRow::Note(self.entries[found.entry].path.clone()));
        }
        let create = self
            .create
            .as_ref()
            .filter(|_| index == self.matches.len())?;
        Some(SwitcherRow::Create(create.clone()))
    }

    fn match_entry(&mut self, query: &Query, index: usize) -> Option<NoteMatch> {
        let entry = &self.entries[index];
        let recency = entry
            .recent_rank
            .map_or(0, |rank| (RECENT_BOOST - RECENT_STEP * rank as i32).max(0));
        let by_name = self
            .matcher
            .score_from(query, &entry.candidate, entry.name_char_start)
            .map(|found| (found.score + NAME_MATCH_BONUS, found.positions));
        let (score, positions) = match by_name {
            Some(found) => found,
            None => {
                let found = self.matcher.score(query, &entry.candidate)?;
                (found.score, found.positions)
            }
        };
        Some(NoteMatch {
            entry: index,
            score: score + recency,
            positions,
        })
    }

    fn render_note(&self, found: &NoteMatch, theme: &PickerTheme) -> AnyElement {
        let entry = &self.entries[found.entry];
        let (folder_positions, name_positions): (Vec<usize>, Vec<usize>) = found
            .positions
            .iter()
            .partition(|position| **position < entry.name_start);
        let name_positions: Vec<usize> = name_positions
            .iter()
            .map(|position| position - entry.name_start)
            .collect();
        let name = highlighted_text(entry.name().to_string(), &name_positions, theme);
        let folder = highlighted_text(entry.folder().to_string(), &folder_positions, theme);
        row_layout(IconName::FileText, theme)
            .child(div().flex_none().child(name))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .text_size(theme.detail_font_size)
                    .text_color(theme.detail_text)
                    .child(folder.grow()),
            )
            .into_any_element()
    }
}

fn normalize(path: String) -> String {
    let path = path.replace('\\', "/");
    path.trim_start_matches("./").trim_matches('/').to_string()
}

fn row_layout(icon_name: IconName, theme: &PickerTheme) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(theme.row_gap)
        .w_full()
        .text_size(theme.row_font_size)
        .child(
            icon(icon_name)
                .flex_none()
                .size(theme.icon_size)
                .text_color(theme.icon),
        )
}

impl PickerDelegate for SwitcherDelegate {
    type Event = SwitcherEvent;

    fn placeholder(&self) -> SharedString {
        "Find or create a note".into()
    }

    fn match_count(&self) -> usize {
        self.matches.len() + usize::from(self.create.is_some())
    }

    fn update_matches(&mut self, query: &str) {
        let _span = crate::trace::span("switcher-filter");
        let parsed = Query::new(query);
        let mut matches: Vec<NoteMatch> = (0..self.entries.len())
            .filter_map(|index| self.match_entry(&parsed, index))
            .collect();
        if !parsed.is_empty() {
            let entries = &self.entries;
            matches.sort_by(|a, b| {
                b.score
                    .cmp(&a.score)
                    .then_with(|| {
                        entries[a.entry]
                            .path
                            .len()
                            .cmp(&entries[b.entry].path.len())
                    })
                    .then(a.entry.cmp(&b.entry))
            });
        }
        let name = query.trim();
        self.create = (matches.is_empty() && !name.is_empty()).then(|| name.to_string());
        self.matches = matches;
    }

    fn render_match(&self, index: usize, _selected: bool, theme: &PickerTheme) -> AnyElement {
        if let Some(found) = self.matches.get(index) {
            return self.render_note(found, theme);
        }
        let name = self.create.clone().unwrap_or_default();
        row_layout(IconName::FilePlus, theme)
            .child(div().flex_1().child(format!("Create “{name}”")))
            .into_any_element()
    }

    fn confirm(&mut self, index: usize) -> Option<SwitcherEvent> {
        Some(match self.row(index)? {
            SwitcherRow::Note(path) => SwitcherEvent::Open {
                path,
                new_tab: false,
            },
            SwitcherRow::Create(name) => SwitcherEvent::Create(name),
        })
    }

    fn secondary_confirm(&mut self, index: usize) -> Option<SwitcherEvent> {
        Some(match self.row(index)? {
            SwitcherRow::Note(path) => SwitcherEvent::Open {
                path,
                new_tab: true,
            },
            SwitcherRow::Create(name) => SwitcherEvent::Create(name),
        })
    }

    fn empty_message(&self, _query: &str) -> SharedString {
        "Your vault has no notes yet. Type a name to create one.".into()
    }
}

/// The quick switcher. Emits [`SwitcherEvent`], then [`DismissEvent`].
pub struct QuickSwitcher {
    picker: Entity<Picker<SwitcherDelegate>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SwitcherEvent> for QuickSwitcher {}
impl EventEmitter<DismissEvent> for QuickSwitcher {}

impl Focusable for QuickSwitcher {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl QuickSwitcher {
    /// A switcher over the vault-relative note `paths`, with the recently
    /// opened ones in `recent` (most recent first).
    pub fn new(
        paths: Vec<String>,
        recent: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let delegate = SwitcherDelegate::new(paths, recent);
        let picker = cx.new(|cx| Picker::new(delegate, window, cx));
        let subscriptions = vec![
            cx.subscribe(&picker, |_, _, event: &Confirmed<SwitcherEvent>, cx| {
                cx.emit(event.0.clone());
                cx.emit(DismissEvent);
            }),
            cx.subscribe(&picker, |_, _, _: &DismissEvent, cx| cx.emit(DismissEvent)),
        ];
        Self {
            picker,
            _subscriptions: subscriptions,
        }
    }

    pub fn picker(&self) -> &Entity<Picker<SwitcherDelegate>> {
        &self.picker
    }
}

impl Render for QuickSwitcher {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.picker.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(|path| path.to_string()).collect()
    }

    fn rows(delegate: &SwitcherDelegate) -> Vec<SwitcherRow> {
        (0..delegate.match_count())
            .filter_map(|index| delegate.row(index))
            .collect()
    }

    fn note(path: &str) -> SwitcherRow {
        SwitcherRow::Note(path.to_string())
    }

    #[test]
    fn file_names_rank_above_folders() {
        let mut switcher = SwitcherDelegate::new(
            paths(&["travel/notes/packing.md", "notes.md", "archive/travel.md"]),
            Vec::new(),
        );
        switcher.update_matches("notes");
        assert_eq!(rows(&switcher)[0], note("notes.md"));
        switcher.update_matches("travel");
        assert_eq!(rows(&switcher)[0], note("archive/travel.md"));
    }

    #[test]
    fn recent_notes_come_first_and_get_a_boost() {
        let mut switcher = SwitcherDelegate::new(
            paths(&[
                "alpha.md",
                "beta.md",
                "gamma.md",
                "project plan.md",
                "plan.md",
            ]),
            paths(&["gamma.md", "project plan.md"]),
        );
        switcher.update_matches("");
        assert_eq!(
            rows(&switcher)[..3],
            [note("gamma.md"), note("project plan.md"), note("alpha.md")]
        );
        switcher.update_matches("plan");
        assert_eq!(rows(&switcher)[0], note("project plan.md"));
    }

    #[test]
    fn nothing_matching_offers_to_create() {
        let mut switcher = SwitcherDelegate::new(paths(&["alpha.md"]), Vec::new());
        switcher.update_matches("  zebra crossing ");
        assert_eq!(
            rows(&switcher),
            [SwitcherRow::Create("zebra crossing".into())]
        );
        assert_eq!(
            switcher.confirm(0),
            Some(SwitcherEvent::Create("zebra crossing".into()))
        );
    }

    #[test]
    fn secondary_confirm_opens_in_a_new_tab() {
        let mut switcher = SwitcherDelegate::new(paths(&["a/b.md"]), Vec::new());
        switcher.update_matches("b");
        let expected = SwitcherEvent::Open {
            path: "a/b.md".into(),
            new_tab: true,
        };
        assert_eq!(switcher.secondary_confirm(0), Some(expected));
    }

    #[test]
    fn paths_are_normalised_and_deduplicated() {
        let switcher = SwitcherDelegate::new(
            paths(&["daily\\2024-01-01.md", "a.md"]),
            paths(&["a.md", "daily/2024-01-01.md"]),
        );
        assert_eq!(switcher.entries().len(), 2);
        assert_eq!(switcher.entries()[1].folder(), "daily");
        assert_eq!(switcher.entries()[1].name(), "2024-01-01");
    }

    #[test]
    fn spaces_match_folder_separators() {
        let mut switcher = SwitcherDelegate::new(
            paths(&["daily/notes.md", "dairy notes.md", "other.md"]),
            Vec::new(),
        );
        switcher.update_matches("daily note");
        assert_eq!(rows(&switcher)[0], note("daily/notes.md"));
    }

    fn synthetic_vault(count: usize) -> Vec<String> {
        const FOLDERS: [&str; 6] = [
            "daily",
            "projects/editor",
            "reading",
            "people",
            "archive/2023",
            "ideas",
        ];
        const WORDS: [&str; 8] = [
            "plan", "review", "meeting", "draft", "sketch", "summary", "Idea", "log",
        ];
        (0..count)
            .map(|index| {
                let folder = FOLDERS[index % FOLDERS.len()];
                let word = WORDS[(index / 7) % WORDS.len()];
                format!("{folder}/{word} {index:05}.md")
            })
            .collect()
    }

    #[test]
    fn filters_ten_thousand_paths_quickly() {
        let mut switcher = SwitcherDelegate::new(synthetic_vault(10_000), Vec::new());
        let queries = ["p", "pl", "pla", "plan", "plan 0042", "prj ed sk", "zzz"];
        let started = Instant::now();
        for query in queries {
            switcher.update_matches(query);
        }
        let per_query = started.elapsed() / queries.len() as u32;
        switcher.update_matches("sketch 00420");
        assert_eq!(rows(&switcher)[0], note("daily/sketch 00420.md"));
        // Unoptimised test builds are several times slower than release.
        assert!(per_query.as_millis() < 150, "{per_query:?} per query");
    }
}
