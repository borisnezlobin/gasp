//! One line of text that ends in an ellipsis when it doesn't fit: tab
//! titles, file names, search excerpts, breadcrumbs.
//!
//! GPUI's own `text_ellipsis` decides where to cut the first time the text
//! is measured, which in a flex row is before the row knows its width, so
//! long text was clipped mid-letter instead. This element measures its
//! full width for layout and cuts the text in prepaint, against the width
//! it was actually given.

use std::ops::Range;

use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, HighlightStyle, InspectorElementId,
    IntoElement, LayoutId, Pixels, ShapedLine, SharedString, Size, Style, TextRun, TextStyle,
    Window, px, relative,
};

/// The mark that ends a cut line.
const ELLIPSIS: &str = "\u{2026}";

/// A line of text that truncates with an ellipsis. It takes the width it
/// needs, and can shrink to nothing in a flex row.
pub struct Truncated {
    text: SharedString,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    grow: bool,
}

/// Text that ends in "…" when it doesn't fit.
pub fn truncated(text: impl Into<SharedString>) -> Truncated {
    Truncated {
        text: text.into(),
        highlights: Vec::new(),
        grow: false,
    }
}

impl Truncated {
    /// Styles byte ranges of the text, such as search matches.
    pub fn with_highlights(
        mut self,
        highlights: impl IntoIterator<Item = (Range<usize>, HighlightStyle)>,
    ) -> Truncated {
        self.highlights = highlights.into_iter().collect();
        self
    }

    /// Takes the room left in its flex row, as `flex_1` would.
    pub fn grow(mut self) -> Truncated {
        self.grow = true;
        self
    }
}

/// What layout found: the style's line and the runs for the whole text.
pub struct TruncatedLayout {
    style: TextStyle,
    font_size: Pixels,
    line_height: Pixels,
    runs: Vec<TextRun>,
}

fn runs_for(
    text: &str,
    style: &TextStyle,
    highlights: &[(Range<usize>, HighlightStyle)],
) -> Vec<TextRun> {
    let mut runs = Vec::new();
    let mut at = 0;
    for (range, highlight) in highlights {
        let range = range.start.max(at).min(text.len())..range.end.min(text.len());
        if range.is_empty() {
            continue;
        }
        if at < range.start {
            runs.push(style.to_run(range.start - at));
        }
        runs.push(style.clone().highlight(*highlight).to_run(range.len()));
        at = range.end;
    }
    if at < text.len() {
        runs.push(style.to_run(text.len() - at));
    }
    runs
}

impl Element for Truncated {
    type RequestLayoutState = TruncatedLayout;
    type PrepaintState = Option<ShapedLine>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        _: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = style
            .line_height
            .to_pixels(font_size.into(), window.rem_size());
        let runs = runs_for(&self.text, &style, &self.highlights);
        let full_width = window
            .text_system()
            .shape_line(self.text.clone(), font_size, &runs, None)
            .width;
        let mut layout = Style::default();
        layout.min_size.width = px(0.).into();
        layout.flex_shrink = 1.;
        if self.grow {
            layout.flex_grow = 1.;
            layout.flex_basis = relative(0.).into();
        }
        // The full width, always: a flex row shrinks it when there's no
        // room, down to nothing, and prepaint cuts the text to fit.
        let layout_id = window.request_measured_layout(layout, move |known, _, _, _| Size {
            width: known.width.unwrap_or(full_width),
            height: line_height,
        });
        let state = TruncatedLayout {
            style,
            font_size,
            line_height,
            runs,
        };
        (layout_id, state)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let text_system = window.text_system().clone();
        let line = text_system.shape_line(self.text.clone(), layout.font_size, &layout.runs, None);
        if line.width <= bounds.size.width.ceil() {
            return Some(line);
        }
        let mut runs = layout.runs.clone();
        let cut = cx
            .text_system()
            .line_wrapper(layout.style.font(), layout.font_size)
            .truncate_line(self.text.clone(), bounds.size.width, ELLIPSIS, &mut runs);
        Some(text_system.shape_line(cut, layout.font_size, &runs, None))
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        line: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(line) = line.take() {
            let top = bounds.origin.y + (bounds.size.height - layout.line_height) / 2.;
            let origin = gpui::point(bounds.origin.x, top);
            line.paint_background(origin, layout.line_height, window, cx)
                .ok();
            line.paint(origin, layout.line_height, window, cx).ok();
        }
    }
}

impl IntoElement for Truncated {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_cover_the_text_around_highlights() {
        let style = TextStyle::default();
        let bold = HighlightStyle {
            font_weight: Some(gpui::FontWeight::BOLD),
            ..HighlightStyle::default()
        };
        let runs = runs_for("one two three", &style, &[(4..7, bold)]);
        let lengths: Vec<usize> = runs.iter().map(|run| run.len).collect();
        assert_eq!(lengths, [4, 3, 6]);
        let past_the_end = runs_for("ab", &style, &[(1..9, bold)]);
        assert_eq!(past_the_end.iter().map(|run| run.len).sum::<usize>(), 2);
    }
}
