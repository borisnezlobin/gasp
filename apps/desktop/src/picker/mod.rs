//! A generic modal picker: a query input over a filtered, virtualised list.
//!
//! A [`PickerDelegate`] owns the items, filters them for a query, draws a
//! row and turns a confirmed row into an event. [`Picker`] does the rest:
//! typing filters, Up and Down (and Ctrl+N and Ctrl+P) move, Page Up and
//! Page Down jump, Enter confirms, Mod+Enter confirms the secondary way,
//! Escape dismisses, and rows answer hover and click.
//!
//! Call [`bind_keys`] once at startup, next to `keymap::bind_rules`.

pub mod fuzzy;
pub mod input;
pub mod shortcut;

use std::ops::Range;

use editor_config::{Platform, RuleSet};
use gpui::{
    AnyElement, App, BoxShadow, ClickEvent, Context, DismissEvent, Entity, EventEmitter,
    FocusHandle, Focusable, HighlightStyle, KeyBinding, ParentElement, Render, ScrollStrategy,
    SharedString, StyledText, Subscription, UniformListScrollHandle, Window, div, point,
    prelude::*, px, uniform_list,
};

use crate::keymap::{RunCommand, keystroke_for};
use crate::theme::PickerTheme;
use input::{INPUT_CONTEXT, QueryEvent, QueryInput, input_command_ids};

/// The key context the picker sets around its input and list.
pub const PICKER_CONTEXT: &str = "Picker";

gpui::actions!(
    picker,
    [
        SelectNext,
        SelectPrevious,
        SelectNextPage,
        SelectPreviousPage,
        Confirm,
        SecondaryConfirm,
        Dismiss
    ]
);

/// Binds the picker's list keys, and the query input's editing keys from
/// the editing rules in `rules` (so the input edits like the editor).
pub fn bind_keys(rules: &RuleSet, cx: &mut App) {
    let context = Some(PICKER_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("down", SelectNext, context),
        KeyBinding::new("ctrl-n", SelectNext, context),
        KeyBinding::new("up", SelectPrevious, context),
        KeyBinding::new("ctrl-p", SelectPrevious, context),
        KeyBinding::new("pagedown", SelectNextPage, context),
        KeyBinding::new("pageup", SelectPreviousPage, context),
        KeyBinding::new("enter", Confirm, context),
        KeyBinding::new("secondary-enter", SecondaryConfirm, context),
        KeyBinding::new("escape", Dismiss, context),
    ]);
    let platform = Platform::current();
    let bindings = input_keystrokes(rules, platform)
        .into_iter()
        .map(|(keystroke, id)| {
            KeyBinding::new(
                &keystroke,
                RunCommand { id: id.into() },
                Some(INPUT_CONTEXT),
            )
        });
    cx.bind_keys(bindings);
}

/// (GPUI keystroke, command id) for every editing rule the query input runs.
pub fn input_keystrokes(rules: &RuleSet, platform: Platform) -> Vec<(String, String)> {
    rules
        .key_rules(platform)
        .filter(|rule| rule.when.is_none())
        .filter(|rule| input_command_ids().any(|id| id == rule.command))
        .filter_map(|rule| {
            let chord = rule.chord_for(platform)?;
            Some((keystroke_for(chord, platform), rule.command.clone()))
        })
        .collect()
}

/// What a picker's owner supplies: the items, how to filter and draw them,
/// and what confirming one means.
pub trait PickerDelegate: Sized + 'static {
    /// What confirming a row produces.
    type Event: 'static;

    /// The input's placeholder, such as "Run a command".
    fn placeholder(&self) -> SharedString;

    /// The number of rows for the current query.
    fn match_count(&self) -> usize;

    /// Filters and ranks the items for `query`.
    fn update_matches(&mut self, query: &str);

    /// The row selected after the matches change.
    fn default_selection(&self, _query: &str) -> usize {
        0
    }

    /// The content of row `index`. The picker draws the row around it.
    fn render_match(&self, index: usize, selected: bool, theme: &PickerTheme) -> AnyElement;

    /// Enter on row `index`.
    fn confirm(&mut self, index: usize) -> Option<Self::Event>;

    /// Mod+Enter on row `index`.
    fn secondary_confirm(&mut self, index: usize) -> Option<Self::Event> {
        self.confirm(index)
    }

    /// The sentence shown when no row matches `query`.
    fn empty_message(&self, query: &str) -> SharedString;
}

/// A row was confirmed. The picker's owner decides whether to close.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confirmed<E>(pub E);

/// A modal picker. It emits [`Confirmed`] for a confirmed row and
/// [`DismissEvent`] on Escape.
pub struct Picker<D: PickerDelegate> {
    delegate: D,
    query: Entity<QueryInput>,
    selected: usize,
    scroll: UniformListScrollHandle,
    theme: PickerTheme,
    _query_changes: Subscription,
}

