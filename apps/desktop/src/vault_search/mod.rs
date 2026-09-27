//! The vault search panel (`search.open`): a query field, results grouped
//! by note with the matching lines, and replace across notes.
//!
//! Wiring: call [`bind_keys`] once at startup (the fields' editing keys
//! come from `keymap::bind_rules`). The workspace creates a
//! [`VaultSearch`] with the vault root, focuses it on `search.open`, opens
//! the note on [`VaultSearchEvent::Open`], reloads open notes listed in
//! [`VaultSearchEvent::Replaced`], and returns focus to the editor on
//! [`VaultSearchEvent::Dismissed`].

pub mod engine;
pub mod tags;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui::{
    AnyElement, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    HighlightStyle, KeyBinding, ListAlignment, ListState, Pixels, Subscription, Task, Window,
    actions, div, list, prelude::*, px,
};

use crate::icons::{IconName, icon};
use crate::note_texts::NoteTexts;
use crate::text_input::{TextInput, TextInputEvent, TextInputStyle};
use crate::theme::{PickerTheme, UiTheme};
use crate::ui::{Button, truncated, ui_theme};
use crate::vault_index::VaultIndex;
use engine::{Note, NoteResult, ReplaceReport};

/// The key context the panel sets.
pub const SEARCH_CONTEXT: &str = "VaultSearch";
/// The key context around the replace field.
pub const SEARCH_REPLACE_CONTEXT: &str = "VaultSearchReplace";

actions!(
    vault_search,
    [
        SelectNextResult,
        SelectPreviousResult,
        Confirm,
        Dismiss,
        FocusNextField,
        RequestReplaceAll,
    ]
);

/// Keys inside the panel: arrows move, Enter opens (or confirms a
/// replace), Escape closes (or cancels one).
pub fn bind_keys(cx: &mut App) {
    let panel = Some(SEARCH_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("down", SelectNextResult, panel),
        KeyBinding::new("up", SelectPreviousResult, panel),
        KeyBinding::new("enter", Confirm, panel),
        KeyBinding::new("escape", Dismiss, panel),
        KeyBinding::new("tab", FocusNextField, panel),
        KeyBinding::new("shift-tab", FocusNextField, panel),
        KeyBinding::new("enter", RequestReplaceAll, Some(SEARCH_REPLACE_CONTEXT)),
    ]);
}

/// What the panel tells the workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VaultSearchEvent {
    /// Open the note at `path` (absolute) with the cursor at byte `offset`.
    Open { path: PathBuf, offset: usize },
    /// These notes (absolute paths) were rewritten on disk.
    Replaced { paths: Vec<PathBuf> },
    /// Escape: hide the panel and focus the editor.
    Dismissed,
}

/// One line of the result list: a note, or one of its matching lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub note: usize,
    pub hit: Option<usize>,
}

/// A replace waiting for confirmation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingReplace {
    pub query: String,
    pub replacement: String,
    pub paths: Vec<PathBuf>,
    pub matches: usize,
}

/// The search panel over one vault.
pub struct VaultSearch {
    root: PathBuf,
    /// The vault's index, which knows every note's tags, frontmatter
    /// included, for `tag:` searches.
    index: Option<Entity<VaultIndex>>,
    /// The workspace's note texts, kept between searches, when there is
    /// one.
    texts: Option<NoteTexts>,
    notes: Arc<Vec<Note>>,
    query: Entity<TextInput>,
    replacement: Entity<TextInput>,
    results: Vec<NoteResult>,
    rows: Vec<Row>,
    selected: usize,
    generation: Arc<AtomicUsize>,
    search_task: Option<Task<()>>,
    load_task: Option<Task<()>>,
    replace_task: Option<Task<()>>,
    pending_replace: Option<PendingReplace>,
    status: Option<String>,
    /// Note rows are taller than the hit rows under them, so the list
    /// measures each row, and still lays out only the ones on screen.
    list: ListState,
    focus_handle: FocusHandle,
    ui: UiTheme,
    /// The picker's tokens, so the panel looks like the quick switcher.
    surface: PickerTheme,
    show_replace: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<VaultSearchEvent> for VaultSearch {}

/// Lets the workspace host the panel as a modal.
impl EventEmitter<gpui::DismissEvent> for VaultSearch {}

impl Focusable for VaultSearch {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.query.focus_handle(cx)
    }
}

