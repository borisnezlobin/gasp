//! Fills empty arguments with a box for the live preview.
//!
//! While `\frac{}{}` is being typed its groups are empty, and mitex turns
//! each into a zero-width space: the preview shows a bare fraction bar, or
//! next to nothing. Putting `\square` in an empty argument draws the shape
//! of what is being written, as other math editors do. Only groups that are
//! arguments are filled: after a command, a script mark or a previous
//! argument. A group standing alone, as in `{}^{14}C`, is TeX's way of
//! writing nothing and stays as it is, and so do text arguments.

use crate::bars::{TEXT_COMMANDS, split_command};

/// What an empty group would be an argument of.
#[derive(Clone, Copy, PartialEq)]
enum Before {
    /// A command, `^`, `_` or a previous argument: an empty group is an
    /// argument waiting to be written.
    Argument,
    /// Anything else, or a command whose argument isn't math.
    Other,
}

/// `src` with every empty argument group holding `\square`.
pub fn fill_empty_arguments(src: &str) -> String {
    let mut output = String::with_capacity(src.len() + 16);
    let mut before = Before::Other;
    let mut rest = src;
    while let Some(character) = rest.chars().next() {
        let (length, next) = match character {
            '\\' => command(rest),
            '{' => {
                let empty = empty_group_length(rest);
                if empty > 0 && before == Before::Argument {
                    output.push_str(r"{\square}");
                    rest = &rest[empty..];
                    continue;
                }
                (1, Before::Other)
            }
            '^' | '_' | '}' | ']' => (1, Before::Argument),
            other if other.is_whitespace() => (other.len_utf8(), before),
            other => (other.len_utf8(), Before::Other),
        };
        output.push_str(&rest[..length]);
        rest = &rest[length..];
        before = next;
    }
    output
}

/// The length of the command at the start of `text` and what it makes an
/// empty group after it.
fn command(text: &str) -> (usize, Before) {
    let (name, after) = split_command(text);
    let takes_math = name.starts_with(|c: char| c.is_ascii_alphabetic())
        && !TEXT_COMMANDS.contains(&name)
        && !matches!(name, "begin" | "end");
    let before = if takes_math {
        Before::Argument
    } else {
        Before::Other
    };
    (text.len() - after.len(), before)
}

/// The length of `{` plus optional whitespace plus `}` at the start of
/// `text`, or 0 when the group there holds something.
fn empty_group_length(text: &str) -> usize {
    let inner = &text[1..];
    let spaces = inner.len() - inner.trim_start().len();
    if inner[spaces..].starts_with('}') {
        spaces + 2
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_fraction_arguments_get_boxes() {
        assert_eq!(
            fill_empty_arguments(r"\frac{}{}"),
            r"\frac{\square}{\square}"
        );
        assert_eq!(fill_empty_arguments(r"\frac{a}{ }"), r"\frac{a}{\square}");
        assert_eq!(fill_empty_arguments(r"\sqrt[3]{}"), r"\sqrt[3]{\square}");
    }

    #[test]
    fn empty_scripts_get_boxes() {
        assert_eq!(fill_empty_arguments("x^{}_{}"), r"x^{\square}_{\square}");
        assert_eq!(fill_empty_arguments("x^ {}"), r"x^ {\square}");
    }

    #[test]
    fn written_arguments_are_untouched() {
        let source = r"\frac{a}{/} + \sqrt{x} + e^{i\pi}";
        assert_eq!(fill_empty_arguments(source), source);
    }

    #[test]
    fn empty_groups_that_are_not_arguments_stay() {
        for source in [
            r"{}^{14}C",
            r"a {} b",
            r"\text{}",
            r"\operatorname{}",
            r"\begin{}",
            r"\{}",
            r"x{}",
        ] {
            assert_eq!(fill_empty_arguments(source), source, "{source}");
        }
        assert_eq!(fill_empty_arguments(r"{}^{}"), r"{}^{\square}");
    }

    #[test]
    fn unicode_and_unbalanced_source_pass_through() {
        for source in [r"\frac{α}{β}", r"\frac{a}{", "{", "\\", "é^"] {
            assert_eq!(fill_empty_arguments(source), source, "{source}");
        }
    }

    #[test]
    fn filled_fractions_render_taller_than_bare_ones() {
        let bare = crate::render_latex(r"\frac{}{}", false, 16.).unwrap();
        let filled = crate::render_latex(&fill_empty_arguments(r"\frac{}{}"), false, 16.).unwrap();
        assert!(filled.height > bare.height + 4., "{filled:?}");
        assert!(filled.width > bare.width, "{filled:?}");
    }
}
