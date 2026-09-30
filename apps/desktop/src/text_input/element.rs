//! Paints an input's text: the text or placeholder, the selection, the
//! IME composition underline and the caret. A one-line input scrolls to
//! keep the caret in view; the title wraps onto as many lines as it needs
//! instead, as a heading would. It also registers the input for platform
//! text input.

use std::ops::Range;

use gpui::{
    App, AvailableSpace, Bounds, ContentMask, Element, ElementId, ElementInputHandler, Entity,
    GlobalElementId, InspectorElementId, IntoElement, LayoutId, PaintQuad, Pixels, Point,
    ShapedLine, SharedString, Size, Style, TextAlign, TextRun, UnderlineStyle, Window, WrappedLine,
    fill, point, px, relative, size,
};

use super::{PaintedLine, TextInput};

pub(super) struct TextLine {
    input: Entity<TextInput>,
}

impl TextLine {
    pub(super) fn new(input: Entity<TextInput>) -> Self {
        Self { input }
    }
}

pub(super) struct LinePaint {
    line: ShapedText,
    origin: Point<Pixels>,
    line_height: Pixels,
    is_placeholder: bool,
    selection: Vec<PaintQuad>,
    caret: Option<PaintQuad>,
}

/// The input's text as shaped: one line, or wrapped onto several.
#[derive(Clone)]
pub(super) enum ShapedText {
    Line(ShapedLine),
    Wrapped(WrappedLine),
}

impl ShapedText {
    /// Where byte `index` is drawn, from the text's origin.
    pub(super) fn position(&self, index: usize, line_height: Pixels) -> Point<Pixels> {
        match self {
            ShapedText::Line(line) => point(line.x_for_index(index), px(0.)),
            ShapedText::Wrapped(line) => line
                .position_for_index(index, line_height)
                .unwrap_or_else(|| point(line.width(), px(0.))),
        }
    }

    /// The byte offset nearest `position`, from the text's origin.
    pub(super) fn closest_index(&self, position: Point<Pixels>, line_height: Pixels) -> usize {
        match self {
            ShapedText::Line(line) => line.closest_index_for_x(position.x),
            ShapedText::Wrapped(line) => {
                let inside = point(position.x, position.y.max(px(0.)));
                match line.closest_index_for_position(inside, line_height) {
                    Ok(index) | Err(index) => index,
                }
            }
        }
    }

