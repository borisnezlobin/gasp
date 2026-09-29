//! The find bar: a query field with match count, next and previous, case,
//! whole-word and regex toggles, and an optional replace row. It highlights
//! matches in the editor and keeps them current as the note changes.

use std::ops::Range;

use gasp_config::Platform;
use gasp_config::keys::KeyChord;
use gasp_core::find::{FindOptions, FindQuery, replace_all_transaction, replace_one_transaction};
use gpui::{
    App, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding,
    SharedString, Subscription, Window, actions, div, prelude::*,
};

use crate::editor::{EditorEvent, EditorView, HighlightKind};
use crate::icons::IconName;
use crate::keymap::RunCommand;
use crate::picker::shortcut::Shortcut;
use crate::text_input::{TextInput, TextInputEvent};
use crate::theme::UiTheme;
use crate::ui::{Button, IconButton, popover, ui_theme};

/// The key context the bar sets.
pub const FIND_BAR_CONTEXT: &str = "FindBar";
/// The key context around the replace field.
pub const REPLACE_CONTEXT: &str = "FindReplace";

actions!(
    find_bar,
    [
        SelectNextMatch,
        SelectPreviousMatch,
        Dismiss,
        ToggleCaseSensitive,
        ToggleWholeWord,
        ToggleRegex,
        FocusNextField,
        FocusPreviousField,
        ReplaceNext,
        ReplaceAll,
    ]
);

/// Keys inside the bar. Lists and dialogs share these conventions, so
/// they are not rules.
pub(super) fn bind_bar_keys(cx: &mut App) {
    let bar = Some(FIND_BAR_CONTEXT);
    let replace = Some(REPLACE_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("enter", SelectNextMatch, bar),
        KeyBinding::new("shift-enter", SelectPreviousMatch, bar),
        KeyBinding::new("escape", Dismiss, bar),
        KeyBinding::new("alt-c", ToggleCaseSensitive, bar),
        KeyBinding::new("alt-w", ToggleWholeWord, bar),
        KeyBinding::new("alt-r", ToggleRegex, bar),
        KeyBinding::new("tab", FocusNextField, bar),
        KeyBinding::new("shift-tab", FocusPreviousField, bar),
        KeyBinding::new("enter", ReplaceNext, replace),
        KeyBinding::new("secondary-alt-enter", ReplaceAll, replace),
    ]);
}

/// What the bar tells the workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FindBarEvent {
    /// Escape or the close button: hide the bar. Focus is already back in
    /// the editor.
    Dismissed,
}

/// Find and replace over one editor.
pub struct FindBar {
    editor: Entity<EditorView>,
    query: Entity<TextInput>,
    replacement: Entity<TextInput>,
    focus_handle: FocusHandle,
    options: FindOptions,
    compiled: Option<FindQuery>,
    matches: Vec<Range<usize>>,
    active: Option<usize>,
    /// The match the bar last selected in the editor, so a query that no
    /// longer matches can let go of it.
    revealed: Option<Range<usize>>,
    replace_visible: bool,
    /// Whether the bar is showing. A closed bar ignores edits, so its
    /// highlights don't come back while the note is typed in.
    open: bool,
    theme: UiTheme,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<FindBarEvent> for FindBar {}

impl Focusable for FindBar {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.query.focus_handle(cx)
    }
}

/// "3 of 12", "No results", or nothing for an empty query.
pub fn match_label(active: Option<usize>, count: usize, has_query: bool) -> String {
    match (active, count) {
        (_, 0) if has_query => "No results".to_owned(),
        (_, 0) => String::new(),
        (Some(index), count) => format!("{} of {count}", index + 1),
        (None, count) => format!("{count} found"),
    }
}

