//! Layout of one equation, and its SVG.

use std::sync::atomic::{AtomicUsize, Ordering};

use typst::introspection::Tag;
use typst::layout::{Abs, Frame, FrameItem};
use typst_layout::Page;

use crate::MathError;
use crate::convert::latex_to_typst;
use crate::scope::mitex_scope_for;
use crate::world::{SCOPE_MODULE_PATH, compile_document};

/// A rendered equation. Lengths are in typographic points.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedMath {
    /// A standalone SVG document of exactly `width` by `height` points.
    pub svg: String,
    /// Width of the equation.
    pub width: f64,
    /// Height of the equation.
    pub height: f64,
    /// Distance from the top edge to the baseline, for aligning inline math
    /// with the surrounding text.
    pub baseline: f64,
}

/// Performs the one-time setup (fonts, standard library, the mitex scope) so
/// the first real render is not slowed down by it.
pub fn warm_up() {
    crate::world::warm_up();
    let _ = render_typst("$x$", 12.0);
}

/// Converts LaTeX math with mitex and renders it.
pub fn render_latex(src: &str, display: bool, font_size: f64) -> Result<RenderedMath, MathError> {
    let equation = latex_to_typst(src, display)?;
    render_typst(&equation, font_size)
}

/// Renders a Typst equation literal (as returned by [`latex_to_typst`])
/// evaluated with the mitex scope.
pub fn render_typst(equation: &str, font_size: f64) -> Result<RenderedMath, MathError> {
    let page = layout_typst(equation, font_size)?;
    let metrics = Metrics::of(&page, is_display(equation));
    Ok(RenderedMath {
        svg: typst_svg::svg(&page, &typst_svg::SvgOptions::default()),
        width: metrics.width,
        height: metrics.height,
        baseline: metrics.baseline,
    })
}

/// Converts LaTeX math with mitex and lays it out as one page of exactly
/// the equation's size.
pub(crate) fn layout_latex(src: &str, display: bool, font_size: f64) -> Result<Page, MathError> {
    let equation = latex_to_typst(src, display)?;
    layout_typst(&equation, font_size)
}

/// Typst memoises every layout, so its memory grows with each equation
/// seen. Every this many renders, the layouts not used in the last
/// [`EVICTION_AGE`] rounds are let go.
const RENDERS_BETWEEN_EVICTIONS: usize = 200;
const EVICTION_AGE: usize = 2;

static RENDERS: AtomicUsize = AtomicUsize::new(0);

fn evict_now_and_then() {
    let renders = RENDERS.fetch_add(1, Ordering::Relaxed) + 1;
    if renders.is_multiple_of(RENDERS_BETWEEN_EVICTIONS) {
        typst::comemo::evict(EVICTION_AGE);
    }
}

fn layout_typst(equation: &str, font_size: f64) -> Result<Page, MathError> {
    evict_now_and_then();
    let main = main_source(equation, font_size);
    let document = compile_document(main).map_err(MathError::Render)?;
    document
        .pages()
        .first()
        .cloned()
        .ok_or_else(|| MathError::Render("no page was produced".to_owned()))
}

/// An equation's size and baseline in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Metrics {
    pub width: f64,
    pub height: f64,
    /// Distance from the top edge to the baseline.
    pub baseline: f64,
}

impl Metrics {
    pub(crate) fn of(page: &Page, display: bool) -> Metrics {
        let size = page.frame.size();
        let baseline = if display {
            first_group_baseline(&page.frame)
        } else {
            equation_tag_baseline(&page.frame)
        };
        Metrics {
            width: size.x.to_pt(),
            height: size.y.to_pt(),
            baseline: baseline.unwrap_or(size.y).to_pt(),
        }
    }
}

/// Display literals are `$ … $`: Typst treats an equation that starts with
/// whitespace as a block.
fn is_display(equation: &str) -> bool {
    equation
        .strip_prefix('$')
        .is_some_and(|rest| rest.starts_with(char::is_whitespace))
}

fn main_source(equation: &str, font_size: f64) -> String {
    format!(
        "#import \"{SCOPE_MODULE_PATH}\": mitex-scope\n\
         #set page(width: auto, height: auto, margin: 0pt, fill: none)\n\
         #set text(size: {font_size}pt, top-edge: \"bounds\", bottom-edge: \"bounds\")\n\
         #eval(\"{}\", scope: {})\n",
        escape_typst_string(equation),
        mitex_scope_for(equation)
    )
}

fn escape_typst_string(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 8);
    for character in text.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// A display equation's first line is a hard frame with its own baseline.
fn first_group_baseline(frame: &Frame) -> Option<Abs> {
    frame.items().find_map(|(position, item)| match item {
        FrameItem::Group(group) if group.frame.has_baseline() => {
            Some(position.y + group.frame.baseline())
        }
        _ => None,
    })
}

