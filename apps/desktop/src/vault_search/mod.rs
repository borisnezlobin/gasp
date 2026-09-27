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

use std::ops::Range;

use gpui::{
    AnyElement, App, AppContext, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable,
    HighlightStyle, KeyBinding, ScrollStrategy, SharedString, StyledText, Subscription, Task,
    UniformListScrollHandle, Window, actions, div, prelude::*, uniform_list,
};

use crate::icons::{IconName, icon};
use crate::picker::surface_shadow;
use crate::text_input::{TextInput, TextInputEvent, TextInputStyle};
use crate::theme::{FindUiTheme, PickerTheme, Theme};
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
    scroll: UniformListScrollHandle,
    focus_handle: FocusHandle,
    theme: FindUiTheme,
    /// The picker's surface, so the panel looks like the quick switcher.
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
    /// A panel over the notes under `root`, which it starts loading.
    pub fn new(root: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let theme = FindUiTheme {
            font_family: crate::ui::ui_theme(cx).font_family,
            ..Theme::default().find_ui
        };
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
            scroll: UniformListScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            surface: PickerTheme {
                font_family: theme.font_family.clone(),
                ..PickerTheme::default()
            },
            theme,
            show_replace: false,
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
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
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
        self.scroll.scroll_to_item(index, ScrollStrategy::Top);
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

/// Result rows shown before the list scrolls.
const VISIBLE_ROWS: usize = 12;

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
            .gap(self.theme.gap)
            .px(surface.input_padding_x)
            .py(surface.input_padding_y)
            .child(div().flex_1().min_w_0().child(self.query.clone()))
            .child(self.button(
                "toggle-replace",
                label,
                false,
                |this, _, window, cx| this.toggle_replace(window, cx),
                cx,
            ))
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
            .px(self.surface.input_padding_x)
            .pb(self.theme.gap)
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
            .px(self.surface.input_padding_x)
            .pb(self.theme.gap)
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

    /// A note's row: its name, the folder it's in, and how many matches.
    fn note_row(&self, note: &NoteResult) -> gpui::Div {
        let theme = &self.theme;
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
            .child(
                icon(IconName::FileText)
                    .flex_none()
                    .size(self.surface.icon_size)
                    .text_color(self.surface.icon),
            )
            .child(div().flex_none().child(name))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(theme.small_font_size)
                    .text_color(self.surface.detail_text)
                    .child(folder),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(theme.small_font_size)
                    .text_color(self.surface.detail_text)
                    .child(note.match_count.to_string()),
            )
    }

    /// One matching line, indented under its note, with the matches marked.
    fn hit_row(&self, hit: &engine::LineHit) -> gpui::Div {
        let theme = &self.theme;
        let highlight = HighlightStyle {
            background_color: Some(theme.match_background),
            color: Some(theme.text),
            ..HighlightStyle::default()
        };
        let excerpt = StyledText::new(SharedString::from(hit.excerpt.clone()))
            .with_highlights(hit.ranges.iter().map(|range| (range.clone(), highlight)));
        div()
            .pl(self.surface.icon_size + self.surface.row_gap)
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .text_size(theme.small_font_size)
            .text_color(theme.muted_text)
            .child(excerpt)
    }

    fn render_rows(&self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let surface = &self.surface;
        range
            .map(|index| {
                let row = self.rows[index];
                let note = &self.results[row.note];
                let content = match row.hit {
                    None => self.note_row(note),
                    Some(hit) => self.hit_row(&note.hits[hit]),
                };
                let selected = index == self.selected;
                div()
                    .id(("search-row", index))
                    .w_full()
                    .h(surface.row_height)
                    .px(surface.row_padding_x)
                    .flex()
                    .items_center()
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
            })
            .collect()
    }

    /// Only the rows on screen are laid out, so a query that matches
    /// thousands of lines types as fast as one that matches none.
    fn results_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let surface = &self.surface;
        let shown = self.rows.len().min(VISIBLE_ROWS);
        let list = uniform_list(
            "vault-search-results",
            self.rows.len(),
            cx.processor(|panel, range, _, cx| panel.render_rows(range, cx)),
        )
        .track_scroll(self.scroll.clone())
        .w_full()
        .h(surface.row_height * shown as f32);
        div()
            .px(surface.list_padding)
            .pb(surface.list_padding)
            .child(list)
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
        let theme = self.theme.clone();
        let surface = self.surface.clone();
        let pending = self.pending_replace.clone();
        let message = self.message(cx);
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
            .w_full()
            .bg(surface.background)
            .rounded(surface.corner_radius)
            .shadow(vec![surface_shadow(&surface)])
            .font_family(theme.font_family.clone())
            .text_size(surface.row_font_size)
            .text_color(theme.text)
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
