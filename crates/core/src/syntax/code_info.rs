//! Parses a code fence's info string: language, `title:`, `ln:` and `hl:`.

use super::kinds::CodeBlockInfo;

pub(crate) fn parse(info: &str) -> CodeBlockInfo {
    let mut result = CodeBlockInfo {
        fenced: true,
        info: info.to_owned(),
        ..CodeBlockInfo::default()
    };
    let words = split_words(info);
    for (index, word) in words.iter().enumerate() {
        match word.split_once([':', '=']) {
            Some((key, value)) => apply_option(&mut result, key, unquote(value)),
            None if index == 0 => result.language = Some(word.clone()),
            None => {}
        }
    }
    result
}

fn apply_option(result: &mut CodeBlockInfo, key: &str, value: &str) {
    match key.to_ascii_lowercase().as_str() {
        "title" => result.title = Some(value.to_owned()),
        "ln" => result.line_numbers = parse_bool(value),
        "hl" => result.highlighted_lines = parse_line_ranges(value),
        _ => {}
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" => Some(true),
        "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_line_ranges(value: &str) -> Vec<(u32, u32)> {
    value
        .split(',')
        .filter_map(|part| {
            let (first, last) = part.split_once('-').unwrap_or((part, part));
            Some((first.trim().parse().ok()?, last.trim().parse().ok()?))
        })
        .collect()
}

fn unquote(value: &str) -> &str {
    let quoted = value.len() >= 2
        && (value.starts_with('"') && value.ends_with('"')
            || value.starts_with('\'') && value.ends_with('\''));
    if quoted {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

/// Splits on whitespace, keeping quoted runs together.
fn split_words(info: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for ch in info.chars() {
        match (quote, ch) {
            (Some(open), _) if ch == open => {
                quote = None;
                current.push(ch);
            }
            (None, '"' | '\'') => {
                quote = Some(ch);
                current.push(ch);
            }
            (None, _) if ch.is_whitespace() => push_word(&mut words, &mut current),
            _ => current.push(ch),
        }
    }
    push_word(&mut words, &mut current);
    words
}

fn push_word(words: &mut Vec<String>, current: &mut String) {
    if !current.is_empty() {
        words.push(std::mem::take(current));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_title_and_options_are_read() {
        let info = parse(r#"rust title:"my file.rs" ln:true hl:2,4-6"#);
        assert_eq!(info.language.as_deref(), Some("rust"));
        assert_eq!(info.title.as_deref(), Some("my file.rs"));
        assert_eq!(info.line_numbers, Some(true));
        assert_eq!(info.highlighted_lines, vec![(2, 2), (4, 6)]);
    }

    #[test]
    fn a_bare_title_has_no_language() {
        let info = parse("title=notes.txt");
        assert_eq!(info.language, None);
        assert_eq!(info.title.as_deref(), Some("notes.txt"));
    }
}
