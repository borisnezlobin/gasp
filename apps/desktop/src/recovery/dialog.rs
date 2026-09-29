//! The recovery dialog: a note's snapshots down the left, newest first,
//! and on the right how the chosen one differs from the note now. Lines
//! that restoring brings back are tinted green, lines it removes are
//! tinted red and struck through, and long unchanged stretches fold to a
//! line saying how long they are.
//!
//! Up and Down choose a snapshot, Enter restores it and Escape closes.
//! Restoring replaces the note's text as one step that Undo takes back,
//! and the note as it was is kept as a snapshot first.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use super::compare::{DiffRow, Mark, compare};
use super::store::{Snapshot, SnapshotStore};
use crate::picker::shortcut::Shortcut;
use crate::settings_view::controls::{button, inert_button};
use crate::settings_view::modal_size;
use crate::theme::SettingsTheme;
use crate::ui::Selectable;
use crate::ui::keycap;
use gasp_config::keys::KeyChord;
use gasp_config::settings::RecoverySettings;
use gpui::{
    AnyElement, ClickEvent, Context, DismissEvent, EventEmitter, FocusHandle, Focusable,
    HighlightStyle, Hsla, KeyDownEvent, ScrollHandle, SharedString, StrikethroughStyle, StyledText,
    Task, Window, div, prelude::*, relative,
};

/// What the dialog asks of its host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecoveryEvent {
    /// Put this text in the note.
    Restore(String),
}

/// A snapshot read from disk.
#[derive(Clone, Debug)]
pub struct Version {
    pub taken: SystemTime,
    pub text: String,
}

/// Where the dialog is.
enum State {
    Loading,
    /// Snapshots, newest first; empty when the note has none.
    Ready(Vec<Version>),
}

pub struct RecoveryDialog {
    note: PathBuf,
    current: String,
    settings: RecoverySettings,
    state: State,
    selected: usize,
    /// The comparison for the selected snapshot, and its changed lines.
    comparison: Option<(Vec<DiffRow>, usize)>,
    focus_handle: FocusHandle,
    style: SettingsTheme,
    list_scroll: ScrollHandle,
    _load: Task<()>,
}

impl EventEmitter<DismissEvent> for RecoveryDialog {}
impl EventEmitter<RecoveryEvent> for RecoveryDialog {}

impl Focusable for RecoveryDialog {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl RecoveryDialog {
    /// The dialog for `note` (relative to the vault), whose text is
    /// `current`. Snapshots are read in the background.
    pub fn new(
        store: Arc<SnapshotStore>,
        note: PathBuf,
        current: String,
        settings: RecoverySettings,
        cx: &mut Context<Self>,
    ) -> Self {
        let relative = note.clone();
        let read = cx.background_spawn(async move { read_versions(&store, &relative) });
        let load = cx.spawn(async move |this, cx| {
            let versions = read.await;
            this.update(cx, |dialog, cx| dialog.loaded(versions, cx))
                .ok();
        });
        RecoveryDialog {
            note,
            current,
            settings,
            state: State::Loading,
            selected: 0,
            comparison: None,
            focus_handle: cx.focus_handle(),
            style: crate::ui::settings_theme(cx),
            list_scroll: ScrollHandle::new(),
            _load: load,
        }
    }

