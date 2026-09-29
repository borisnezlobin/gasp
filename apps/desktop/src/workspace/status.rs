//! What the status bar's widgets say: word and character counts, reading
//! time, time spent editing and the cursor's position. The status bar
//! itself is the built-in `status` toolbar, drawn by [`crate::toolbar`].

use gasp_config::toolbars::Widget;

use crate::editor::EditorView;

/// Average adult silent-reading speed.
pub const WORDS_PER_MINUTE: usize = 238;

/// The status widgets that are text, in the order the status bar has
/// always shown them.
const TEXT_WIDGETS: [Widget; 5] = [
    Widget::WordCount,
    Widget::CharacterCount,
    Widget::ReadingTime,
    Widget::EditTime,
    Widget::CursorPosition,
];

/// Counts for the text the status bar describes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextStats {
    pub words: usize,
    /// Characters, not counting line breaks.
    pub characters: usize,
}

impl TextStats {
    pub fn of(text: &str) -> TextStats {
        TextStats::of_chars(text.chars())
    }

    /// Counts in one pass without copying the text, so the status bar can
    /// read a long note's rope on every keystroke. A word is a run of
    /// non-space characters with at least one letter or digit in it.
    pub fn of_chars(chars: impl Iterator<Item = char>) -> TextStats {
        let mut stats = TextStats::default();
        let mut word_has_text = false;
        for ch in chars {
            if !matches!(ch, '\n' | '\r') {
                stats.characters += 1;
            }
            if ch.is_whitespace() {
                stats.words += usize::from(word_has_text);
                word_has_text = false;
            } else {
                word_has_text |= ch.is_alphanumeric();
            }
        }
        stats.words += usize::from(word_has_text);
        stats
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
    /// Seconds spent editing the note, on every device.
    pub edited_seconds: u64,
}

impl StatusInfo {
    pub fn of_editor(editor: &EditorView) -> StatusInfo {
        let selection = editor.selected_range();
        let doc = editor.doc();
        let for_selection = !selection.is_empty();
        let range = if for_selection {
            selection
        } else {
            0..doc.len()
        };
        let stats = TextStats::of_chars(doc.rope().byte_slice(range).chars());
        let cursor = editor.cursor();
        let line = doc.line_of_offset(cursor);
        let column = doc
            .rope()
            .byte_slice(doc.line_start(line)..cursor)
            .len_chars();
        StatusInfo {
            stats,
            for_selection,
            line: line + 1,
            column: column + 1,
            edited_seconds: 0,
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

    /// What the bar shows, leaving out counts with nothing to say: a
    /// selection is one phrase ("3 words, 18 characters selected"), and a
    /// note too short to read takes no reading time.
    pub fn items(&self) -> Vec<String> {
        TEXT_WIDGETS
            .iter()
            .filter_map(|widget| self.widget_text(*widget))
            .collect()
    }

    /// What a status widget on a toolbar says now, or `None` when it has
    /// nothing to say: with a selection the word count says the whole
    /// phrase and the counts after it step aside.
    pub fn widget_text(&self, widget: Widget) -> Option<String> {
        let selecting = self.for_selection;
        match widget {
            Widget::WordCount if selecting => Some(self.selection_label()),
            Widget::WordCount => Some(self.words_label()),
            Widget::CursorPosition => Some(self.position_label()),
            _ if selecting => None,
            Widget::CharacterCount => Some(self.characters_label()),
            Widget::ReadingTime => (self.stats.reading_minutes() > 0).then(|| self.reading_label()),
            Widget::EditTime => crate::edit_time::edit_time_label(self.edited_seconds),
            Widget::Sync => None,
        }
    }

    fn selection_label(&self) -> String {
        if self.stats.words == 0 {
            return format!("{} selected", self.characters_label());
        }
        let noun = if self.stats.words == 1 {
            "word"
        } else {
            "words"
        };
        let words = format!("{} {noun}", group_thousands(self.stats.words));
        format!("{words}, {} selected", self.characters_label())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn info(words: usize, characters: usize, for_selection: bool) -> StatusInfo {
        StatusInfo {
            stats: TextStats { words, characters },
            for_selection,
            line: 3,
            column: 7,
            edited_seconds: 0,
        }
    }

    #[test]
    fn a_selection_reads_as_one_phrase() {
        assert_eq!(info(0, 1, true).items(), ["1 character selected", "3:7"]);
        assert_eq!(
            info(3, 18, true).items(),
            ["3 words, 18 characters selected", "3:7"]
        );
        assert_eq!(
            info(0, 0, false).items(),
            ["0 words", "0 characters", "3:7"]
        );
        assert_eq!(info(300, 1500, false).items()[2], "2 min read");
        let edited = StatusInfo {
            edited_seconds: 12 * 60,
            ..info(300, 1500, false)
        };
        assert_eq!(edited.items()[3], "12 min editing");
        let selected = StatusInfo {
            edited_seconds: 12 * 60,
            ..info(3, 18, true)
        };
        assert_eq!(
            selected.items().len(),
            2,
            "a selection is about the selection"
        );
    }

    #[test]
    fn counts_words_and_characters() {
        let stats = TextStats::of("# Title\n\nSome *bold* words - here.\n");
        assert_eq!(stats.words, 5);
        assert_eq!(stats.characters, 32);
        assert_eq!(TextStats::of("").words, 0);
    }

    #[test]
    fn counting_chars_matches_splitting_the_string() {
        for text in ["", "one", "  two  words ", "a\r\nb - c\n", "émigré café 12"] {
            let words = text
                .split_whitespace()
                .filter(|word| word.chars().any(char::is_alphanumeric))
                .count();
            assert_eq!(TextStats::of(text).words, words, "{text:?}");
        }
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
