//! A simple `$`-scanner that pulls math out of Markdown for measurements and
//! tests. The editor's real parser lives in the core crate; this only needs
//! to be good enough to collect equations from a vault.

/// One equation found in Markdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MathSnippet {
    /// The LaTeX between the delimiters.
    pub source: String,
    /// `true` for `$$…$$`, `false` for `$…$`.
    pub display: bool,
}

/// Finds `$…$` and `$$…$$` math, skipping code fences, code spans and
/// escaped dollars. Display math may span lines; inline math may not.
pub fn find_math(markdown: &str) -> Vec<MathSnippet> {
    let mut scanner = Scanner::default();
    for line in markdown.lines() {
        scanner.line(line);
    }
    scanner.found
}

#[derive(Default)]
struct Scanner {
    found: Vec<MathSnippet>,
    in_fence: bool,
    open_display: Option<String>,
}

impl Scanner {
    fn line(&mut self, line: &str) {
        if self.open_display.is_none() && is_fence(line) {
            self.in_fence = !self.in_fence;
            return;
        }
        if self.in_fence {
            return;
        }
        let mut rest = line;
        while !rest.is_empty() {
            rest = match self.open_display.take() {
                Some(open) => self.continue_display(open, rest),
                None => self.scan_text(rest),
            };
        }
        if let Some(open) = self.open_display.as_mut() {
            open.push('\n');
        }
    }

    /// Continues a display equation; returns what follows its closing `$$`.
    fn continue_display<'a>(&mut self, mut open: String, text: &'a str) -> &'a str {
        match find_unescaped(text, "$$") {
            Some(end) => {
                open.push_str(&text[..end]);
                self.found.push(MathSnippet {
                    source: open.trim().to_owned(),
                    display: true,
                });
                &text[end + 2..]
            }
            None => {
                open.push_str(text);
                self.open_display = Some(open);
                ""
            }
        }
    }

    /// Scans ordinary text up to and including the next equation.
    fn scan_text<'a>(&mut self, text: &'a str) -> &'a str {
        let bytes = text.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'\\' => index += 2,
                b'`' => index = skip_code_span(text, index),
                b'$' if bytes.get(index + 1) == Some(&b'$') => {
                    self.open_display = Some(String::new());
                    return &text[index + 2..];
                }
                b'$' => match inline_end(text, index + 1) {
                    Some(end) => {
                        self.found.push(MathSnippet {
                            source: text[index + 1..end].to_owned(),
                            display: false,
                        });
                        return &text[end + 1..];
                    }
                    None => index += 1,
                },
                _ => index += 1,
            }
        }
        ""
    }
}

fn is_fence(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

/// Finds `needle` in `text`, ignoring occurrences right after a backslash.
fn find_unescaped(text: &str, needle: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index += 2;
            continue;
        }
        if text[index..].starts_with(needle) {
            return Some(index);
        }
        index += 1;
    }
    None
}

/// The closing `$` of inline math starting at `start`. Like Obsidian, the
/// content must not start or end with whitespace.
fn inline_end(text: &str, start: usize) -> Option<usize> {
    let content = text.get(start..)?;
    if content.starts_with(char::is_whitespace) || content.starts_with('$') {
        return None;
    }
    let end = start + find_unescaped(content, "$")?;
    let ends_with_space = text[..end].ends_with(char::is_whitespace);
    (end > start && !ends_with_space).then_some(end)
}

/// Returns the index just past the code span starting at `start`, or just
/// past the backticks when the span is never closed.
fn skip_code_span(text: &str, start: usize) -> usize {
    let ticks = text[start..]
        .bytes()
        .take_while(|&byte| byte == b'`')
        .count();
    let fence = &text[start..start + ticks];
    let after = start + ticks;
    match text[after..].find(fence) {
        Some(offset) => after + offset + ticks,
        None => after,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sources(markdown: &str) -> Vec<(String, bool)> {
        find_math(markdown)
            .into_iter()
            .map(|snippet| (snippet.source, snippet.display))
            .collect()
    }

    #[test]
    fn finds_inline_and_display() {
        let found = sources("Let $x^2$ be.\n$$\n\\int f\n$$\nand $y$.");
        assert_eq!(
            found,
            vec![
                ("x^2".to_owned(), false),
                ("\\int f".to_owned(), true),
                ("y".to_owned(), false),
            ]
        );
    }

    #[test]
    fn display_on_one_line() {
        assert_eq!(sources("$$a+b$$"), vec![("a+b".to_owned(), true)]);
    }

    #[test]
    fn skips_code_and_escapes() {
        let markdown = "costs \\$5 and `$x$` here\n```\n$y$\n```\n$z$";
        assert_eq!(sources(markdown), vec![("z".to_owned(), false)]);
    }

    #[test]
    fn escaped_dollar_inside_math() {
        assert_eq!(sources(r"$\$5$"), vec![(r"\$5".to_owned(), false)]);
    }

    #[test]
    fn prices_are_not_math() {
        assert!(sources("between $5 and $ 10").is_empty());
    }
}
