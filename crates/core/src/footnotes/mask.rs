//! Blank out code and math so footnote-like text inside them is ignored.
//! The masked text has the same byte length as the original, with newlines
//! kept, so offsets line up.

use std::ops::Range;

/// Returns `text` with fenced code, code spans and math replaced by spaces.
pub(crate) fn mask_code_and_math(text: &str) -> String {
    let mut bytes = text.as_bytes().to_vec();
    mask_fences(&mut bytes, text);
    mask_inline(&mut bytes);
    // Only whole ASCII-delimited regions are blanked, so this stays valid UTF-8.
    String::from_utf8(bytes).unwrap_or_else(|err| String::from_utf8_lossy(err.as_bytes()).into())
}

/// Byte ranges of each line's content, excluding the `\n` (but not a `\r`).
pub(crate) fn line_ranges(text: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            ranges.push(start..index);
            start = index + 1;
        }
    }
    ranges.push(start..text.len());
    ranges
}

/// Strips leading whitespace and blockquote/callout markers.
pub(crate) fn strip_line_prefix(line: &[u8]) -> &[u8] {
    let skip = line
        .iter()
        .take_while(|&&b| matches!(b, b' ' | b'\t' | b'>'))
        .count();
    &line[skip..]
}

fn blank(bytes: &mut [u8], range: Range<usize>) {
    for byte in &mut bytes[range] {
        if *byte != b'\n' {
            *byte = b' ';
        }
    }
}

fn fence_marker(line: &[u8]) -> Option<u8> {
    let rest = strip_line_prefix(line);
    let first = *rest.first()?;
    let is_fence = matches!(first, b'`' | b'~') && rest.iter().take(3).all(|&b| b == first);
    (is_fence && rest.len() >= 3).then_some(first)
}

fn mask_fences(bytes: &mut [u8], text: &str) {
    let mut open: Option<u8> = None;
    for range in line_ranges(text) {
        let marker = fence_marker(&bytes[range.clone()]);
        let inside = match (open, marker) {
            (None, Some(fence)) => {
                open = Some(fence);
                true
            }
            (Some(fence), Some(other)) if fence == other => {
                open = None;
                true
            }
            (Some(_), _) => true,
            (None, None) => false,
        };
        if inside {
            blank(bytes, range);
        }
    }
}

fn mask_inline(bytes: &mut [u8]) {
    let mut index = 0;
    while index < bytes.len() {
        index = match bytes[index] {
            b'\\' => index + 2,
            b'`' => mask_code_span(bytes, index),
            b'$' => mask_math(bytes, index),
            _ => index + 1,
        };
    }
}

fn run_length(bytes: &[u8], at: usize, byte: u8) -> usize {
    bytes[at..].iter().take_while(|&&b| b == byte).count()
}

/// Masks a code span opened by the backtick run at `start`; returns where to
/// continue scanning.
fn mask_code_span(bytes: &mut [u8], start: usize) -> usize {
    let run = run_length(bytes, start, b'`');
    let mut index = start + run;
    while index < bytes.len() && bytes[index] != b'\n' {
        if bytes[index] != b'`' {
            index += 1;
            continue;
        }
        let close = run_length(bytes, index, b'`');
        if close == run {
            blank(bytes, start..index + close);
            return index + close;
        }
        index += close;
    }
    start + run
}

fn mask_math(bytes: &mut [u8], start: usize) -> usize {
    if bytes.get(start + 1) == Some(&b'$') {
        return mask_display_math(bytes, start);
    }
    mask_inline_math(bytes, start)
}

fn mask_display_math(bytes: &mut [u8], start: usize) -> usize {
    let body = start + 2;
    let close = bytes[body..].windows(2).position(|pair| pair == b"$$");
    match close {
        Some(offset) => {
            let end = body + offset + 2;
            blank(bytes, start..end);
            end
        }
        None => body,
    }
}

/// `$x$` math: no whitespace just inside the dollars, and on one line.
fn mask_inline_math(bytes: &mut [u8], start: usize) -> usize {
    let opens = bytes
        .get(start + 1)
        .is_some_and(|b| !b.is_ascii_whitespace() && *b != b'$');
    if !opens {
        return start + 1;
    }
    let mut index = start + 2;
    while index < bytes.len() && bytes[index] != b'\n' {
        let byte = bytes[index];
        if byte == b'\\' {
            index += 2;
            continue;
        }
        if byte == b'$' && !bytes[index - 1].is_ascii_whitespace() {
            blank(bytes, start..index + 1);
            return index + 1;
        }
        index += 1;
    }
    start + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_fences_code_spans_and_math() {
        let text = "a `[^1]` b\n```\n[^2]\n```\n$[^3]$ and $$\n[^4]\n$$ [^5]";
        let masked = mask_code_and_math(text);
        assert_eq!(masked.len(), text.len());
        assert!(!masked.contains("[^1]"));
        assert!(!masked.contains("[^2]"));
        assert!(!masked.contains("[^3]"));
        assert!(!masked.contains("[^4]"));
        assert!(masked.contains("[^5]"));
    }

    #[test]
    fn currency_is_not_math() {
        let text = "costs $5 and $10[^1] total";
        assert_eq!(mask_code_and_math(text), text);
    }

    #[test]
    fn unmatched_backticks_are_left_alone() {
        let text = "a ``b` [^1]";
        assert_eq!(mask_code_and_math(text), text);
    }

    #[test]
    fn tilde_fence_needs_matching_character_to_close() {
        let text = "~~~\n```\n[^1]\n~~~\n[^2]";
        let masked = mask_code_and_math(text);
        assert!(!masked.contains("[^1]"));
        assert!(masked.contains("[^2]"));
    }

    #[test]
    fn fence_inside_callout_is_masked() {
        let text = "> ```\n> [^1]\n> ```\n[^2]";
        let masked = mask_code_and_math(text);
        assert!(!masked.contains("[^1]"));
        assert!(masked.contains("[^2]"));
    }
}
