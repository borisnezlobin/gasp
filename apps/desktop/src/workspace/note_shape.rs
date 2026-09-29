//! A note's shape: its opening lines as the bars a new tab draws on the
//! note's card, so notes tell apart by their layout before their titles
//! are read. A heading is a dark bar, a paragraph wraps into grey bars, a
//! list item has a dot, a task a box (filled when done), a quote a rule,
//! code a shaded block and a picture a frame.

use std::io::Read;
use std::path::Path;

/// How many characters fit on one bar of a card.
const CHARACTERS_PER_LINE: usize = 44;
/// The most of a note's start that's read to find its shape.
const READ_BYTES: u64 = 4096;

/// One line of a note's card.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShapeLine {
    /// A heading, and how much of the line it takes.
    Heading(f32),
    Text(f32),
    Item(f32),
    Task {
        done: bool,
        share: f32,
    },
    Quote(f32),
    Code(f32),
    Picture,
}

/// The first `lines` lines of the card for `text`.
pub fn shape_of(text: &str, lines: usize) -> Vec<ShapeLine> {
    let mut shape = Vec::new();
    let mut in_code = false;
    for line in without_frontmatter(text).lines() {
        if shape.len() >= lines {
            break;
        }
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            shape.push(ShapeLine::Code(share(trimmed.len())));
            continue;
        }
        shape.extend(shape_of_line(trimmed));
    }
    shape.truncate(lines);
    shape
}

fn shape_of_line(line: &str) -> Vec<ShapeLine> {
    if line.is_empty() {
        return Vec::new();
    }
    if let Some(rest) = line.strip_prefix('#') {
        return vec![ShapeLine::Heading(share(
            rest.trim_start_matches('#').trim().len(),
        ))];
    }
    if line.starts_with("![") {
        return vec![ShapeLine::Picture];
    }
    if let Some(done) = task_state(line) {
        return vec![ShapeLine::Task {
            done,
            share: share(line.len().saturating_sub(6)),
        }];
    }
    if is_list_item(line) {
        return vec![ShapeLine::Item(share(line.len()))];
    }
    if let Some(rest) = line.strip_prefix('>') {
        return vec![ShapeLine::Quote(share(rest.trim().len()))];
    }
    wrapped(line.chars().count())
}

/// A paragraph of `length` characters as the bars it wraps into.
fn wrapped(length: usize) -> Vec<ShapeLine> {
    let full = length / CHARACTERS_PER_LINE;
    let rest = length % CHARACTERS_PER_LINE;
    let mut bars = vec![ShapeLine::Text(1.); full];
    if rest > 0 {
        bars.push(ShapeLine::Text(share(rest)));
    }
    bars
}

fn share(characters: usize) -> f32 {
    (characters as f32 / CHARACTERS_PER_LINE as f32).clamp(0.15, 1.)
}

fn task_state(line: &str) -> Option<bool> {
    let rest = line
        .strip_prefix("- [")
        .or_else(|| line.strip_prefix("* ["))?;
    let mark = rest.chars().next()?;
    rest[mark.len_utf8()..]
        .starts_with(']')
        .then_some(mark != ' ')
}

fn is_list_item(line: &str) -> bool {
    if line.starts_with("- ") || line.starts_with("* ") || line.starts_with("+ ") {
        return true;
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    digits > 0 && line[digits..].starts_with(". ")
}

fn without_frontmatter(text: &str) -> &str {
    let Some(rest) = text.strip_prefix("---\n") else {
        return text;
    };
    rest.find("\n---")
        .and_then(|end| rest.get(end + 4..))
        .unwrap_or(text)
}

/// Reads the start of the note at `path` and answers its shape; empty
/// when it can't be read.
pub fn read_shape(path: &Path, lines: usize) -> Vec<ShapeLine> {
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut start = Vec::new();
    if file.take(READ_BYTES).read_to_end(&mut start).is_err() {
        return Vec::new();
    }
    shape_of(&String::from_utf8_lossy(&start), lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(line: &ShapeLine) -> &'static str {
        match line {
            ShapeLine::Heading(_) => "heading",
            ShapeLine::Text(_) => "text",
            ShapeLine::Item(_) => "item",
            ShapeLine::Task { done: true, .. } => "done",
            ShapeLine::Task { done: false, .. } => "task",
            ShapeLine::Quote(_) => "quote",
            ShapeLine::Code(_) => "code",
            ShapeLine::Picture => "picture",
        }
    }

    #[test]
    fn each_kind_of_line_has_its_own_shape() {
        let text = "---\ntags: [a]\n---\n# Title\n\nA short line.\n- item\n- [x] done\n- [ ] to do\n> said\n![[cat.png]]\n```\ncode\n```\n- [ ]\n";
        let kinds: Vec<&str> = shape_of(text, 20).iter().map(kind).collect();
        assert_eq!(
            kinds,
            ["heading", "text", "item", "done", "task", "quote", "picture", "code", "task"]
        );
    }

    #[test]
    fn a_long_paragraph_wraps_and_the_card_stops_when_full() {
        let paragraph = "word ".repeat(40);
        let shape = shape_of(&paragraph, 3);
        assert_eq!(shape, vec![ShapeLine::Text(1.); 3]);
    }
}