    fn draw(
        &self,
        origin: Point<Pixels>,
        line_height: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui::Result<()> {
        match self {
            ShapedText::Line(line) => line.paint(origin, line_height, window, cx),
            ShapedText::Wrapped(line) => {
                line.paint(origin, line_height, TextAlign::Left, None, window, cx)
            }
        }
    }
}

impl IntoElement for TextLine {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// How far to scroll so that `caret_x` shows in a `visible` wide box,
/// moving as little as possible from `scroll`.
pub(super) fn scroll_to_show(
    scroll: Pixels,
    caret_x: Pixels,
    line_width: Pixels,
    visible: Pixels,
) -> Pixels {
    let visible = visible.max(Pixels::ZERO);
    let scroll = if caret_x < scroll {
        caret_x
    } else if caret_x > scroll + visible {
        caret_x - visible
    } else {
        scroll
    };
    let max_scroll = (line_width - visible).max(Pixels::ZERO);
    scroll.min(max_scroll).max(Pixels::ZERO)
}

/// The quads that cover `from` to `to` in text wrapped `width` wide with
/// lines `line_height` tall: part of one line, or the rest of the first,
/// every line between and the start of the last.
fn selection_rects(
    from: Point<Pixels>,
    to: Point<Pixels>,
    width: Pixels,
    line_height: Pixels,
) -> Vec<Bounds<Pixels>> {
    let line = |y: Pixels, left: Pixels, right: Pixels| {
        Bounds::from_corners(point(left, y), point(right, y + line_height))
    };
    if from.y == to.y {
        return vec![line(from.y, from.x, to.x)];
    }
    let mut rects = vec![line(from.y, from.x, width)];
    let mut y = from.y + line_height;
    while y < to.y {
        rects.push(line(y, px(0.), width));
        y += line_height;
    }
    rects.push(line(to.y, px(0.), to.x));
    rects
}

/// The text the input draws, or its placeholder, and its runs, with the
/// composition underlined.
fn shown_runs(input: &TextInput) -> (SharedString, Vec<TextRun>) {
    let theme = &input.theme;
    let look = input.look();
    let is_placeholder = input.state.text().is_empty();
    let (shown, color): (SharedString, _) = if is_placeholder {
        (input.placeholder.clone(), theme.placeholder)
    } else {
        (input.shown_text().into(), look.text)
    };
    let base = TextRun {
        len: shown.len(),
        font: look.font.clone(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let marked = input
        .state
        .marked()
        .filter(|_| !is_placeholder)
        .map(|range| input.shown_offset(range.start)..input.shown_offset(range.end));
    let runs = text_runs(base, marked, theme.composition_underline_thickness);
    (shown, runs)
}

/// Shapes the text as one line, or wrapped at `width` when `wraps`.
fn shape(
    wraps: bool,
    shown: SharedString,
    font_size: Pixels,
    runs: &[TextRun],
    width: Pixels,
    window: &mut Window,
) -> ShapedText {
    let text_system = window.text_system();
    if wraps
        && let Some(line) = text_system
            .shape_text(shown.clone(), font_size, runs, Some(width), None)
            .ok()
            .and_then(|lines| lines.into_iter().next())
    {
        return ShapedText::Wrapped(line);
    }
    ShapedText::Line(text_system.shape_line(shown, font_size, runs, None))
}

/// Runs for `len` bytes of text, underlining the composition.
fn text_runs(base: TextRun, marked: Option<Range<usize>>, thickness: Pixels) -> Vec<TextRun> {
    let len = base.len;
    let Some(marked) = marked.filter(|marked| marked.end <= len) else {
        return vec![base];
    };
    let underline = UnderlineStyle {
        color: Some(base.color),
        thickness,
        wavy: false,
    };
    [
        (marked.start, None),
        (marked.len(), Some(underline)),
        (len - marked.end, None),
    ]
    .into_iter()
    .filter(|(len, _)| *len > 0)
    .map(|(len, underline)| TextRun {
        len,
        underline,
        ..base.clone()
    })
    .collect()
}

impl Element for TextLine {
    type RequestLayoutState = ();
    type PrepaintState = LinePaint;

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
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let input = self.input.read(cx);
        let line_height = input.look().line_height;
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        if !input.wraps() {
            style.size.height = line_height.into();
            return (window.request_layout(style, [], cx), ());
        }
        let (text, runs) = shown_runs(input);
        let font_size = input.look().font_size;
        let measure = move |known: Size<Option<Pixels>>,
                            available: Size<AvailableSpace>,
                            window: &mut Window,
                            _: &mut App| {
            let width = known.width.or(match available.width {
                AvailableSpace::Definite(width) => Some(width),
                _ => None,
            });
            let lines = window
                .text_system()
                .shape_text(text.clone(), font_size, &runs, width, None)
                .ok()
                .and_then(|lines| lines.into_iter().next())
                .map_or(1, |line| line.wrap_boundaries().len() + 1);
            size(width.unwrap_or_default(), line_height * lines as f32)
        };
        (window.request_measured_layout(style, measure), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> LinePaint {
        let input = self.input.read(cx);
        let theme = &input.theme;
        let look = input.look();
        let is_placeholder = input.state.text().is_empty();
        let (shown, runs) = shown_runs(input);
        let line = shape(
            input.wraps(),
            shown,
            look.font_size,
            &runs,
            bounds.size.width,
            window,
        );
        let shown_range =
            |range: Range<usize>| input.shown_offset(range.start)..input.shown_offset(range.end);
        let (selected, cursor) = if is_placeholder {
            (0..0, 0)
        } else {
            (
                shown_range(input.state.selected_range()),
                input.shown_offset(input.state.cursor()),
            )
        };
        let caret_at = line.position(cursor, look.line_height);
        let scroll = match &line {
            ShapedText::Line(shaped) => {
                let visible = bounds.size.width - theme.caret_width;
                scroll_to_show(input.scroll_x, caret_at.x, shaped.width, visible)
            }
            ShapedText::Wrapped(_) => px(0.),
        };
        let origin = point(bounds.left() - scroll, bounds.top());
        let selection = if selected.is_empty() {
            Vec::new()
        } else {
            let from = line.position(selected.start, look.line_height);
            let to = line.position(selected.end, look.line_height);
            selection_rects(from, to, bounds.size.width, look.line_height)
                .into_iter()
                .map(|rect| fill(rect + origin, theme.selection))
                .collect()
        };
        let caret = selected.is_empty().then(|| {
            fill(
                Bounds::new(origin + caret_at, size(theme.caret_width, look.line_height)),
                theme.caret,
            )
        });
        self.input.update(cx, |input, _| input.scroll_x = scroll);
        LinePaint {
            line,
            origin,
            line_height: look.line_height,
            is_placeholder,
            selection,
            caret,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut LinePaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        let focused = focus_handle.is_focused(window);
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            if focused {
                for quad in prepaint.selection.drain(..) {
                    window.paint_quad(quad);
                }
            }
            let painted = prepaint
                .line
                .draw(prepaint.origin, prepaint.line_height, window, cx);
            if let Err(error) = painted {
                eprintln!("could not paint a text input: {error}");
            }
            if let Some(caret) = prepaint.caret.take().filter(|_| focused) {
                window.paint_quad(caret);
            }
        });
        let painted = PaintedLine {
            line: (!prepaint.is_placeholder).then(|| prepaint.line.clone()),
            origin: prepaint.origin,
            line_height: prepaint.line_height,
        };
        self.input
            .update(cx, |input, _| input.painted = Some(painted));
    }
}

#[cfg(test)]
mod tests {
    use gpui::px;

    use super::*;

    #[test]
    fn scrolling_keeps_the_caret_in_view() {
        // Fits: no scroll.
        assert_eq!(scroll_to_show(px(0.), px(50.), px(80.), px(100.)), px(0.));
        // Caret past the right edge: scroll just enough.
        assert_eq!(
            scroll_to_show(px(0.), px(150.), px(200.), px(100.)),
            px(50.)
        );
        // Caret left of the view: scroll back to it.
        assert_eq!(
            scroll_to_show(px(80.), px(20.), px(200.), px(100.)),
            px(20.)
        );
        // Text got shorter: never scroll past its end.
        assert_eq!(
            scroll_to_show(px(90.), px(120.), px(120.), px(100.)),
            px(20.)
        );
    }

    #[test]
    fn a_selection_across_wrapped_lines_covers_each_line() {
        let line = px(20.);
        let one = selection_rects(
            point(px(10.), px(0.)),
            point(px(50.), px(0.)),
            px(100.),
            line,
        );
        assert_eq!(one.len(), 1);
        assert_eq!((one[0].left(), one[0].right()), (px(10.), px(50.)));
        let three = selection_rects(
            point(px(10.), px(0.)),
            point(px(30.), px(40.)),
            px(100.),
            line,
        );
        let spans: Vec<_> = three
            .iter()
            .map(|r| (r.left(), r.right(), r.top()))
            .collect();
        assert_eq!(
            spans,
            [
                (px(10.), px(100.), px(0.)),
                (px(0.), px(100.), px(20.)),
                (px(0.), px(30.), px(40.)),
            ]
        );
    }

    #[test]
    fn the_composition_is_underlined() {
        let base = TextRun {
            len: 9,
            font: gpui::font("Test"),
            color: gpui::black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = text_runs(base.clone(), Some(3..6), px(1.));
        let lens: Vec<_> = runs.iter().map(|run| run.len).collect();
        assert_eq!(lens, [3, 3, 3]);
        assert!(runs[1].underline.is_some() && runs[0].underline.is_none());
        assert_eq!(text_runs(base, None, px(1.)).len(), 1);
    }
}