    /// Snapshots, newest first, once read.
    pub fn versions(&self) -> &[Version] {
        match &self.state {
            State::Ready(versions) => versions,
            State::Loading => &[],
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    fn loaded(&mut self, versions: Vec<Version>, cx: &mut Context<Self>) {
        self.state = State::Ready(versions);
        self.select(0, cx);
    }

    /// Chooses the snapshot at `index` and compares it with the note.
    pub fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(version) = self.versions().get(index) else {
            self.comparison = None;
            return cx.notify();
        };
        self.comparison = Some(compare(&self.current, &version.text));
        self.selected = index;
        self.list_scroll.scroll_to_item(index);
        cx.notify();
    }

    fn step(&mut self, by: isize, cx: &mut Context<Self>) {
        let count = self.versions().len();
        if count > 0 {
            let index = self.selected.saturating_add_signed(by).min(count - 1);
            self.select(index, cx);
        }
    }

    /// Puts the selected snapshot in the note and closes.
    pub fn restore(&mut self, cx: &mut Context<Self>) {
        let Some(version) = self.versions().get(self.selected) else {
            return;
        };
        cx.emit(RecoveryEvent::Restore(version.text.clone()));
        cx.emit(DismissEvent);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if keystroke.modifiers.modified() {
            return;
        }
        let empty = self.versions().is_empty();
        match keystroke.key.as_str() {
            "up" => self.step(-1, cx),
            "down" => self.step(1, cx),
            "enter" if empty => cx.emit(DismissEvent),
            "enter" => self.restore(cx),
            "escape" => cx.emit(DismissEvent),
            _ => return,
        }
        cx.stop_propagation();
    }

    // ---- Drawing ----

    fn render_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let style = &self.style;
        let now = SystemTime::now();
        let rows = self.versions().iter().enumerate().map(|(index, version)| {
            let selected = index == self.selected;
            let selector = format!("recovery-version-{index}");
            div()
                .id(("recovery-version", index))
                .selector(|| selector)
                .flex()
                .flex_col()
                .justify_center()
                .flex_none()
                .h(style.nav_item_height * 1.6)
                .px(style.control_gap)
                .rounded(style.radius)
                .cursor_pointer()
                .when(selected, |row| row.bg(style.selected))
                .when(!selected, |row| row.hover(|row| row.bg(style.hover)))
                .child(div().truncate().child(when_label(version.taken, now)))
                .child(
                    div()
                        .text_size(style.small_text_size)
                        .text_color(style.text_muted)
                        .child(words_label(&version.text)),
                )
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| view.select(index, cx)))
        });
        div()
            .id("recovery-versions")
            .flex_none()
            .w(style.nav_width)
            .h_full()
            .p(style.nav_padding)
            .bg(style.card_background)
            .overflow_y_scroll()
            .track_scroll(&self.list_scroll)
            .flex()
            .flex_col()
            .gap(style.gap_xs)
            .children(rows)
            .into_any_element()
    }

    fn render_header(&self) -> AnyElement {
        let style = &self.style;
        let title = self
            .note
            .file_stem()
            .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
        let intro = match self.versions().get(self.selected) {
            Some(version) => format!(
                "How it read {}, next to how it reads now.",
                sentence_time(version.taken, SystemTime::now())
            ),
            None => String::new(),
        };
        div()
            .flex()
            .flex_col()
            .gap(style.text_gap)
            .child(
                div()
                    .text_size(style.page_title_size)
                    .font_weight(style.strong_weight)
                    .child(title),
            )
            .child(
                div()
                    .text_size(style.small_text_size)
                    .text_color(style.text_muted)
                    .child(intro),
            )
            .into_any_element()
    }

    fn render_comparison(&self, cx: &mut Context<Self>) -> AnyElement {
        let style = &self.style;
        let palette = crate::ui::palette(cx);
        let stroke = crate::ui::ui_theme(cx).hairline;
        let rows = self
            .comparison
            .iter()
            .flat_map(|(rows, _)| rows.iter())
            .enumerate()
            .map(|(index, row)| {
                diff_row(
                    index,
                    row,
                    style,
                    palette.diff_added,
                    palette.diff_removed,
                    stroke,
                )
            });
        div()
            .id("recovery-comparison")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(style.card_padding_x * 0.75)
            .rounded(style.radius)
            .bg(style.card_background)
            .flex()
            .flex_col()
            .children(rows)
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let style = &self.style;
        let changed = self.comparison.as_ref().map_or(0, |(_, changed)| *changed);
        let summary = match changed {
            0 => "It’s the same as the note now.".to_owned(),
            1 => "One line differs from the note now.".to_owned(),
            n => format!("{n} lines differ from the note now."),
        };
        let keys = crate::ui::ui_theme(cx).keycap.compact();
        let restore = if changed > 0 {
            button(
                "recovery-restore",
                "Restore this version",
                true,
                false,
                style,
            )
            .gap(style.gap_sm)
            .child(key_chip("Enter", &keys.on_text(style.on_accent)))
            .selector(|| "recovery-restore".to_owned())
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| view.restore(cx)))
        } else {
            inert_button("recovery-restore", "Restore this version", style)
                .selector(|| "recovery-restore".to_owned())
        };
        let cancel = button("recovery-cancel", "Not now", false, false, style)
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(DismissEvent)));
        div()
            .flex()
            .items_center()
            .gap(style.control_gap)
            .text_size(style.small_text_size)
            .child(div().flex_1().text_color(style.text_muted).child(summary))
            .child(cancel)
            .child(restore)
            .into_any_element()
    }

    /// No snapshots yet: a small card saying what's kept, and how often.
    fn render_empty(&self, cx: &mut Context<Self>) -> AnyElement {
        let style = &self.style;
        let every = match self.settings.interval_minutes {
            1 => "every minute".to_owned(),
            n => format!("every {n} minutes"),
        };
        let kept = match self.settings.keep_days {
            1 => "a day".to_owned(),
            n => format!("{n} days"),
        };
        let close = button("recovery-close", "OK", true, false, style)
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(DismissEvent)));
        div()
            .w(style.empty_dialog_width)
            .flex()
            .flex_col()
            .gap(style.card_gap)
            .px(style.content_padding_x)
            .py(style.content_padding_y)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(style.text_gap)
                    .child(
                        div()
                            .text_size(style.page_title_size)
                            .font_weight(style.strong_weight)
                            .child("No earlier versions yet"),
                    )
                    .child(div().text_color(style.text_muted).child(format!(
                        "While you edit this note, a copy of it is kept {every}, for {kept}. \
                         Come back here to bring one back."
                    ))),
            )
            .child(div().flex().justify_end().child(close))
            .into_any_element()
    }
}

