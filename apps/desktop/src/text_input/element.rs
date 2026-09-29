//! Paints an input's line: the text or placeholder, the selection, the
//! IME composition underline and the caret, scrolled to keep the caret in
//! view. It also registers the input for platform text input.

use std::ops::Range;

use gpui::{
    App, Bounds, ContentMask, Element, ElementId, ElementInputHandler, Entity, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, PaintQuad, Pixels, Point, ShapedLine, SharedString,
    Style, TextRun, UnderlineStyle, Window, fill, point, relative, size,
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
    line: ShapedLine,
    origin: Point<Pixels>,
    line_height: Pixels,
    is_placeholder: bool,
    selection: Option<PaintQuad>,
    caret: Option<PaintQuad>,
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
        let line_height = self.input.read(cx).look().line_height;
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = line_height.into();
        (window.request_layout(style, [], cx), ())
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
        let shown_range =
            |range: Range<usize>| input.shown_offset(range.start)..input.shown_offset(range.end);
        let marked = input
            .state
            .marked()
            .filter(|_| !is_placeholder)
            .map(shown_range);
        let runs = text_runs(base, marked, theme.composition_underline_thickness);
        let line = window
            .text_system()
            .shape_line(shown, look.font_size, &runs, None);
        let (selected, cursor) = if is_placeholder {
            (0..0, 0)
        } else {
            (
                shown_range(input.state.selected_range()),
                input.shown_offset(input.state.cursor()),
            )
        };
        let caret_x = line.x_for_index(cursor);
        let visible = bounds.size.width - theme.caret_width;
        let scroll = scroll_to_show(input.scroll_x, caret_x, line.width, visible);
        let origin = point(bounds.left() - scroll, bounds.top());
        let x = |offset: usize| origin.x + line.x_for_index(offset);
        let selection = (!selected.is_empty()).then(|| {
            fill(
                Bounds::from_corners(
                    point(x(selected.start), bounds.top()),
                    point(x(selected.end), bounds.bottom()),
                ),
                theme.selection,
            )
        });
        let caret = selected.is_empty().then(|| {
            fill(
                Bounds::new(
                    point(x(cursor), bounds.top()),
                    size(theme.caret_width, bounds.size.height),
                ),
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
            if let Some(selection) = prepaint.selection.take().filter(|_| focused) {
                window.paint_quad(selection);
            }
            let painted = prepaint
                .line
                .paint(prepaint.origin, prepaint.line_height, window, cx);
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