impl FindBar {
    /// A bar over `editor`, seeded from its selection and focused.
    pub fn new(editor: Entity<EditorView>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let theme = ui_theme(cx);
        let query = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_placeholder("Find")
                .bubble_enter_and_escape()
        });
        let replacement = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_placeholder("Replace with")
                .bubble_enter_and_escape()
        });
        let subscriptions = vec![
            cx.subscribe(&query, |this, _, event: &TextInputEvent, cx| {
                if *event == TextInputEvent::Changed {
                    this.query_changed(cx)
                }
            }),
            cx.subscribe(&editor, |this, _, event: &EditorEvent, cx| {
                if *event == EditorEvent::Edited && this.open {
                    this.refresh_matches(cx);
                }
            }),
        ];
        let mut bar = Self {
            editor,
            query,
            replacement,
            focus_handle: cx.focus_handle(),
            options: FindOptions::default(),
            compiled: None,
            matches: Vec::new(),
            active: None,
            revealed: None,
            replace_visible: false,
            open: true,
            theme,
            _subscriptions: subscriptions,
        };
        bar.show(false, window, cx);
        bar
    }

    /// Opens (or re-focuses) the bar: seeds the query from a one-line
    /// selection, shows the replace row when `replace` is set, and selects
    /// the query so typing replaces it.
    pub fn show(&mut self, replace: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.open = true;
        self.replace_visible |= replace;
        if let Some(seed) = self.selection_seed(cx) {
            self.query.update(cx, |query, cx| {
                query.set_text(&seed, cx);
                query.select_all(cx);
            });
            self.query_changed(cx);
        } else {
            self.query.update(cx, |query, cx| query.select_all(cx));
            self.refresh_matches(cx);
        }
        // The replacement takes the keyboard only once there's something
        // to find, so typing into a fresh bar always fills the query.
        let has_query = !self.query.read(cx).text().is_empty();
        let target = if replace && has_query {
            &self.replacement
        } else {
            &self.query
        };
        window.focus(&target.focus_handle(cx));
        cx.notify();
    }

    fn selection_seed(&self, cx: &App) -> Option<String> {
        let editor = self.editor.read(cx);
        let range = editor.selected_range();
        let text = editor.doc().slice(range);
        (!text.is_empty() && !text.contains('\n')).then_some(text)
    }

    pub fn query_text(&self, cx: &App) -> String {
        self.query.read(cx).text().to_owned()
    }

    pub fn set_query(&mut self, text: &str, cx: &mut Context<Self>) {
        self.query.update(cx, |query, cx| query.set_text(text, cx));
        self.query_changed(cx);
    }

    pub fn set_replacement(&mut self, text: &str, cx: &mut Context<Self>) {
        self.replacement
            .update(cx, |replacement, cx| replacement.set_text(text, cx));
    }

    pub fn options(&self) -> FindOptions {
        self.options
    }

    pub fn set_options(&mut self, options: FindOptions, cx: &mut Context<Self>) {
        self.options = options;
        self.query_changed(cx);
    }

    pub fn matches(&self) -> &[Range<usize>] {
        &self.matches
    }

    pub fn active_index(&self) -> Option<usize> {
        self.active
    }

    pub fn is_replace_visible(&self) -> bool {
        self.replace_visible
    }

    /// The count shown in the bar, such as "3 of 12".
    pub fn label(&self, cx: &App) -> String {
        let has_query = !self.query.read(cx).text().is_empty();
        match_label(self.active, self.matches.len(), has_query)
    }

    /// The query changed: recompile, then jump to the first match at or
    /// after the selection.
    fn query_changed(&mut self, cx: &mut Context<Self>) {
        let text = self.query.read(cx).text().to_owned();
        let compiled = FindQuery::new(&text, self.options);
        let invalid = compiled.is_err();
        self.query
            .update(cx, |query, cx| query.set_invalid(invalid, cx));
        self.compiled = compiled.ok().flatten();
        let from = self.editor.read(cx).selected_range().start;
        self.recompute(from, cx);
        self.reveal_active(cx);
        self.let_go_of_stale_match(cx);
    }

    /// With no match left, the text an earlier query matched stays
    /// selected and reads as a result; the cursor goes back to its start.
    fn let_go_of_stale_match(&mut self, cx: &mut Context<Self>) {
        if self.active.is_some() {
            return;
        }
        let Some(stale) = self.revealed.take() else {
            return;
        };
        self.editor.update(cx, |editor, cx| {
            if editor.selected_range() == stale {
                editor.select(stale.start, stale.start, cx);
            }
        });
    }

    /// The note changed: find again, keeping the active match near where
    /// it was.
    fn refresh_matches(&mut self, cx: &mut Context<Self>) {
        let from = self.active_range().map_or_else(
            || self.editor.read(cx).selected_range().start,
            |range| range.start,
        );
        self.recompute(from, cx);
    }

    fn recompute(&mut self, from: usize, cx: &mut Context<Self>) {
        let text = self.editor.read(cx).text();
        self.matches = self
            .compiled
            .as_ref()
            .map_or_else(Vec::new, |query| query.matches(&text));
        self.active = gasp_core::find::match_at_or_after(&self.matches, from);
        self.push_highlights(cx);
    }

    fn active_range(&self) -> Option<Range<usize>> {
        self.active
            .and_then(|index| self.matches.get(index).cloned())
    }

    fn push_highlights(&mut self, cx: &mut Context<Self>) {
        let all = self.matches.clone();
        let active: Vec<_> = self.active_range().into_iter().collect();
        self.editor.update(cx, |editor, cx| {
            editor.set_highlights(HighlightKind::SearchMatch, all, cx);
            editor.set_highlights(HighlightKind::ActiveSearchMatch, active, cx);
        });
        cx.notify();
    }

    fn clear_highlights(&mut self, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            editor.set_highlights(HighlightKind::SearchMatch, Vec::new(), cx);
            editor.set_highlights(HighlightKind::ActiveSearchMatch, Vec::new(), cx);
        });
    }

    /// Selects the active match in the editor, which scrolls to it.
    fn reveal_active(&mut self, cx: &mut Context<Self>) {
        if let Some(range) = self.active_range() {
            self.editor
                .update(cx, |editor, cx| editor.select(range.start, range.end, cx));
            self.revealed = Some(range);
        }
    }

    /// Moves to the next match, wrapping. When the editor's selection isn't
    /// the active match, starts from the cursor instead.
    pub fn next(&mut self, cx: &mut Context<Self>) {
        self.step(true, cx);
    }

    /// Moves to the previous match, wrapping.
    pub fn previous(&mut self, cx: &mut Context<Self>) {
        self.step(false, cx);
    }

    fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.matches.is_empty() {
            return;
        }
        let selection = self.editor.read(cx).selected_range();
        let on_active = self.active_range() == Some(selection.clone());
        let count = self.matches.len();
        self.active = Some(match (on_active, self.active, forward) {
            (true, Some(index), true) => gasp_core::find::next_index(index, count),
            (true, Some(index), false) => gasp_core::find::previous_index(index, count),
            (_, _, true) => first_after(&self.matches, selection.end),
            (_, _, false) => last_before(&self.matches, selection.start),
        });
        self.push_highlights(cx);
        self.reveal_active(cx);
    }

    /// Replaces the active match and moves to the next one.
    pub fn replace_next(&mut self, cx: &mut Context<Self>) {
        let (Some(query), Some(range)) = (self.compiled.clone(), self.active_range()) else {
            return;
        };
        let replacement = self.replacement.read(cx).text().to_owned();
        self.editor.update(cx, |editor, cx| {
            let transaction =
                replace_one_transaction(editor.doc(), &query, range, &replacement, editor.now_ms());
            if let Some(transaction) = transaction {
                editor.apply_transaction(transaction, cx);
            }
        });
        let cursor = self.editor.read(cx).cursor();
        self.recompute(cursor, cx);
        self.reveal_active(cx);
    }

    /// Replaces every match as one undo step. Returns how many.
    pub fn replace_all(&mut self, cx: &mut Context<Self>) -> usize {
        let Some(query) = self.compiled.clone() else {
            return 0;
        };
        let count = self.matches.len();
        let replacement = self.replacement.read(cx).text().to_owned();
        self.editor.update(cx, |editor, cx| {
            let transaction =
                replace_all_transaction(editor.doc(), &query, &replacement, editor.now_ms());
            if let Some(transaction) = transaction {
                editor.apply_transaction(transaction, cx);
            }
        });
        let cursor = self.editor.read(cx).cursor();
        self.recompute(cursor, cx);
        count
    }

    /// Hides the bar's highlights, selects the active match and gives focus
    /// back to the editor.
    pub fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        self.reveal_active(cx);
        self.clear_highlights(cx);
        window.focus(&self.editor.focus_handle(cx));
        cx.emit(FindBarEvent::Dismissed);
    }

    fn toggle(&mut self, flip: fn(&mut FindOptions), cx: &mut Context<Self>) {
        flip(&mut self.options);
        self.query_changed(cx);
    }

    fn focus_other_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.replace_visible {
            return;
        }
        let target = if self.query.focus_handle(cx).is_focused(window) {
            &self.replacement
        } else {
            &self.query
        };
        window.focus(&target.focus_handle(cx));
    }

    /// Find commands pressed while the bar has focus.
    fn on_run_command(&mut self, action: &RunCommand, window: &mut Window, cx: &mut Context<Self>) {
        match action.id.as_ref() {
            "find.next" => self.next(cx),
            "find.previous" => self.previous(cx),
            "find.open" => self.show(false, window, cx),
            "find.replace" => self.show(true, window, cx),
            _ => cx.propagate(),
        }
    }
}