/// One line of the comparison. A changed line shows what restoring
/// brings back on a green tint and what it removes struck through on a
/// red one.
fn diff_row(
    index: usize,
    row: &DiffRow,
    style: &SettingsTheme,
    added: Hsla,
    removed: Hsla,
    stroke: gpui::Pixels,
) -> AnyElement {
    let line = div()
        .id(("recovery-line", index))
        .px(style.gap_sm)
        .rounded(style.radius / 2.);
    match row {
        // An empty line keeps its height.
        DiffRow::Same(text) if text.is_empty() => line.child(" ").into_any_element(),
        // Unchanged lines are context: demoted, so the change leads.
        DiffRow::Same(text) => line
            .text_color(style.text_muted)
            .child(text.clone())
            .into_any_element(),
        DiffRow::Changed(stretches) => {
            let marks = MarkStyles {
                added,
                removed,
                struck: style.text_muted,
                stroke,
            };
            line.child(marked_text(stretches, &marks))
                .into_any_element()
        }
        DiffRow::Folded(count) => div()
            .px(style.gap_sm)
            .py(style.gap_xs)
            .text_size(style.small_text_size)
            .text_color(style.text_faint)
            .child(match count {
                1 => "One line is the same".to_owned(),
                n => format!("{n} lines are the same"),
            })
            .into_any_element(),
    }
}

/// How marked stretches look.
struct MarkStyles {
    added: Hsla,
    removed: Hsla,
    struck: Hsla,
    /// The strike-through line's thickness.
    stroke: gpui::Pixels,
}

/// A changed line as one run of text, its stretches highlighted.
fn marked_text(stretches: &[(Mark, String)], styles: &MarkStyles) -> StyledText {
    let mut text = String::new();
    let mut highlights = Vec::new();
    for (mark, stretch) in stretches {
        let start = text.len();
        text.push_str(stretch);
        let style = match mark {
            Mark::Same => continue,
            Mark::Back => HighlightStyle {
                background_color: Some(styles.added),
                ..HighlightStyle::default()
            },
            Mark::Gone => HighlightStyle {
                background_color: Some(styles.removed),
                color: Some(styles.struck),
                strikethrough: Some(StrikethroughStyle {
                    thickness: styles.stroke,
                    color: Some(styles.struck),
                }),
                ..HighlightStyle::default()
            },
        };
        highlights.push((start..text.len(), style));
    }
    if text.is_empty() {
        text.push(' ');
    }
    StyledText::new(text).with_highlights(highlights)
}

