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

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui::{
    App, AppContext, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable,
    HighlightStyle, KeyBinding, ScrollHandle, SharedString, StyledText, Subscription, Task, Window,
    actions, div, prelude::*,
};

use crate::text_input::{TextInput, TextInputEvent};
use crate::theme::{FindUiTheme, Theme};
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
    scroll: ScrollHandle,
    focus_handle: FocusHandle,
    theme: FindUiTheme,
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
    /// A panel over the notes under `root`, which it starts loading.
    pub fn new(root: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let theme = Theme::default().find_ui;
        let query = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_placeholder("Search all notes")
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
        let mut panel = Self {
            root,
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
            scroll: ScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            theme,
            _subscriptions: subscriptions,
        };
        panel.refresh(cx);
        panel
    }

    /// Reloads the notes from disk and searches again. Call it when the
    /// panel is shown.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let root = self.root.clone();
        let loading = cx.background_spawn(async move { engine::load_vault(&root) });
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let notes = loading.await;
            this.update(cx, |panel, cx| {
                panel.notes = Arc::new(notes);
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
        let searching = cx
            .background_spawn(async move { engine::search(&notes, &query, &generation, current) });
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
        self.scroll.scroll_to_item(0);
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
        self.scroll.scroll_to_item(index);
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

    fn focus_other_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = if self.query.focus_handle(cx).is_focused(window) {
            &self.replacement
        } else {
            &self.query
        };
        window.focus(&target.focus_handle(cx));
    }
}

/// Every note followed by its matching lines.
pub fn rows_for(results: &[NoteResult]) -> Vec<Row> {
    results
        .iter()
        .enumerate()
        .flat_map(|(note, result)| {
            std::iter::once(Row { note, hit: None }).chain((0..result.hits.len()).map(move |hit| {
                Row {
                    note,
                    hit: Some(hit),
                }
            }))
        })
        .collect()
}

type PanelAction = fn(&mut VaultSearch, &ClickEvent, &mut Window, &mut Context<VaultSearch>);

impl VaultSearch {
    fn button(
        &self,
        id: &'static str,
        label: &'static str,
        primary: bool,
        action: PanelAction,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = &self.theme;
        let (background, color, hover) = if primary {
            (
                theme.accent_background,
                theme.accent_text,
                theme.accent_background,
            )
        } else {
            (
                gpui::transparent_black(),
                theme.text,
                theme.button_hover_background,
            )
        };
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .h(theme.button_size)
            .px(theme.button_padding_x)
            .rounded(theme.radius)
            .bg(background)
            .text_color(color)
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(action))
            .child(label)
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
            .gap(self.theme.gap)
            .child(div().flex_1().min_w_0().child(self.replacement.clone()))
            .child(self.button(
                "replace-all",
                "Replace all",
                false,
                |this, _, _, cx| this.request_replace_all(cx),
                cx,
            ))
    }

    fn confirm_row(&self, pending: &PendingReplace, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(self.theme.gap)
            .child(div().flex_1().child(replace_prompt(pending)))
            .child(self.button(
                "replace-cancel",
                "Cancel",
                false,
                |this, _, _, cx| this.cancel_replace(cx),
                cx,
            ))
            .child(self.button(
                "replace-confirm",
                "Replace",
                true,
                |this, _, _, cx| this.confirm_replace(cx),
                cx,
            ))
    }

    fn render_row(&self, index: usize, row: Row, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let note = &self.results[row.note];
        let background = if index == self.selected {
            theme.row_selected_background
        } else {
            gpui::transparent_black()
        };
        let base = div()
            .id(("search-row", index))
            .w_full()
            .py(theme.row_padding_y)
            .px(theme.panel_padding)
            .rounded(theme.radius)
            .bg(background)
            .hover(|style| style.bg(theme.button_hover_background))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected = index;
                this.open_selected(cx);
            }));
        match row.hit {
            None => base
                .when(index > 0, |row| row.mt(theme.gap))
                .flex()
                .gap(theme.gap)
                .child(
                    div()
                        .flex_1()
                        .overflow_hidden()
                        .child(note.path.with_extension("").to_string_lossy().into_owned()),
                )
                .child(
                    div()
                        .text_size(theme.small_font_size)
                        .text_color(theme.muted_text)
                        .child(note.match_count.to_string()),
                ),
            Some(hit) => {
                let hit = &note.hits[hit];
                let highlight = HighlightStyle {
                    background_color: Some(theme.match_background),
                    ..HighlightStyle::default()
                };
                let excerpt = StyledText::new(SharedString::from(hit.excerpt.clone()))
                    .with_highlights(hit.ranges.iter().map(|range| (range.clone(), highlight)));
                base.pl(theme.panel_padding + theme.result_indent)
                    .text_size(theme.small_font_size)
                    .text_color(theme.muted_text)
                    .overflow_hidden()
                    .child(excerpt)
            }
        }
    }

    fn results_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let rows: Vec<_> = self
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| self.render_row(index, *row, cx).into_any_element())
            .collect();
        div()
            .id("vault-search-results")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .children(rows)
    }
}

impl Render for VaultSearch {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.clone();
        let status = self.status.clone();
        let pending = self.pending_replace.clone();
        let has_query = !self.query.read(cx).text().trim().is_empty();
        let empty = has_query && self.rows.is_empty();
        div()
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
            .flex()
            .flex_col()
            .size_full()
            .gap(theme.gap)
            .p(theme.panel_padding)
            .bg(theme.panel_background)
            .font_family(theme.font_family)
            .text_size(theme.font_size)
            .text_color(theme.text)
            .child(self.query.clone())
            .child(self.replace_row(cx))
            .when_some(pending, |panel, pending| {
                panel.child(self.confirm_row(&pending, cx))
            })
            .when_some(status, |panel, status| {
                panel.child(div().text_color(theme.muted_text).child(status))
            })
            .when(empty, |panel| {
                panel.child(div().text_color(theme.muted_text).child("No notes match"))
            })
            .child(self.results_list(cx))
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
