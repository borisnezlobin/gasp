//! Structured problems found while loading config files.

use std::fmt;
use std::ops::Range;

/// How serious a diagnostic is. Errors make the whole file fall back; warnings don't.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// One problem in one config file. Lines and columns are 1-based; column counts characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// The file name relative to the config folder, such as `rules.toml`.
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub severity: Severity,
}

impl Diagnostic {
    pub fn error(file: &str, text: &str, span: Option<Range<usize>>, message: String) -> Self {
        Self::at(file, text, span, message, Severity::Error)
    }

    pub fn warning(file: &str, text: &str, span: Option<Range<usize>>, message: String) -> Self {
        Self::at(file, text, span, message, Severity::Warning)
    }

    fn at(
        file: &str,
        text: &str,
        span: Option<Range<usize>>,
        message: String,
        severity: Severity,
    ) -> Self {
        let (line, column) = span.map_or((1, 1), |span| line_and_column(text, span.start));
        Diagnostic {
            file: file.to_string(),
            line,
            column,
            message,
            severity,
        }
    }

    /// Converts a TOML parse or type error, keeping its position.
    pub fn from_toml(file: &str, text: &str, error: &toml::de::Error) -> Self {
        Self::error(file, text, error.span(), error.message().trim().to_string())
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}",
            self.file, self.line, self.column, self.message
        )
    }
}

/// Converts a byte offset into a 1-based line and character column.
pub fn line_and_column(text: &str, offset: usize) -> (usize, usize) {
    let offset = floor_char_boundary(text, offset.min(text.len()));
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let column = before[line_start..].chars().count() + 1;
    (line, column)
}

fn floor_char_boundary(text: &str, mut offset: usize) -> usize {
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// Best-effort span of the first line that assigns `key`, for errors found after parsing.
pub fn span_of_key(text: &str, key: &str) -> Option<Range<usize>> {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if assigns_key(trimmed, key) {
            return Some(offset + indent..offset + indent + key.len());
        }
        offset += line.len();
    }
    None
}

fn assigns_key(line: &str, key: &str) -> bool {
    let Some(rest) = line
        .strip_prefix(key)
        .or_else(|| line.strip_prefix(&format!("\"{key}\"")))
    else {
        return false;
    };
    rest.trim_start().starts_with('=')
}

/// True when any diagnostic is an error.
pub fn has_errors(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(Diagnostic::is_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_and_column_are_one_based() {
        let text = "a = 1\nbb = 2\n";
        assert_eq!(line_and_column(text, 0), (1, 1));
        assert_eq!(line_and_column(text, 6), (2, 1));
        assert_eq!(line_and_column(text, 9), (2, 4));
    }

    #[test]
    fn columns_count_characters_not_bytes() {
        let text = "é = 1";
        assert_eq!(line_and_column(text, 2), (1, 2));
    }

    #[test]
    fn toml_errors_carry_position() {
        let text = "a = 1\nb = = 2\n";
        let error = toml::from_str::<toml::Table>(text).unwrap_err();
        let diagnostic = Diagnostic::from_toml("settings.toml", text, &error);
        assert_eq!(diagnostic.file, "settings.toml");
        assert_eq!(diagnostic.line, 2);
        assert!(diagnostic.column > 1);
        assert!(!diagnostic.message.is_empty());
    }

    #[test]
    fn finds_the_line_of_a_key() {
        let text = "[color]\nblack = \"#000\"\n  accent = \"{color.black}\"\n";
        let span = span_of_key(text, "accent").unwrap();
        assert_eq!(line_and_column(text, span.start), (3, 3));
        assert!(span_of_key(text, "missing").is_none());
    }
}
