//! The status bar: word and character counts, reading time, cursor
//! position and the sync indicator's slot.

use gpui::{IntoElement, ParentElement, SharedString, Styled, div, prelude::*};

use crate::editor::EditorView;
use crate::icons::{IconName, icon};
use crate::theme::UiTheme;

/// Average adult silent-reading speed.
pub const WORDS_PER_MINUTE: usize = 238;

/// Counts for the text the status bar describes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextStats {
    pub words: usize,
    /// Characters, not counting line breaks.
    pub characters: usize,
}

impl TextStats {
    pub fn of(text: &str) -> TextStats {
        TextStats {
            words: text
                .split_whitespace()
                .filter(|word| word.chars().any(char::is_alphanumeric))
                .count(),
            characters: text.chars().filter(|ch| !matches!(ch, '\n' | '\r')).count(),
        }
    }

    /// Whole minutes to read, rounded up, and zero for an empty note.
    pub fn reading_minutes(&self) -> usize {
        self.words.div_ceil(WORDS_PER_MINUTE)
    }
}

/// What the status bar shows for the active note.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StatusInfo {
    pub stats: TextStats,
    /// Whether the counts are for the selection rather than the note.
    pub for_selection: bool,
    /// One-based line and column of the cursor.
    pub line: usize,
    pub column: usize,
}

impl StatusInfo {
    pub fn of_editor(editor: &EditorView) -> StatusInfo {
        let selection = editor.selected_range();
        let doc = editor.doc();
        let (stats, for_selection) = if selection.is_empty() {
            (TextStats::of(&editor.text()), false)
        } else {
            (TextStats::of(&doc.slice(selection)), true)
        };
        let cursor = editor.cursor();
        let line = doc.line_of_offset(cursor);
        let column = doc.slice(doc.line_start(line)..cursor).chars().count();
        StatusInfo {
            stats,
            for_selection,
            line: line + 1,
            column: column + 1,
        }
    }

    pub fn words_label(&self) -> String {
        let noun = if self.stats.words == 1 {
            "word"
        } else {
            "words"
        };
        let label = format!("{} {noun}", group_thousands(self.stats.words));
        if self.for_selection {
            format!("{label} selected")
        } else {
            label
        }
    }

    pub fn characters_label(&self) -> String {
        let noun = if self.stats.characters == 1 {
            "character"
        } else {
            "characters"
        };
        format!("{} {noun}", group_thousands(self.stats.characters))
    }

    pub fn reading_label(&self) -> String {
        match self.stats.reading_minutes() {
            0 => "No reading time".to_owned(),
            minutes => format!("{minutes} min read"),
        }
    }

    pub fn position_label(&self) -> String {
        format!("{}:{}", self.line, self.column)
    }
}

/// `12345` as `12,345`.
pub fn group_thousands(number: usize) -> String {
    let digits = number.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Draws the status bar: small muted counts at the bottom right. `info`
/// is `None` when no note is open.
pub fn render_status_bar(info: Option<&StatusInfo>, theme: &UiTheme) -> impl IntoElement {
    let item = |text: String| -> gpui::Div { div().child(SharedString::from(text)) };
    let mut bar = div()
        .id("status-bar")
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .justify_end()
        .gap(theme.status_gap)
        .h(theme.status_height)
        .px(theme.space_lg)
        .text_size(theme.small_font_size)
        .text_color(theme.text_faint);
    if let Some(info) = info {
        bar = bar
            .child(item(info.words_label()))
            .child(item(info.characters_label()))
            .child(item(info.reading_label()))
            .child(item(info.position_label()));
    }
    bar.child(
        // The sync indicator's slot. Sync isn't wired yet, so it shows the
        // idle cloud.
        div().id("sync-indicator").child(
            icon(IconName::CloudCheck)
                .size(theme.small_icon_size)
                .text_color(theme.text_faint),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_words_and_characters() {
        let stats = TextStats::of("# Title\n\nSome *bold* words - here.\n");
        assert_eq!(stats.words, 5);
        assert_eq!(stats.characters, 32);
        assert_eq!(TextStats::of("").words, 0);
    }

    #[test]
    fn reading_time_rounds_up() {
        let stats = |words| TextStats {
            words,
            characters: 0,
        };
        assert_eq!(stats(0).reading_minutes(), 0);
        assert_eq!(stats(1).reading_minutes(), 1);
        assert_eq!(stats(238).reading_minutes(), 1);
        assert_eq!(stats(239).reading_minutes(), 2);
    }

    #[test]
    fn numbers_group_by_thousands() {
        assert_eq!(group_thousands(7), "7");
        assert_eq!(group_thousands(1234), "1,234");
        assert_eq!(group_thousands(1234567), "1,234,567");
    }
}