impl<D: PickerDelegate> EventEmitter<Confirmed<D::Event>> for Picker<D> {}
impl<D: PickerDelegate> EventEmitter<DismissEvent> for Picker<D> {}

impl<D: PickerDelegate> Focusable for Picker<D> {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.query.focus_handle(cx)
    }
}

impl<D: PickerDelegate> Picker<D> {
    /// A picker over `delegate`, focused and showing the matches for an
    /// empty query.
    pub fn new(mut delegate: D, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let theme = PickerTheme::default();
        let placeholder = delegate.placeholder();
        let query_theme = theme.clone();
        let query = cx.new(|cx| QueryInput::new(placeholder, query_theme, cx));
        let subscription = cx.subscribe(&query, |picker, _, event: &QueryEvent, cx| {
            let QueryEvent::Changed = event;
            picker.refresh(cx);
        });
        delegate.update_matches("");
        let selected = delegate.default_selection("");
        window.focus(&query.focus_handle(cx));
        let scroll = UniformListScrollHandle::new();
        scroll.scroll_to_item(selected, ScrollStrategy::Center);
        Self {
            delegate,
            query,
            selected,
            scroll,
            theme,
            _query_changes: subscription,
        }
    }

    pub fn delegate(&self) -> &D {
        &self.delegate
    }

    pub fn delegate_mut(&mut self) -> &mut D {
        &mut self.delegate
    }

    pub fn theme(&self) -> &PickerTheme {
        &self.theme
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn query_input(&self) -> &Entity<QueryInput> {
        &self.query
    }

    pub fn query(&self, cx: &App) -> String {
        self.query.read(cx).text().to_string()
    }

    /// Replaces the query, which filters the list.
    pub fn set_query(&mut self, text: &str, cx: &mut Context<Self>) {
        self.query.update(cx, |query, cx| query.set_text(text, cx));
    }

    /// Filters again, as after the items changed.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let query = self.query(cx);
        self.delegate.update_matches(&query);
        self.selected = self.delegate.default_selection(&query);
        self.scroll
            .scroll_to_item(self.selected, ScrollStrategy::Top);
        cx.notify();
    }

    /// Selects row `index`, clamped to the rows there are.
    pub fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        let count = self.delegate.match_count();
        if count == 0 {
            return;
        }
        self.selected = index.min(count - 1);
        self.scroll
            .scroll_to_item(self.selected, ScrollStrategy::Top);
        cx.notify();
    }

    fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
        let count = self.delegate.match_count();
        if count == 0 {
            return;
        }
        let next = if forward {
            (self.selected + 1) % count
        } else {
            (self.selected + count - 1) % count
        };
        self.select(next, cx);
    }

    fn page(&mut self, forward: bool, cx: &mut Context<Self>) {
        let rows = self.theme.visible_rows.saturating_sub(1).max(1);
        let target = if forward {
            self.selected + rows
        } else {
            self.selected.saturating_sub(rows)
        };
        self.select(target, cx);
    }

    /// Confirms the selected row, the secondary way when `secondary` is set.
    pub fn confirm(&mut self, secondary: bool, cx: &mut Context<Self>) {
        if self.selected >= self.delegate.match_count() {
            return;
        }
        let event = if secondary {
            self.delegate.secondary_confirm(self.selected)
        } else {
            self.delegate.confirm(self.selected)
        };
        if let Some(event) = event {
            cx.emit(Confirmed(event));
        }
        cx.notify();
    }

    fn on_select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step(true, cx);
    }

    fn on_select_previous(&mut self, _: &SelectPrevious, _: &mut Window, cx: &mut Context<Self>) {
        self.step(false, cx);
    }

    fn on_next_page(&mut self, _: &SelectNextPage, _: &mut Window, cx: &mut Context<Self>) {
        self.page(true, cx);
    }

    fn on_previous_page(&mut self, _: &SelectPreviousPage, _: &mut Window, cx: &mut Context<Self>) {
        self.page(false, cx);
    }

    fn on_confirm(&mut self, _: &Confirm, _: &mut Window, cx: &mut Context<Self>) {
        self.confirm(false, cx);
    }

    fn on_secondary_confirm(
        &mut self,
        _: &SecondaryConfirm,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.confirm(true, cx);
    }

    fn on_dismiss(&mut self, _: &Dismiss, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn on_row_click(&mut self, index: usize, event: &ClickEvent, cx: &mut Context<Self>) {
        self.select(index, cx);
        self.confirm(event.modifiers().secondary(), cx);
    }

    fn render_rows(&self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = &self.theme;
        range
            .map(|index| {
                let selected = index == self.selected;
                div()
                    .id(index)
                    .h(theme.row_height)
                    .px(theme.row_padding_x)
                    .flex()
                    .items_center()
                    .rounded(theme.row_corner_radius)
                    .when(selected, |row| row.bg(theme.selected_row))
                    .when(!selected, |row| {
                        row.hover(|style| style.bg(theme.hovered_row))
                    })
                    .on_click(cx.listener(move |picker, event: &ClickEvent, _, cx| {
                        picker.on_row_click(index, event, cx);
                    }))
                    .child(self.delegate.render_match(index, selected, theme))
                    .into_any_element()
            })
            .collect()
    }

    fn render_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = &self.theme;
        let count = self.delegate.match_count();
        if count == 0 {
            let message = self.delegate.empty_message(&self.query(cx));
            return div()
                .px(theme.input_padding_x)
                .pb(theme.input_padding_y)
                .text_size(theme.row_font_size)
                .text_color(theme.detail_text)
                .child(message)
                .into_any_element();
        }
        let rows = count.min(theme.visible_rows);
        let list = uniform_list(
            "picker-matches",
            count,
            cx.processor(|picker, range, _, cx| picker.render_rows(range, cx)),
        )
        .track_scroll(self.scroll.clone())
        .h(theme.row_height * rows as f32);
        div()
            .px(theme.list_padding)
            .pb(theme.list_padding)
            .child(list)
            .into_any_element()
    }
}

