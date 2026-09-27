//! Rewrites bare vertical bars before conversion.
//!
//! In TeX `|` and `\|` are ordinary symbols, so `|x|` has no space inside.
//! mitex emits them as `| x |`, and Typst then spaces the bars like
//! relations. Wrapping each bar in `\mathord{…}` makes mitex emit
//! `mathord(|)`, which Typst lays out like TeX. Delimiters after `\left`,
//! `\right`, `\big` and friends, text arguments and array column specs are
//! left alone.

/// Commands whose next token is a delimiter that must stay as it is.
const DELIMITER_COMMANDS: [&str; 19] = [
    "left", "right", "middle", "big", "Big", "bigg", "Bigg", "bigl", "bigr", "Bigl", "Bigr",
    "biggl", "biggr", "Biggl", "Biggr", "bigm", "Bigm", "biggm", "Biggm",
];

/// Commands whose braced argument is text, copied verbatim.
pub(crate) const TEXT_COMMANDS: [&str; 10] = [
    "text",
    "textrm",
    "textit",
    "textbf",
    "textsf",
    "texttt",
    "textnormal",
    "mbox",
    "hbox",
    "operatorname",
];

/// Environments whose first braced argument is a column spec.
const COLUMN_SPEC_ENVIRONMENTS: [&str; 4] = ["{array}", "{tabular}", "{subarray}", "{array*}"];

/// Bar commands and the ordinary-symbol form that replaces them.
const BAR_COMMANDS: [(&str, &str); 6] = [
    ("vert", r"\mathord{|}"),
    ("lvert", r"\mathord{|}"),
    ("rvert", r"\mathord{|}"),
    ("Vert", r"\mathord{\|}"),
    ("lVert", r"\mathord{\|}"),
    ("rVert", r"\mathord{\|}"),
];

/// Returns `src` with every bare bar made an ordinary symbol.
pub(crate) fn tighten_bars(src: &str) -> String {
    let mut output = String::with_capacity(src.len() + 16);
    let mut rest = src;
    while let Some(index) = rest.find(['|', '\\']) {
        output.push_str(&rest[..index]);
        rest = &rest[index..];
        rest = match rest.strip_prefix('|') {
            Some(after) => {
                output.push_str(r"\mathord{|}");
                after
            }
            None => rewrite_command(rest, &mut output),
        };
    }
    output.push_str(rest);
    output
}

/// Handles the command at the start of `text`, which begins with `\`.
fn rewrite_command<'a>(text: &'a str, output: &mut String) -> &'a str {
    let (name, after) = split_command(text);
    if name == "|" {
        output.push_str(r"\mathord{\|}");
        return after;
    }
    if let Some((_, replacement)) = BAR_COMMANDS.iter().find(|(bar, _)| *bar == name) {
        output.push_str(replacement);
        return after;
    }
    output.push_str(&text[..text.len() - after.len()]);
    copy_protected_argument(name, after, output)
}

/// Copies the argument that follows `name` verbatim when it must not be
/// rewritten, and returns the remaining text.
fn copy_protected_argument<'a>(name: &str, after: &'a str, output: &mut String) -> &'a str {
    let length = if DELIMITER_COMMANDS.contains(&name) {
        delimiter_length(after)
    } else if TEXT_COMMANDS.contains(&name) {
        braced_group_length(after)
    } else if name == "begin" {
        column_spec_length(after)
    } else {
        0
    };
    output.push_str(&after[..length]);
    &after[length..]
}

/// Splits `\name` (letters, or one other character) off the front of `text`.
pub(crate) fn split_command(text: &str) -> (&str, &str) {
    let body = &text[1..];
    let letters = body
        .bytes()
        .take_while(|byte| byte.is_ascii_alphabetic())
        .count();
    let length = match letters {
        0 => body.chars().next().map_or(0, char::len_utf8),
        count => count,
    };
    (&body[..length], &body[length..])
}

/// Length of the whitespace and single delimiter token at the start of `text`.
fn delimiter_length(text: &str) -> usize {
    let spaces = text.len() - text.trim_start().len();
    let token = &text[spaces..];
    let token_length = if token.starts_with('\\') {
        let (_, after) = split_command(token);
        token.len() - after.len()
    } else {
        token.chars().next().map_or(0, char::len_utf8)
    };
    spaces + token_length
}

/// Length of the `{…}` group (with balanced braces) at the start of `text`,
/// after optional whitespace, or 0 when there is none.
fn braced_group_length(text: &str) -> usize {
    let spaces = text.len() - text.trim_start().len();
    if !text[spaces..].starts_with('{') {
        return 0;
    }
    let mut depth = 0usize;
    let mut escaped = false;
    for (offset, character) in text[spaces..].char_indices() {
        match (escaped, character) {
            (true, _) => escaped = false,
            (false, '\\') => escaped = true,
            (false, '{') => depth += 1,
            (false, '}') if depth == 1 => return spaces + offset + 1,
            (false, '}') => depth -= 1,
            _ => {}
        }
    }
    text.len()
}

/// For `\begin{array}{c|c}`, the length of `{array}{c|c}`; for other
/// environments, the length of just the name group.
fn column_spec_length(text: &str) -> usize {
    let name_length = braced_group_length(text);
    let name = text[..name_length].trim_start();
    if !COLUMN_SPEC_ENVIRONMENTS.contains(&name) {
        return name_length;
    }
    name_length + braced_group_length(&text[name_length..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_bars_become_ordinary() {
        assert_eq!(tighten_bars("|x|"), r"\mathord{|}x\mathord{|}");
        assert_eq!(tighten_bars(r"\|v\|"), r"\mathord{\|}v\mathord{\|}");
        assert_eq!(
            tighten_bars(r"\lvert x \rvert"),
            r"\mathord{|} x \mathord{|}"
        );
    }

    #[test]
    fn sized_delimiters_are_kept() {
        let source = r"\left| x \right| + \big\| y \big\| + \left\vert z \right\vert";
        assert_eq!(tighten_bars(source), source);
    }

    #[test]
    fn text_and_column_specs_are_kept() {
        let source = r"\text{a|b} \begin{array}{c|c} a & b \end{array}";
        assert_eq!(tighten_bars(source), source);
    }

    #[test]
    fn other_commands_and_unicode_pass_through() {
        let source = r"\frac{α}{β} \mid \setminus";
        assert_eq!(tighten_bars(source), source);
    }

    #[test]
    fn trailing_backslash_does_not_panic() {
        assert_eq!(tighten_bars("x\\"), "x\\");
        assert_eq!(tighten_bars(r"\left"), r"\left");
        assert_eq!(tighten_bars(r"\text{unclosed"), r"\text{unclosed");
    }
}