/// "Replace 12 matches in 4 notes?"
pub fn replace_prompt(pending: &PendingReplace) -> String {
    format!(
        "Replace {} in {}?",
        plural(pending.matches, "match", "matches"),
        plural(pending.paths.len(), "note", "notes")
    )
}

/// "Replaced 12 matches in 4 notes", plus any failures.
pub fn replace_summary(report: &ReplaceReport) -> String {
    let mut summary = format!(
        "Replaced {} in {}",
        plural(report.matches, "match", "matches"),
        plural(report.changed.len(), "note", "notes")
    );
    if !report.failed.is_empty() {
        summary.push_str(&format!(
            ", {} couldn't be changed",
            plural(report.failed.len(), "note", "notes")
        ));
    }
    summary
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

impl VaultSearch {
    /// A panel over the workspace's note texts. It searches the notes as
    /// last read at once, and again once the ones that changed are read.
    pub fn with_texts(texts: NoteTexts, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut panel = Self::build(texts.root().to_path_buf(), window, cx);
        panel.notes = texts.snapshot().unwrap_or_default();
        panel.texts = Some(texts);
        panel.refresh(cx);
        panel
    }

    /// Searches `tag:name` with the vault's index, which knows tags that
    /// are only in a note's frontmatter.
    pub fn with_index(mut self, index: Entity<VaultIndex>) -> Self {
        self.index = Some(index);
        self
    }

    /// A panel over the notes under `root`, which it starts loading.
    pub fn new(root: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut panel = Self::build(root, window, cx);
        panel.refresh(cx);
        panel
    }

    fn build(root: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let ui = ui_theme(cx);
        let query = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_placeholder("Search all notes")
                .with_style(TextInputStyle::Query)
                .bubble_enter_and_escape()
        });
        let replacement = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_placeholder("Replace with")
                .bubble_enter_and_escape()
        });
        let subscriptions = vec![cx.subscribe(&query, |this, _, event: &TextInputEvent, cx| {
            if *event == TextInputEvent::Changed {
                this.query_changed(cx);
            }
        })];
        window.focus(&query.focus_handle(cx));
        Self {
            root,
            index: None,
            texts: None,
            notes: Arc::default(),
            query,
            replacement,
            results: Vec::new(),
            rows: Vec::new(),
            selected: 0,
            generation: Arc::default(),
            search_task: None,
            load_task: None,
            replace_task: None,
            pending_replace: None,
            status: None,
            list: ListState::new(0, ListAlignment::Top, px(0.)),
            focus_handle: cx.focus_handle(),
            surface: PickerTheme::from_ui(&ui),
            ui,
            show_replace: false,
            _subscriptions: subscriptions,
        }
    }

    /// Reloads the notes from disk and searches again. Call it when the
    /// panel is shown.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let root = self.root.clone();
        let texts = self.texts.clone();
        let loading = cx.background_spawn(async move {
            let _span = crate::trace::span("search-load");
            match texts {
                Some(texts) => texts.load(),
                None => Arc::new(engine::load_vault(&root)),
            }
        });
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let notes = loading.await;
            this.update(cx, |panel, cx| {
                panel.notes = notes;
                panel.load_task = None;
                panel.start_search(cx);
            })
            .ok();
        }));
    }

    /// Focuses the query and selects it, as `search.open` does when the
    /// panel is already open.
    pub fn focus_query(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.query.update(cx, |query, cx| query.select_all(cx));
        window.focus(&self.query.focus_handle(cx));
    }

    pub fn set_query(&mut self, text: &str, cx: &mut Context<Self>) {
        self.query.update(cx, |query, cx| query.set_text(text, cx));
        self.query_changed(cx);
    }

    fn query_changed(&mut self, cx: &mut Context<Self>) {
        self.pending_replace = None;
        self.start_search(cx);
    }

    pub fn set_replacement(&mut self, text: &str, cx: &mut Context<Self>) {
        self.replacement
            .update(cx, |input, cx| input.set_text(text, cx));
    }

    pub fn results(&self) -> &[NoteResult] {
        &self.results
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn pending_replace(&self) -> Option<&PendingReplace> {
        self.pending_replace.as_ref()
    }

    /// The last replace's summary, such as "Replaced 3 matches in 2 notes".
    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    /// Searches on a background thread. A newer search bumps the
    /// generation, which stops the older one, and drops its task.
    fn start_search(&mut self, cx: &mut Context<Self>) {
        let current = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        let generation = self.generation.clone();
        let notes = self.notes.clone();
        let query = self.query.read(cx).text().to_owned();
        let tagged = tags::tag_query(&query)
            .zip(self.index.as_ref())
            .map(|(tag, index)| (tag.to_owned(), index.read(cx).links().notes_tagged(tag)));
        let searching = cx.background_spawn(async move {
            let _span = crate::trace::span("search-query");
            match tagged {
                Some((tag, notes_tagged)) => {
                    tags::search_tagged(&notes, &tag, &notes_tagged, &generation, current)
                }
                None => engine::search(&notes, &query, &generation, current),
            }
        });
        self.search_task = Some(cx.spawn(async move |this, cx| {
            let results = searching.await;
            this.update(cx, |panel, cx| {
                if panel.generation.load(Ordering::Relaxed) == current {
                    panel.show_results(results, cx);
                }
            })
            .ok();
        }));
    }

    fn show_results(&mut self, results: Vec<NoteResult>, cx: &mut Context<Self>) {
        self.rows = rows_for(&results);
        self.results = results;
        self.selected = 0;
        self.list.reset(self.rows.len());
        cx.notify();
    }

    pub fn select_next(&mut self, cx: &mut Context<Self>) {
        if !self.rows.is_empty() {
            self.select_row((self.selected + 1) % self.rows.len(), cx);
        }
    }

    pub fn select_previous(&mut self, cx: &mut Context<Self>) {
        if !self.rows.is_empty() {
            let count = self.rows.len();
            self.select_row((self.selected + count - 1) % count, cx);
        }
    }

    fn select_row(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected = index;
        self.list.scroll_to_reveal_item(index);
        cx.notify();
    }

    /// Opens the selected row's note at its match.
    pub fn open_selected(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(self.selected).copied() else {
            return;
        };
        let note = &self.results[row.note];
        let hit = row.hit.or((!note.hits.is_empty()).then_some(0));
        let offset = hit.map_or(0, |hit| note.hits[hit].offset);
        cx.emit(VaultSearchEvent::Open {
            path: self.root.join(&note.path),
            offset,
        });
    }

    /// Asks to confirm replacing every match across the current results.
    pub fn request_replace_all(&mut self, cx: &mut Context<Self>) {
        let query = self.query.read(cx).text().trim().to_owned();
        let matches: usize = self.results.iter().map(|result| result.match_count).sum();
        if query.is_empty() || matches == 0 {
            return;
        }
        let paths = self
            .results
            .iter()
            .filter(|result| result.match_count > 0)
            .map(|result| result.path.clone())
            .collect();
        self.pending_replace = Some(PendingReplace {
            query,
            replacement: self.replacement.read(cx).text().to_owned(),
            paths,
            matches,
        });
        self.status = None;
        cx.notify();
    }

    pub fn cancel_replace(&mut self, cx: &mut Context<Self>) {
        self.pending_replace = None;
        cx.notify();
    }

    /// Runs the confirmed replace on a background thread, then reports and
    /// searches again.
    pub fn confirm_replace(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.pending_replace.take() else {
            return;
        };
        let root = self.root.clone();
        let replacing = cx.background_spawn(async move {
            engine::replace_in_vault(&root, &pending.paths, &pending.query, &pending.replacement)
        });
        self.replace_task = Some(cx.spawn(async move |this, cx| {
            let report = replacing.await;
            this.update(cx, |panel, cx| panel.finish_replace(report, cx))
                .ok();
        }));
        cx.notify();
    }

    fn finish_replace(&mut self, report: ReplaceReport, cx: &mut Context<Self>) {
        self.status = Some(replace_summary(&report));
        let paths = report
            .changed
            .iter()
            .map(|path| self.root.join(path))
            .collect();
        cx.emit(VaultSearchEvent::Replaced { paths });
        if let Some(texts) = &self.texts {
            texts.mark_changed(&report.changed);
        }
        self.refresh(cx);
        cx.notify();
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        if self.pending_replace.is_some() {
            self.confirm_replace(cx);
        } else {
            self.open_selected(cx);
        }
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        if self.pending_replace.is_some() {
            self.cancel_replace(cx);
        } else {
            cx.emit(VaultSearchEvent::Dismissed);
            cx.emit(gpui::DismissEvent);
        }
    }

    /// Shows the replace field and focuses it, or hides it and goes back
    /// to the query.
    fn toggle_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_replace = !self.show_replace;
        let target = if self.show_replace {
            &self.replacement
        } else {
            &self.query
        };
        window.focus(&target.focus_handle(cx));
        cx.notify();
    }

    fn focus_other_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_replace = true;
        cx.notify();
        let target = if self.query.focus_handle(cx).is_focused(window) {
            &self.replacement
        } else {
            &self.query
        };
        window.focus(&target.focus_handle(cx));
    }
}