impl<D: PickerDelegate> Render for Picker<D> {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.clone();
        div()
            .key_context(PICKER_CONTEXT)
            .on_action(cx.listener(Self::on_select_next))
            .on_action(cx.listener(Self::on_select_previous))
            .on_action(cx.listener(Self::on_next_page))
            .on_action(cx.listener(Self::on_previous_page))
            .on_action(cx.listener(Self::on_confirm))
            .on_action(cx.listener(Self::on_secondary_confirm))
            .on_action(cx.listener(Self::on_dismiss))
            .w(theme.width)
            .flex()
            .flex_col()
            .font_family(theme.font_family)
            .text_color(theme.text)
            .bg(theme.background)
            .rounded(theme.corner_radius)
            .shadow(vec![surface_shadow(&theme)])
            .child(
                div()
                    .px(theme.input_padding_x)
                    .py(theme.input_padding_y)
                    .child(self.query.clone()),
            )
            .child(self.render_list(cx))
    }
}

/// The shadow that lifts a picker off the page.
pub fn surface_shadow(theme: &PickerTheme) -> BoxShadow {
    BoxShadow {
        color: theme.shadow,
        offset: point(px(0.), theme.shadow_offset_y),
        blur_radius: theme.shadow_blur,
        spread_radius: px(0.),
    }
}

/// `text` with the chars at byte offsets `positions` drawn as matches.
pub fn highlighted_text(
    text: impl Into<SharedString>,
    positions: &[usize],
    theme: &PickerTheme,
) -> StyledText {
    let text = text.into();
    let style = HighlightStyle {
        color: Some(theme.match_text),
        font_weight: Some(theme.match_weight),
        ..HighlightStyle::default()
    };
    let highlights = match_ranges(&text, positions)
        .into_iter()
        .map(|range| (range, style));
    StyledText::new(text).with_highlights(highlights)
}

/// Merges matched char offsets into ranges of adjacent chars, dropping any
/// that fall outside `text`.
pub fn match_ranges(text: &str, positions: &[usize]) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for &start in positions {
        let Some(c) = text.get(start..).and_then(|rest| rest.chars().next()) else {
            continue;
        };
        let end = start + c.len_utf8();
        match ranges.last_mut() {
            Some(last) if last.end == start => last.end = end,
            _ => ranges.push(start..end),
        }
    }
    ranges
}

/// A shortcut drawn as a small key cap.
pub fn keycap(label: impl Into<SharedString>, theme: &PickerTheme) -> AnyElement {
    div()
        .flex_none()
        .px(theme.keycap_padding_x)
        .py(theme.keycap_padding_y)
        .rounded(theme.keycap_corner_radius)
        .bg(theme.keycap_background)
        .text_color(theme.keycap_text)
        .text_size(theme.detail_font_size)
        .child(label.into())
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_matches_merge_into_one_range() {
        assert_eq!(match_ranges("notes", &[0, 1, 3]), vec![0..2, 3..4]);
        assert_eq!(match_ranges("café", &[3]), vec![3..5]);
        assert_eq!(match_ranges("ab", &[5]), Vec::<Range<usize>>::new());
    }

    #[test]
    fn input_keys_come_from_the_editing_rules() {
        let keys = input_keystrokes(&RuleSet::defaults(), Platform::Macos);
        let bound = |keystroke: &str, id: &str| {
            keys.iter()
                .any(|(candidate, command)| candidate == keystroke && command == id)
        };
        assert!(bound("backspace", "edit.delete-backward"));
        assert!(bound("alt-backspace", "edit.delete-word-backward"));
        assert!(bound("cmd-v", "edit.paste"));
        assert!(!keys.iter().any(|(_, id)| id == "cursor.up"));
        assert!(!keys.iter().any(|(_, id)| id == "edit.newline"));
    }
}
