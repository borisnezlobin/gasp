//! The find bar: a query field with match count, next and previous, case,
//! whole-word and regex toggles, and an optional replace row. It highlights
//! matches in the editor and keeps them current as the note changes.

use std::ops::Range;

use editor_core::find::{FindOptions, FindQuery, replace_all_transaction, replace_one_transaction};
use gpui::{
    App, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding,
    SharedString, Subscription, Window, actions, div, prelude::*,
};

use crate::editor::{EditorEvent, EditorView, HighlightKind};
use crate::icons::{IconName, icon};
use crate::keymap::RunCommand;
use crate::text_input::{TextInput, TextInputEvent};
use crate::theme::FindUiTheme;

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
    replace_visible: bool,
    theme: FindUiTheme,
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
        let theme = FindUiTheme {
            font_family: crate::ui::ui_theme(cx).font_family,
            ..editor.read(cx).theme.find_ui.clone()
        };
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
                if *event == EditorEvent::Edited {
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
            replace_visible: false,
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
        let target = if replace {
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
        self.active = editor_core::find::match_at_or_after(&self.matches, from);
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
            (true, Some(index), true) => editor_core::find::next_index(index, count),
            (true, Some(index), false) => editor_core::find::previous_index(index, count),
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
    editor_core::find::match_at_or_after(matches, offset).unwrap_or(0)
}

/// The last match ending at or before `offset`, wrapping to the last.
fn last_before(matches: &[Range<usize>], offset: usize) -> usize {
    matches
        .iter()
        .rposition(|range| range.end <= offset)
        .unwrap_or(matches.len().saturating_sub(1))
}

type BarAction = fn(&mut FindBar, &ClickEvent, &mut Window, &mut Context<FindBar>);

impl FindBar {
    fn icon_button(
        &self,
        id: &'static str,
        name: IconName,
        on: Option<bool>,
        action: BarAction,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = &self.theme;
        let is_on = on == Some(true);
        let (background, color) = if is_on {
            (theme.accent_background, theme.accent_text)
        } else {
            (gpui::transparent_black(), theme.icon)
        };
        let hover = if is_on {
            theme.accent_background
        } else {
            theme.button_hover_background
        };
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(theme.button_size)
            .rounded(theme.radius)
            .bg(background)
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(action))
            .child(icon(name).size(theme.icon_size).text_color(color))
    }

    fn text_button(
        &self,
        id: &'static str,
        label: &'static str,
        action: BarAction,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = &self.theme;
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .h(theme.button_size)
            .px(theme.button_padding_x)
            .rounded(theme.radius)
            .hover(|style| style.bg(theme.button_hover_background))
            .on_click(cx.listener(action))
            .child(label)
    }

    fn find_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let label: SharedString = self.label(cx).into();
        div()
            .flex()
            .items_center()
            .gap(theme.gap)
            .child(div().flex_1().min_w_0().child(self.query.clone()))
            .child(
                div()
                    .flex_none()
                    .text_size(theme.small_font_size)
                    .text_color(theme.muted_text)
                    .child(label),
            )
            .child(self.icon_button(
                "find-case",
                IconName::TextAa,
                Some(self.options.case_sensitive),
                |this, _, _, cx| this.toggle(|options| options.case_sensitive ^= true, cx),
                cx,
            ))
            .child(self.icon_button(
                "find-word",
                IconName::TextT,
                Some(self.options.whole_word),
                |this, _, _, cx| this.toggle(|options| options.whole_word ^= true, cx),
                cx,
            ))
            .child(self.icon_button(
                "find-regex",
                IconName::Asterisk,
                Some(self.options.regex),
                |this, _, _, cx| this.toggle(|options| options.regex ^= true, cx),
                cx,
            ))
            .child(self.icon_button(
                "find-previous",
                IconName::CaretUp,
                None,
                |this, _, _, cx| this.previous(cx),
                cx,
            ))
            .child(self.icon_button(
                "find-next",
                IconName::CaretDown,
                None,
                |this, _, _, cx| this.next(cx),
                cx,
            ))
            .child(self.icon_button(
                "find-close",
                IconName::X,
                None,
                |this, _, window, cx| this.dismiss(window, cx),
                cx,
            ))
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
            .gap(self.theme.gap)
            .child(div().flex_1().min_w_0().child(self.replacement.clone()))
            .child(self.text_button(
                "replace-next",
                "Replace",
                |this, _, _, cx| this.replace_next(cx),
                cx,
            ))
            .child(self.text_button(
                "replace-all",
                "Replace all",
                |this, _, _, cx| {
                    this.replace_all(cx);
                },
                cx,
            ))
    }
}

impl Render for FindBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.clone();
        div()
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
            .flex()
            .flex_col()
            .w_full()
            .gap(theme.gap)
            .p(theme.panel_padding)
            .bg(theme.panel_background)
            .shadow(vec![gpui::BoxShadow {
                color: theme.panel_shadow,
                offset: gpui::point(gpui::px(0.), gpui::px(1.)),
                blur_radius: theme.panel_shadow_blur,
                spread_radius: gpui::px(0.),
            }])
            .font_family(theme.font_family.clone())
            .text_size(theme.font_size)
            .text_color(theme.text)
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
