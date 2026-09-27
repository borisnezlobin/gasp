//! A snippets file: one snippet per line, `#` comments and blank lines.

use std::fmt;

use crate::format::{format_expansion, format_options, format_trigger};
use crate::parse::{ParseError, parse_line};
use crate::snippet::Snippet;

/// Triggers longer than this aren't used to pad the arrow column.
const TRIGGER_COLUMN_LIMIT: usize = 16;
/// Expansions longer than this aren't used to pad the options column.
const EXPANSION_COLUMN_LIMIT: usize = 36;

/// A parsed snippets file that keeps its comments and blank lines.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SnippetFile {
    pub lines: Vec<FileLine>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileLine {
    Snippet(Snippet),
    /// The comment text after `#`, without the `#` or the space after it.
    Comment(String),
    Blank,
}

impl SnippetFile {
    /// Parses a whole file, returning every error rather than only the first.
    pub fn parse(text: &str) -> Result<SnippetFile, Vec<ParseError>> {
        let mut lines = Vec::new();
        let mut errors = Vec::new();
        for (index, line) in text.lines().enumerate() {
            match parse_file_line(line, index + 1) {
                Ok(parsed) => lines.push(parsed),
                Err(error) => errors.push(error),
            }
        }
        if errors.is_empty() {
            Ok(SnippetFile { lines })
        } else {
            Err(errors)
        }
    }

    /// The snippets in file order.
    pub fn snippets(&self) -> impl Iterator<Item = &Snippet> {
        self.lines.iter().filter_map(|line| match line {
            FileLine::Snippet(snippet) => Some(snippet),
            _ => None,
        })
    }

    pub fn push_comment(&mut self, text: impl Into<String>) {
        self.lines.push(FileLine::Comment(text.into()));
    }

    pub fn push_snippet(&mut self, snippet: Snippet) {
        self.lines.push(FileLine::Snippet(snippet));
    }

    pub fn push_blank(&mut self) {
        self.lines.push(FileLine::Blank);
    }
}

fn parse_file_line(line: &str, number: usize) -> Result<FileLine, ParseError> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(FileLine::Blank);
    }
    if let Some(comment) = trimmed.strip_prefix('#') {
        let comment = comment.strip_prefix(' ').unwrap_or(comment);
        return Ok(FileLine::Comment(comment.to_string()));
    }
    parse_line(line, number).map(FileLine::Snippet)
}

struct Columns {
    trigger: usize,
    expansion: usize,
}

struct Row {
    trigger: String,
    expansion: String,
    options: String,
}

impl Row {
    fn new(snippet: &Snippet) -> Row {
        Row {
            trigger: format_trigger(&snippet.trigger),
            expansion: format_expansion(snippet),
            options: format_options(&snippet.options),
        }
    }

    fn write(&self, columns: &Columns, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let trigger_pad = columns.trigger.saturating_sub(self.trigger.chars().count());
        let expansion_pad = columns
            .expansion
            .saturating_sub(self.expansion.chars().count());
        writeln!(
            f,
            "{}{} → {}{}  {}",
            self.trigger,
            " ".repeat(trigger_pad),
            self.expansion,
            " ".repeat(expansion_pad),
            self.options
        )
    }
}

fn column_width(widths: impl Iterator<Item = usize>, limit: usize) -> usize {
    widths.filter(|width| *width <= limit).max().unwrap_or(0)
}

impl fmt::Display for SnippetFile {
    /// Writes the file with the arrow and options columns aligned.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rows: Vec<Option<Row>> = self
            .lines
            .iter()
            .map(|line| match line {
                FileLine::Snippet(snippet) => Some(Row::new(snippet)),
                _ => None,
            })
            .collect();
        let snippet_rows = || rows.iter().flatten();
        let columns = Columns {
            trigger: column_width(
                snippet_rows().map(|row| row.trigger.chars().count()),
                TRIGGER_COLUMN_LIMIT,
            ),
            expansion: column_width(
                snippet_rows().map(|row| row.expansion.chars().count()),
                EXPANSION_COLUMN_LIMIT,
            ),
        };
        for (line, row) in self.lines.iter().zip(&rows) {
            match (line, row) {
                (_, Some(row)) => row.write(&columns, f)?,
                (FileLine::Comment(text), None) if text.is_empty() => writeln!(f, "#")?,
                (FileLine::Comment(text), None) => writeln!(f, "# {text}")?,
                _ => writeln!(f)?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# Greek
@a → \\alpha  math, instant

reals → \\mathbb{R}  math, instant
{letter}{digit} → {letter}_{digit}  math, instant, priority -1
";

    #[test]
    fn keeps_comments_and_blank_lines() {
        let file = SnippetFile::parse(SAMPLE).unwrap();
        assert_eq!(file.lines.len(), 5);
        assert_eq!(file.lines[0], FileLine::Comment("Greek".to_string()));
        assert_eq!(file.lines[2], FileLine::Blank);
        assert_eq!(file.snippets().count(), 3);
    }

    #[test]
    fn written_file_is_aligned_and_round_trips() {
        let file = SnippetFile::parse(SAMPLE).unwrap();
        let written = file.to_string();
        let lines: Vec<&str> = written.lines().collect();
        let arrow_columns: Vec<usize> = lines.iter().filter_map(|line| line.find('→')).collect();
        assert!(arrow_columns.windows(2).all(|pair| pair[0] == pair[1]));
        assert_eq!(SnippetFile::parse(&written).unwrap(), file);
        assert_eq!(SnippetFile::parse(&written).unwrap().to_string(), written);
    }

    #[test]
    fn reports_every_bad_line() {
        let errors = SnippetFile::parse("a → b\nbad\nc → d  nope\n").unwrap_err();
        let lines: Vec<usize> = errors.iter().map(|error| error.line).collect();
        assert_eq!(lines, vec![2, 3]);
    }
}