/// Every note followed by its matching lines, leaving out a line that
/// only repeats the note's name (its title heading), which the note's
/// row already shows.
pub fn rows_for(results: &[NoteResult]) -> Vec<Row> {
    results
        .iter()
        .enumerate()
        .flat_map(|(note, result)| {
            let name = result
                .path
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let hits = result
                .hits
                .iter()
                .enumerate()
                .filter(move |(_, hit)| hit.excerpt.trim().to_lowercase() != name)
                .map(move |(hit, _)| Row {
                    note,
                    hit: Some(hit),
                });
            std::iter::once(Row { note, hit: None }).chain(hits)
        })
        .collect()
}

/// Result rows shown before the list scrolls.
const VISIBLE_ROWS: usize = 12;

impl VaultSearch {
    /// The query, bare and large as in the quick switcher, with the button
    /// that shows the replace field.
    fn query_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let surface = &self.surface;
        let label = if self.show_replace {
            "Hide replace"
        } else {
            "Replace"
        };
        div()
            .flex()
            .items_center()
            .gap(self.ui.space_md)
            .pl(surface.input_padding_x)
            .pr(surface.input_padding_y)
            .py(surface.input_padding_y)
            .child(div().flex_1().min_w_0().child(self.query.clone()))
            .child(
                Button::new("toggle-replace", label)
                    .quiet()
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_replace(window, cx))),
            )
    }

    fn replace_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context(SEARCH_REPLACE_CONTEXT)
            .on_action(cx.listener(|this, _: &RequestReplaceAll, _, cx| {
                if this.pending_replace.is_some() {
                    this.confirm_replace(cx);
                } else {
                    this.request_replace_all(cx);
                }
            }))
            .flex()
            .items_center()
            .gap(self.ui.space_md)
            .pl(self.surface.input_padding_x)
            .pr(self.surface.input_padding_y)
            .pb(self.surface.input_padding_y)
            .child(div().flex_1().min_w_0().child(self.replacement.clone()))
            .child(
                Button::new("replace-all", "Replace all")
                    .on_click(cx.listener(|this, _, _, cx| this.request_replace_all(cx))),
            )
    }

    fn confirm_row(&self, pending: &PendingReplace, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(self.ui.space_md)
            .pl(self.surface.input_padding_x)
            .pr(self.surface.input_padding_y)
            .pb(self.surface.input_padding_y)
            .child(div().flex_1().child(replace_prompt(pending)))
            .child(
                Button::new("replace-cancel", "Cancel")
                    .on_click(cx.listener(|this, _, _, cx| this.cancel_replace(cx))),
            )
            .child(
                Button::new("replace-confirm", "Replace")
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| this.confirm_replace(cx))),
            )
    }

    /// A note's row: its name, the folder it's in, and how many matches.
    fn note_row(&self, note: &NoteResult) -> gpui::Div {
        let ui = &self.ui;
        let name = note
            .path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let folder = note
            .path
            .parent()
            .map(|folder| folder.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        div()
            .flex()
            .items_center()
            .gap(self.surface.row_gap)
            .h(ui.row_height)
            .child(
                icon(IconName::FileText)
                    .flex_none()
                    .size(self.surface.icon_size)
                    .text_color(self.surface.icon),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .max_w(gpui::relative(0.7))
                    .child(truncated(name)),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .text_size(ui.small_font_size)
                    .text_color(self.surface.detail_text)
                    .child(truncated(folder).grow()),
            )
            // A note found only by its name has no count to show.
            .when(note.match_count > 0, |row| {
                row.child(
                    div()
                        .flex_none()
                        .text_size(ui.small_font_size)
                        .text_color(self.surface.detail_text)
                        .child(note.match_count.to_string()),
                )
            })
    }

    /// One matching line, indented under its note's name, with the matches
    /// marked. It's shorter than a note's row, so a note and its lines read
    /// as one group.
    fn hit_row(&self, hit: &engine::LineHit) -> gpui::Div {
        let ui = &self.ui;
        let highlight = HighlightStyle {
            background_color: Some(ui.match_background),
            color: Some(ui.text),
            ..HighlightStyle::default()
        };
        let excerpt = truncated(hit.excerpt.clone())
            .with_highlights(hit.ranges.iter().map(|range| (range.clone(), highlight)))
            .grow();
        div()
            .flex()
            .items_center()
            .h(ui.compact_row_height)
            .pl(self.surface.icon_size + self.surface.row_gap)
            .text_size(ui.small_font_size)
            .text_color(ui.text_muted)
            .child(excerpt)
    }

    fn render_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let surface = &self.surface;
        let Some(row) = self.rows.get(index).copied() else {
            return div().into_any_element();
        };
        let note = &self.results[row.note];
        let content = match row.hit {
            None => self.note_row(note),
            Some(hit) => self.hit_row(&note.hits[hit]),
        };
        let selected = index == self.selected;
        div()
            .id(("search-row", index))
            .w_full()
            .px(surface.row_padding_x)
            .rounded(surface.row_corner_radius)
            .when(selected, |row| row.bg(surface.selected_row))
            .when(!selected, |row| {
                row.hover(|style| style.bg(surface.hovered_row))
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected = index;
                this.open_selected(cx);
            }))
            .child(content.w_full())
            .into_any_element()
    }

    /// The list's height: every row while they fit, then as many as
    /// [`VISIBLE_ROWS`] note rows would take, and the rest scrolls.
    fn list_height(&self) -> Pixels {
        let notes = self.results.len() as f32;
        let hits = (self.rows.len() - self.results.len()) as f32;
        let all = self.ui.row_height * notes + self.ui.compact_row_height * hits;
        all.min(self.ui.row_height * VISIBLE_ROWS as f32)
    }

    /// Only the rows on screen are laid out, so a query that matches
    /// thousands of lines types as fast as one that matches none.
    fn results_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let rows = list(
            self.list.clone(),
            cx.processor(|panel, index: usize, _, cx| panel.render_row(index, cx)),
        )
        .w_full()
        .h(self.list_height());
        div()
            .px(self.surface.list_padding)
            .pb(self.surface.list_padding)
            .child(rows)
            .into_any_element()
    }

    /// A line of muted text under the fields: the replace summary, or why
    /// there are no results.
    fn message(&self, cx: &mut Context<Self>) -> Option<String> {
        if let Some(status) = &self.status {
            return Some(status.clone());
        }
        let has_query = !self.query.read(cx).text().trim().is_empty();
        if !has_query || !self.rows.is_empty() {
            return None;
        }
        Some(if self.load_task.is_some() {
            "Reading your notes…".to_string()
        } else {
            "No notes match".to_string()
        })
    }
}