/// The first match starting at or after `offset`, wrapping to the first.
fn first_after(matches: &[Range<usize>], offset: usize) -> usize {
    gasp_core::find::match_at_or_after(matches, offset).unwrap_or(0)
}

/// The last match ending at or before `offset`, wrapping to the last.
fn last_before(matches: &[Range<usize>], offset: usize) -> usize {
    matches
        .iter()
        .rposition(|range| range.end <= offset)
        .unwrap_or(matches.len().saturating_sub(1))
}

type BarAction = fn(&mut FindBar, &ClickEvent, &mut Window, &mut Context<FindBar>);

/// A key the bar binds itself, such as `Alt+C`, for a tooltip.
fn bar_key(chord: &str) -> Option<Shortcut> {
    let chord = KeyChord::parse(chord).ok()?;
    Some(Shortcut::new(chord, Platform::current()))
}

impl FindBar {
    /// An icon button in the bar that says what it does. `on` makes it a
    /// toggle.
    #[allow(clippy::too_many_arguments)]
    fn icon_button(
        &self,
        id: &'static str,
        name: IconName,
        label: &'static str,
        hint: Option<Shortcut>,
        on: Option<bool>,
        action: BarAction,
        cx: &mut Context<Self>,
    ) -> IconButton {
        IconButton::new(id, name)
            .tooltip_with_shortcut(label, hint)
            .toggled(on == Some(true))
            .on_click(cx.listener(action))
    }