/// Inline equations are flattened into the paragraph line, but the tag that
/// marks where the equation starts sits on the line's baseline.
///
/// The element is matched by name: in the desktop build a type check
/// (`is::<EquationElem>()`) never matched, for reasons not yet known, and
/// every inline equation then sat on its bottom edge.
///
/// Typst sometimes wraps the line in a group (for example when the
/// equation starts with an attachment or an operator such as `\det`), so
/// the search goes into groups too, adding their offsets.
fn equation_tag_baseline(frame: &Frame) -> Option<Abs> {
    frame.items().find_map(|(position, item)| match item {
        FrameItem::Tag(Tag::Start(content, _)) if content.elem().name() == "equation" => {
            Some(position.y)
        }
        FrameItem::Group(group) => equation_tag_baseline(&group.frame).map(|y| position.y + y),
        _ => None,
    })
}

/// Frees Typst's memoized layout results that have not been used in the last
/// `max_age` calls to this function. Rendering already does this every few
/// hundred equations; call it to let go of more, sooner.
pub fn evict_layout_memory(max_age: usize) {
    typst::comemo::evict(max_age);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_equation_has_sane_dimensions() {
        let rendered = render_latex("x", false, 16.0).unwrap();
        assert!(rendered.svg.starts_with("<svg"));
        assert!(rendered.svg.contains("</svg>"));
        assert!(rendered.width > 4.0 && rendered.width < 16.0);
        assert!(rendered.height > 4.0 && rendered.height < 16.0);
        assert!(rendered.baseline > 0.0 && rendered.baseline <= rendered.height);
    }

    #[test]
    fn descender_puts_baseline_above_bottom() {
        let rendered = render_latex("y", false, 16.0).unwrap();
        assert!(rendered.baseline < rendered.height - 1.0);
    }

    #[test]
    fn equations_starting_with_an_operator_or_attachment_find_their_baseline() {
        for source in ["\\det(P) \\neq 0", "U^T U = I", "\\prod_{j=1}^6 (1+r_j)"] {
            let rendered = render_latex(source, false, 16.0).unwrap();
            assert!(
                rendered.baseline < rendered.height - 0.2,
                "{source}: baseline {} of {}",
                rendered.baseline,
                rendered.height
            );
        }
    }

    #[test]
    fn fraction_baseline_is_between_numerator_and_denominator() {
        let rendered = render_latex(r"\frac{a}{b}", false, 16.0).unwrap();
        assert!(rendered.baseline > rendered.height * 0.4);
        assert!(rendered.baseline < rendered.height * 0.9);
    }

    #[test]
    fn display_math_is_taller_than_inline() {
        let source = r"\sum_{i=1}^{n} i^2";
        let inline = render_latex(source, false, 16.0).unwrap();
        let display = render_latex(source, true, 16.0).unwrap();
        assert!(display.height > inline.height);
        assert!(display.baseline > 0.0 && display.baseline < display.height);
    }

    #[test]
    fn font_size_scales_output() {
        let small = render_latex(r"\alpha + \beta", false, 10.0).unwrap();
        let large = render_latex(r"\alpha + \beta", false, 20.0).unwrap();
        let ratio = large.width / small.width;
        assert!((ratio - 2.0).abs() < 0.05, "ratio {ratio}");
    }

    fn view_box_size(svg: &str) -> (f64, f64) {
        let start = svg.find("viewBox=\"").unwrap() + "viewBox=\"".len();
        let end = start + svg[start..].find('"').unwrap();
        let numbers: Vec<f64> = svg[start..end]
            .split_whitespace()
            .map(|number| number.parse().unwrap())
            .collect();
        (numbers[2], numbers[3])
    }

    #[test]
    fn svg_size_matches_reported_size() {
        let rendered = render_latex(r"\sqrt{2}", false, 16.0).unwrap();
        let (width, height) = view_box_size(&rendered.svg);
        assert!((width - rendered.width).abs() < 0.01);
        assert!((height - rendered.height).abs() < 0.01);
    }

    #[test]
    fn symbols_renamed_in_current_typst_still_render() {
        let source = r"\cap \partial \hbar \oplus \langle x \rangle \bigoplus \odot";
        render_latex(source, false, 16.0).unwrap();
    }

    #[test]
    fn color_commands_render() {
        render_latex(
            r"\color{red} x + \textcolor{blue}{y} + \colorbox{yellow}{z}",
            false,
            16.0,
        )
        .unwrap();
    }

    #[test]
    fn absolute_value_bars_are_tight() {
        let bars = render_latex("|x|", false, 16.0).unwrap();
        let sized = render_latex(r"\left| x \right|", false, 16.0).unwrap();
        assert!((bars.width - sized.width).abs() < 0.01);
    }

    #[test]
    fn typst_errors_are_returned() {
        let error = render_latex(r"\left( x", false, 16.0).unwrap_err();
        assert!(matches!(error, MathError::Render(_)), "{error}");
    }

    #[test]
    fn quotes_and_backslashes_survive_escaping() {
        render_latex(r#"\text{a "quoted" word} \setminus \{1\}"#, false, 16.0).unwrap();
    }

    #[test]
    fn display_literal_detection() {
        assert!(is_display("$ x $"));
        assert!(!is_display("$x$"));
        assert!(!is_display("x"));
    }
}
