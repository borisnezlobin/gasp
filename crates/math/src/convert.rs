//! LaTeX to Typst conversion through mitex.

use crate::MathError;
use crate::bars::tighten_bars;

/// Converts LaTeX math to a Typst equation literal.
///
/// The result is `$body$` for inline math and `$ body $` for display math,
/// ready to be evaluated with the mitex scope. Bare `|` bars are made
/// ordinary symbols first so `|x|` is spaced as in TeX.
pub fn latex_to_typst(src: &str, display: bool) -> Result<String, MathError> {
    let body = mitex::convert_math(&tighten_bars(src), None).map_err(MathError::Convert)?;
    let body = body.trim();
    Ok(if display {
        format!("$ {body} $")
    } else {
        format!("${body}$")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_inline_without_spaces() {
        assert_eq!(latex_to_typst("x", false).unwrap(), "$x$");
    }

    #[test]
    fn wraps_display_with_spaces() {
        assert_eq!(latex_to_typst("x", true).unwrap(), "$ x $");
    }

    #[test]
    fn converts_fraction() {
        let typst = latex_to_typst(r"\frac{a}{b}", false).unwrap();
        assert!(typst.contains("frac("), "{typst}");
    }

    #[test]
    fn converts_environments_and_fonts() {
        let typst = latex_to_typst(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}", true).unwrap();
        assert!(typst.contains("pmatrix("), "{typst}");
        let typst = latex_to_typst(r"\mathbb{R}", false).unwrap();
        assert!(typst.contains("bb("), "{typst}");
    }

    #[test]
    fn unknown_command_is_an_error() {
        let error = latex_to_typst(r"\undefinedmacro{x}", false).unwrap_err();
        assert!(matches!(error, MathError::Convert(_)), "{error}");
    }

    #[test]
    fn stray_closing_brace_is_an_error() {
        assert!(latex_to_typst("}", false).is_err());
        assert!(latex_to_typst(r"\end{pmatrix}", false).is_err());
    }

    #[test]
    fn malformed_input_never_panics() {
        for source in [
            r"\frac{a}{",
            "{",
            "^",
            "_",
            "&",
            r"\\",
            r"\left(",
            r"\begin{cases}",
            "$",
            "",
        ] {
            let _ = latex_to_typst(source, false);
        }
    }
}