    fn toggles(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_none()
            .gap(self.theme.space_xs)
            .child(self.icon_button(
                "find-case",
                IconName::TextAa,
                "Match case",
                bar_key("Alt+C"),
                Some(self.options.case_sensitive),
                |this, _, _, cx| this.toggle(|options| options.case_sensitive ^= true, cx),
                cx,
            ))
            .child(self.icon_button(
                "find-word",
                IconName::TextT,
                "Match whole words",
                bar_key("Alt+W"),
                Some(self.options.whole_word),
                |this, _, _, cx| this.toggle(|options| options.whole_word ^= true, cx),
                cx,
            ))
            .child(self.icon_button(
                "find-regex",
                IconName::Asterisk,
                "Use a regular expression",
                bar_key("Alt+R"),
                Some(self.options.regex),
                |this, _, _, cx| this.toggle(|options| options.regex ^= true, cx),
                cx,
            ))
    }

    fn navigation(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let hint = |id: &str| crate::ui::hints::shortcut(id, cx);
        let (previous, next) = (hint("find.previous"), hint("find.next"));
        div()
            .flex()
            .flex_none()
            .gap(self.theme.space_xs)
            .child(self.icon_button(
                "find-previous",
                IconName::CaretUp,
                "Previous match",
                previous,
                None,
                |this, _, _, cx| this.previous(cx),
                cx,
            ))
            .child(self.icon_button(
                "find-next",
                IconName::CaretDown,
                "Next match",
                next,
                None,
                |this, _, _, cx| this.next(cx),
                cx,
            ))
            .child(self.icon_button(
                "find-close",
                IconName::X,
                "Close",
                bar_key("Escape"),
                None,
                |this, _, window, cx| this.dismiss(window, cx),
                cx,
            ))
    }

