//! Escaping text for Typst markup and string literals.

/// Characters that have a meaning somewhere in Typst markup.
fn needs_escape(character: char) -> bool {
    matches!(
        character,
        '\\' | '*'
            | '_'
            | '`'
            | '$'
            | '#'
            | '['
            | ']'
            | '<'
            | '>'
            | '@'
            | '='
            | '-'
            | '+'
            | '/'
            | '~'
            | '\''
            | '"'
            | '.'
            | ':'
    )
}

/// Escapes `text` so Typst markup shows it literally.
pub(crate) fn markup(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + text.len() / 4);
    for character in text.chars() {
        if needs_escape(character) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

/// Quotes `text` as a Typst string literal.
pub(crate) fn string(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('"');
    for character in text.chars() {
        match character {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_markup_characters() {
        assert_eq!(markup("a*b_c"), "a\\*b\\_c");
        assert_eq!(markup("// no comment"), "\\/\\/ no comment");
        assert_eq!(markup("#let x = 1"), "\\#let x \\= 1");
    }

    #[test]
    fn leaves_plain_text_alone() {
        assert_eq!(markup("plain words, äöü"), "plain words, äöü");
    }

    #[test]
    fn quotes_strings() {
        assert_eq!(string("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
    }
}
