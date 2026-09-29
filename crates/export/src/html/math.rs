//! LaTeX to MathML. Each equation goes through the same mitex conversion as
//! PDF export and is typeset by Typst's HTML output, which writes MathML
//! Core that current browsers render without any script or font.
//!
//! Every equation of a note is compiled in one Typst document, because the
//! per-document set-up costs more than the equations themselves. When that
//! document fails, the equations are compiled one by one so a single bad
//! equation only loses itself.

use typst::diag::SourceResult;
use typst_html::HtmlDocument;

use crate::pdf::escape;
use crate::pdf::world::ExportWorld;

/// An equation of the note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Equation {
    pub latex: String,
    pub display: bool,
}

/// The MathML of each equation, in order; `None` where it could not be
/// typeset. Also returns the stylesheet Typst ships with its MathML, when
/// there was any math.
pub(crate) fn to_mathml(equations: &[Equation]) -> (Vec<Option<String>>, Option<String>) {
    let converted: Vec<Option<String>> = equations
        .iter()
        .map(|equation| gasp_math::latex_to_typst(&equation.latex, equation.display).ok())
        .collect();
    if converted.iter().all(Option::is_none) {
        return (vec![None; equations.len()], None);
    }
    if let Some(output) = compile(&converted) {
        return extract(&output, converted.len());
    }
    let mut stylesheet = None;
    let mathml = converted
        .iter()
        .map(|typst| {
            let output = compile(std::slice::from_ref(typst))?;
            let (mut single, css) = extract(&output, 1);
            stylesheet = stylesheet.take().or(css);
            single.pop().flatten()
        })
        .collect();
    (mathml, stylesheet)
}

/// The id marking equation `index` in the compiled document.
fn marker(index: usize) -> String {
    format!("eq-{index}")
}

/// Compiles the equations into one HTML document, each in a marked `div`.
fn compile(equations: &[Option<String>]) -> Option<String> {
    let mut source = String::from(
        "#import \"/mitex/compat.typ\": mitex-scope\n\
         #let m(source, scope) = eval(source, scope: scope)\n",
    );
    for (index, equation) in equations.iter().enumerate() {
        let Some(equation) = equation else {
            continue;
        };
        source.push_str(&format!(
            "#html.elem(\"div\", attrs: (id: \"{}\"))[#m({}, {})]\n",
            marker(index),
            escape::string(equation),
            gasp_math::mitex_scope_for(equation)
        ));
    }
    let world = ExportWorld::html(source);
    let document: SourceResult<HtmlDocument> = typst::compile(&world).output;
    typst_html::html(&document.ok()?, &typst_html::HtmlOptions::default()).ok()
}

/// Where each equation's marker (`id="eq-N"`) first appears in `html`,
/// found in one pass rather than one search per equation.
fn marker_positions(html: &str, count: usize) -> Vec<Option<usize>> {
    const PREFIX: &str = "id=\"eq-";
    let mut positions = vec![None; count];
    for (at, _) in html.match_indices(PREFIX) {
        let rest = &html[at + PREFIX.len()..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        let canonical = digits == 1 || !rest.starts_with('0');
        if digits == 0 || !canonical || !rest[digits..].starts_with('"') {
            continue;
        }
        let Ok(index) = rest[..digits].parse::<usize>() else {
            continue;
        };
        if let Some(position @ None) = positions.get_mut(index) {
            *position = Some(at);
        }
    }
    positions
}

/// Cuts each marked equation's `<math>` element and the head's stylesheet
/// out of the compiled document.
fn extract(html: &str, count: usize) -> (Vec<Option<String>>, Option<String>) {
    let mathml = marker_positions(html, count)
        .into_iter()
        .map(|start| {
            let rest = &html[start?..];
            let div_end = rest.find("</div>")?;
            let open = rest[..div_end].find("<math")?;
            let close = rest[open..div_end].rfind("</math>")? + open + "</math>".len();
            Some(rest[open..close].to_owned())
        })
        .collect();
    let stylesheet = html
        .find("<style>")
        .and_then(|start| {
            let body = &html[start + "<style>".len()..];
            body.find("</style>").map(|end| body[..end].to_owned())
        })
        .filter(|css| !css.trim().is_empty());
    (mathml, stylesheet)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn equation(latex: &str, display: bool) -> Equation {
        Equation {
            latex: latex.to_owned(),
            display,
        }
    }

    #[test]
    fn typesets_inline_and_display_math() {
        let (mathml, css) = to_mathml(&[equation("x^2", false), equation("\\frac{a}{b}", true)]);
        let inline = mathml[0].as_deref().unwrap();
        assert!(inline.starts_with("<math"), "{inline}");
        assert!(inline.contains("<msup>"), "{inline}");
        let display = mathml[1].as_deref().unwrap();
        assert!(display.contains("display=\"block\""), "{display}");
        assert!(display.contains("<mfrac>"), "{display}");
        assert!(css.is_some_and(|css| css.contains("mfrac")));
    }

    #[test]
    fn a_bad_equation_only_loses_itself() {
        let (mathml, _) = to_mathml(&[
            equation("x", false),
            equation("\\left( x", false),
            equation("y", false),
        ]);
        assert!(mathml[0].is_some());
        assert!(mathml[1].is_none());
        assert!(mathml[2].is_some());
    }
}