    fn find_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let label: SharedString = self.label(cx).into();
        let has_label = !label.is_empty();
        div()
            .flex()
            .items_center()
            .gap(theme.space_sm)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .child(self.query.clone())
                    // The count sits inside the field's right end, so a
                    // new count never moves the buttons.
                    .when(has_label, |field| {
                        field.child(
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .right(theme.space_md)
                                .flex()
                                .items_center()
                                .text_size(theme.small_font_size)
                                .text_color(theme.text_detail)
                                .child(label),
                        )
                    }),
            )
            .child(self.toggles(cx))
            .child(div().w(theme.space_xs))
            .child(self.navigation(cx))
    }

    fn replace_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context(REPLACE_CONTEXT)
            .on_action(cx.listener(|this, _: &ReplaceNext, _, cx| this.replace_next(cx)))
            .on_action(cx.listener(|this, _: &ReplaceAll, _, cx| {
                this.replace_all(cx);
            }))
            .flex()
            .items_center()
            .gap(self.theme.space_sm)
            .child(div().flex_1().min_w_0().child(self.replacement.clone()))
            .child(
                Button::new("replace-next", "Replace")
                    .on_click(cx.listener(|this, _, _, cx| this.replace_next(cx))),
            )
            .child(
                Button::new("replace-all", "Replace all").on_click(cx.listener(
                    |this, _, _, cx| {
                        this.replace_all(cx);
                    },
                )),
            )
    }
}

impl Render for FindBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The theme can change while the bar is open.
        self.theme = ui_theme(cx);
        let theme = self.theme.clone();
        popover(&theme)
            .key_context(FIND_BAR_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_run_command))
            .on_action(cx.listener(|this, _: &SelectNextMatch, _, cx| this.next(cx)))
            .on_action(cx.listener(|this, _: &SelectPreviousMatch, _, cx| this.previous(cx)))
            .on_action(cx.listener(|this, _: &Dismiss, window, cx| this.dismiss(window, cx)))
            .on_action(cx.listener(|this, _: &ToggleCaseSensitive, _, cx| {
                this.toggle(|options| options.case_sensitive ^= true, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleWholeWord, _, cx| {
                this.toggle(|options| options.whole_word ^= true, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleRegex, _, cx| {
                this.toggle(|options| options.regex ^= true, cx)
            }))
            .on_action(cx.listener(|this, _: &FocusNextField, window, cx| {
                this.focus_other_field(window, cx)
            }))
            .on_action(cx.listener(|this, _: &FocusPreviousField, window, cx| {
                this.focus_other_field(window, cx)
            }))
            .occlude()
            .w(theme.find_bar_width)
            .max_w_full()
            .gap(theme.space_sm)
            .p(theme.space_sm)
            .text_size(theme.small_font_size + gpui::px(1.))
            .child(self.find_row(cx))
            .when(self.replace_visible, |bar| bar.child(self.replace_row(cx)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_count_matches() {
        assert_eq!(match_label(Some(2), 12, true), "3 of 12");
        assert_eq!(match_label(None, 0, true), "No results");
        assert_eq!(match_label(None, 0, false), "");
        assert_eq!(match_label(None, 4, true), "4 found");
    }

    #[test]
    fn stepping_from_the_cursor_wraps() {
        let matches = vec![2..4, 6..8, 10..12];
        assert_eq!(first_after(&matches, 5), 1);
        assert_eq!(first_after(&matches, 11), 0);
        assert_eq!(last_before(&matches, 9), 1);
        assert_eq!(last_before(&matches, 1), 2);
    }
}