impl Render for VaultSearch {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The theme can change while the panel is open.
        self.ui = ui_theme(cx);
        self.surface = PickerTheme::from_ui(&self.ui);
        let surface = self.surface.clone();
        let pending = self.pending_replace.clone();
        let message = self.message(cx);
        crate::ui::dialog(&self.ui)
            .key_context(SEARCH_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &SelectNextResult, _, cx| this.select_next(cx)))
            .on_action(
                cx.listener(|this, _: &SelectPreviousResult, _, cx| this.select_previous(cx)),
            )
            .on_action(cx.listener(|this, _: &Confirm, _, cx| this.confirm(cx)))
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| this.dismiss(cx)))
            .on_action(cx.listener(|this, _: &FocusNextField, window, cx| {
                this.focus_other_field(window, cx)
            }))
            .w(self.ui.dialog_width)
            .child(self.query_row(cx))
            .when(self.show_replace, |panel| panel.child(self.replace_row(cx)))
            .when_some(pending, |panel, pending| {
                panel.child(self.confirm_row(&pending, cx))
            })
            .when_some(message, |panel, message| {
                panel.child(
                    div()
                        .px(surface.input_padding_x)
                        .pb(surface.input_padding_y)
                        .text_color(surface.detail_text)
                        .child(message),
                )
            })
            .when(!self.rows.is_empty(), |panel| {
                panel.child(self.results_list(cx))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_list_each_note_then_its_lines() {
        let hit = engine::LineHit {
            line: 0,
            offset: 0,
            excerpt: String::new(),
            ranges: vec![],
        };
        let results = vec![
            NoteResult {
                path: "a.md".into(),
                score: 1,
                match_count: 2,
                hits: vec![hit.clone(), hit],
            },
            NoteResult {
                path: "b.md".into(),
                score: 1,
                match_count: 0,
                hits: vec![],
            },
        ];
        let rows = rows_for(&results);
        assert_eq!(rows.len(), 4);
        assert_eq!(
            rows[1],
            Row {
                note: 0,
                hit: Some(0)
            }
        );
        assert_eq!(rows[3], Row { note: 1, hit: None });
    }

    #[test]
    fn a_line_that_repeats_the_title_is_left_out() {
        let hit = |excerpt: &str| engine::LineHit {
            line: 0,
            offset: 0,
            excerpt: excerpt.into(),
            ranges: vec![],
        };
        let results = vec![NoteResult {
            path: "Physics/Wave Packets.md".into(),
            score: 1,
            match_count: 2,
            hits: vec![hit("Wave packets"), hit("a wave packet spreads")],
        }];
        let rows = rows_for(&results);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[1],
            Row {
                note: 0,
                hit: Some(1)
            }
        );
    }

    #[test]
    fn replace_copy_is_plain() {
        let pending = PendingReplace {
            query: "a".into(),
            replacement: "b".into(),
            paths: vec!["x.md".into()],
            matches: 1,
        };
        assert_eq!(replace_prompt(&pending), "Replace 1 match in 1 note?");
        let report = ReplaceReport {
            changed: vec!["x.md".into(), "y.md".into()],
            matches: 5,
            failed: vec![("z.md".into(), "denied".into())],
        };
        assert_eq!(
            replace_summary(&report),
            "Replaced 5 matches in 2 notes, 1 note couldn't be changed"
        );
    }
}