/// The key that does something, on the button that does the same.
fn key_chip(key: &str, theme: &crate::theme::KeycapTheme) -> impl IntoElement {
    let chord = KeyChord::parse(key).expect("dialog keys parse");
    keycap(
        Shortcut::new(chord, gasp_config::Platform::current()),
        theme,
    )
}

/// Every snapshot of `note`, newest first. Unreadable ones are skipped.
fn read_versions(store: &SnapshotStore, note: &std::path::Path) -> Vec<Version> {
    store
        .list(note)
        .into_iter()
        .filter_map(|snapshot: Snapshot| {
            let text = store.read(&snapshot).ok()?;
            Some(Version {
                taken: snapshot.taken,
                text,
            })
        })
        .collect()
}

fn words_label(text: &str) -> String {
    match gasp_prose::word_count(text) {
        1 => "1 word".to_owned(),
        n => format!("{} words", thousands(n)),
    }
}

/// 1204 as "1,204".
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// When a snapshot was taken, in the local time zone: "Today, 14:05",
/// "Yesterday, 09:12" or "Sat 21 Sep, 16:40".
pub fn when_label(taken: SystemTime, now: SystemTime) -> SharedString {
    let (Some(taken), Some(now)) = (local(taken), local(now)) else {
        return "Some time ago".into();
    };
    let time = taken.strftime("%H:%M");
    let days = (now.date() - taken.date()).get_days();
    match days {
        0 => format!("Today, {time}"),
        1 => format!("Yesterday, {time}"),
        _ => format!("{}, {time}", taken.strftime("%a %-d %b")),
    }
    .into()
}

/// The same moment inside a sentence: "at 14:05 today".
fn sentence_time(taken: SystemTime, now: SystemTime) -> String {
    let (Some(taken_local), Some(now_local)) = (local(taken), local(now)) else {
        return "then".to_owned();
    };
    let time = taken_local.strftime("%H:%M");
    match (now_local.date() - taken_local.date()).get_days() {
        0 => format!("at {time} today"),
        1 => format!("at {time} yesterday"),
        _ => format!("at {time} on {}", taken_local.strftime("%A %-d %B")),
    }
}

fn local(time: SystemTime) -> Option<jiff::Zoned> {
    let timestamp = jiff::Timestamp::try_from(time).ok()?;
    Some(timestamp.to_zoned(jiff::tz::TimeZone::system()))
}

impl Render for RecoveryDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The theme can change while the dialog is open.
        self.style = crate::ui::settings_theme(cx);
        let style = self.style.clone();
        let size = modal_size(window.viewport_size(), &style);
        let root = div()
            .id("recovery-dialog")
            .key_context("RecoveryDialog")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .flex()
            .overflow_hidden()
            .rounded(style.modal_radius)
            .bg(style.background)
            .shadow(vec![style.outline(), style.popover_shadow()])
            .font_family(style.font_family.clone())
            .text_size(style.text_size)
            .line_height(relative(style.line_height_factor))
            .text_color(style.text);
        match &self.state {
            // Reading takes a moment; nothing shows rather than a flash.
            State::Loading => root,
            State::Ready(versions) if versions.is_empty() => root.child(self.render_empty(cx)),
            State::Ready(_) => root
                .w(size.width)
                .h(size.height)
                .child(self.render_list(cx))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(style.card_gap)
                        .px(style.content_padding_x)
                        .py(style.content_padding_y)
                        .child(self.render_header())
                        .child(self.render_comparison(cx))
                        .child(self.render_footer(cx)),
                ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_read_with_separators() {
        assert_eq!(thousands(7), "7");
        assert_eq!(thousands(1204), "1,204");
        assert_eq!(thousands(1_000_000), "1,000,000");
        assert_eq!(words_label("One"), "1 word");
    }

    #[test]
    fn times_read_relative_to_today() {
        let now = SystemTime::now();
        let label = when_label(now, now);
        assert!(label.starts_with("Today, "), "{label}");
        let yesterday = now - std::time::Duration::from_secs(24 * 60 * 60);
        assert!(when_label(yesterday, now).starts_with("Yesterday, "));
    }
}
