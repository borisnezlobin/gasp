//! The custom GPUI element that lays out and paints the editor's visible
//! lines, and records how long that takes.

use std::time::Instant;

use gpui::{
    App, AvailableSpace, BorderStyle, Bounds, BoxShadow, ContentMask, Corners, Element, ElementId,
    ElementInputHandler, Entity, GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId,
    Pixels, Style, TransformationMatrix, Window, fill, point, px, quad, relative, size,
    transparent_black,
};

use crate::editor::{EditorView, HighlightKind};
use crate::frame::{FrameLayout, PlacedLine};
use crate::icons::IconName;
use crate::line_layout::{Hit, Piece, PieceContent, Surface};
use crate::theme::Theme;

/// Suggestions draw above the text and the editor's own overlays, below
/// menus.
const SUGGESTION_LAYER: usize = 1;

/// Draws an [`EditorView`].
pub struct EditorElement {
    view: Entity<EditorView>,
}

impl EditorElement {
    pub fn new(view: Entity<EditorView>) -> Self {
        Self { view }
    }
}

/// What prepaint hands to paint.
pub struct Prepainted {
    frame: FrameLayout,
    selection: Vec<Bounds<Pixels>>,
    caret: Option<Bounds<Pixels>>,
    theme: Theme,
    /// Where the marker of the task under the pointer starts.
    hovered_task: Option<usize>,
}

impl IntoElement for EditorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepainted;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let started = Instant::now();
        self.view.update(cx, |view, cx| {
            let mut frame = view.layout_frame(bounds, window);
            frame.highlights = view.highlight_rects(&frame);
            let selection = view
                .selected_ranges()
                .iter()
                .flat_map(|range| frame.range_rects(range, &view.theme))
                .collect();
            let caret = frame.caret_bounds(view.cursor(), &view.theme);
            if view.focus_handle.is_focused(window)
                && let Some(mut popover) = view.suggestion_popover(&frame, cx)
            {
                popover.layout_as_root(AvailableSpace::min_size(), window, cx);
                window.defer_draw(popover, window.element_offset(), SUGGESTION_LAYER);
            }
            view.start_math_renders(cx);
            view.start_code_loads(cx);
            view.timings.layout.push(started.elapsed());
            Prepainted {
                frame,
                selection,
                caret,
                theme: view.theme.clone(),
                hovered_task: view.hovered_task,
            }
        })
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepainted: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let started = Instant::now();
        let focus_handle = self.view.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.view.clone()),
            cx,
        );
        let focused = focus_handle.is_focused(window);
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            paint_contents(prepainted, focused, window, cx);
        });
        let frame = prepainted.frame.clone();
        self.view
            .update(cx, |view, _| view.finish_frame(frame, started));
        EditorView::schedule_bench_step(&self.view, window, cx);
    }
}

impl EditorView {
    /// Background rectangles for every highlight kind, weaker kinds first
    /// so the active match paints on top.
    pub(crate) fn highlight_rects(
        &self,
        frame: &FrameLayout,
    ) -> Vec<(HighlightKind, Bounds<Pixels>)> {
        let (Some(first), Some(last)) = (frame.lines.first(), frame.lines.last()) else {
            return Vec::new();
        };
        let visible = first.visual.start..last.visual.end();
        self.highlights
            .iter()
            .flat_map(|(kind, ranges)| {
                ranges
                    .iter()
                    .filter(|range| range.start <= visible.end && range.end >= visible.start)
                    .flat_map(|range| frame.range_rects(range, &self.theme))
                    .map(move |rect| (*kind, rect))
            })
            .collect()
    }
}

fn paint_contents(prepainted: &Prepainted, focused: bool, window: &mut Window, cx: &mut App) {
    let theme = &prepainted.theme;
    let frame = &prepainted.frame;
    paint_surfaces(frame, theme, window);
    paint_text_backgrounds(frame, theme, window);
    for (kind, rect) in &frame.highlights {
        let color = match kind {
            HighlightKind::SearchMatch => theme.search_match,
            HighlightKind::ActiveSearchMatch => theme.active_search_match,
        };
        window.paint_quad(fill(*rect, color).corner_radii(theme.radius_sm / 2.));
    }
    for rect in &prepainted.selection {
        window.paint_quad(fill(*rect, theme.selection));
    }
    let context = PaintContext {
        text_left: frame.text_left,
        theme,
        hovered_task: prepainted.hovered_task,
    };
    for placed in &frame.lines {
        paint_line(placed, &context, window, cx);
    }
    if let Some(caret) = prepainted.caret.filter(|_| focused) {
        window.paint_quad(fill(caret, theme.cursor));
    }
    for placed in &frame.lines {
        paint_overlays(placed, frame.text_left, theme, window);
    }
}

/// A surface being drawn across consecutive lines.
struct OpenSurface {
    surface: Surface,
    top: Pixels,
    bottom: Pixels,
}

/// Paints line fills, merging consecutive lines of one block into one
/// rounded shape, then quote bars.
fn paint_surfaces(frame: &FrameLayout, theme: &Theme, window: &mut Window) {
    let mut open: Vec<OpenSurface> = Vec::new();
    for placed in frame
        .lines
        .iter()
        .filter(|placed| !placed.visual.is_collapsed())
    {
        let surfaces = &placed.visual.decor.surfaces;
        let (kept, ended): (Vec<_>, Vec<_>) = open.into_iter().partition(|run| {
            surfaces
                .iter()
                .any(|surface| same_block(surface, &run.surface))
        });
        for run in ended {
            paint_surface(&run, frame.text_left, theme, window);
        }
        open = kept;
        for surface in surfaces {
            match open
                .iter_mut()
                .find(|run| same_block(surface, &run.surface))
            {
                Some(run) => run.bottom = placed.bottom(),
                None => open.push(OpenSurface {
                    surface: surface.clone(),
                    top: placed.top,
                    bottom: placed.bottom(),
                }),
            }
        }
        for bar in &placed.visual.decor.bars {
            let bounds = Bounds::new(
                point(frame.text_left + bar.x, placed.top),
                size(bar.width, placed.visual.height),
            );
            window.paint_quad(fill(bounds, bar.color));
        }
    }
    for run in &open {
        paint_surface(run, frame.text_left, theme, window);
    }
}

/// Rounded fills behind inline code, highlights and tags, under the
/// selection so it shows over them. Fills that meet in a row, such as a
/// highlight across a bold word, join into one.
fn paint_text_backgrounds(frame: &FrameLayout, theme: &Theme, window: &mut Window) {
    for placed in &frame.lines {
        for row in &placed.visual.rows {
            let row_top = placed.top + row.top;
            let fills: Vec<(Bounds<Pixels>, Hsla)> = row
                .pieces
                .iter()
                .flat_map(|piece| piece_fills(piece, frame.text_left, row_top, theme))
                .collect();
            for (bounds, color) in join_fills(fills) {
                window.paint_quad(fill(bounds, color).corner_radii(theme.radius_sm));
            }
        }
    }
}

/// The fills of a text piece: as tall as its glyphs plus a little, and
/// reaching past inline code's ends into the room layout left.
fn piece_fills(
    piece: &Piece,
    text_left: Pixels,
    row_top: Pixels,
    theme: &Theme,
) -> Vec<(Bounds<Pixels>, Hsla)> {
    let PieceContent::Text(text) = &piece.content else {
        return Vec::new();
    };
    let glyphs = text.shaped.ascent + text.shaped.descent.abs();
    let top = row_top + piece.top + (text.line_height - glyphs) / 2. - theme.space_xs;
    let height = glyphs + theme.space_xs * 2.;
    let left = text_left + piece.x - text.slice_x;
    text.backgrounds
        .iter()
        .filter_map(|background| {
            let start = background.range.start.max(text.slice.start);
            let end = background.range.end.min(text.slice.end);
            if start >= end {
                return None;
            }
            let pad = |at_edge: bool| match background.padded && at_edge {
                true => theme.inline_code_padding,
                false => px(0.),
            };
            let x0 = left + text.shaped.x_for_index(start) - pad(start == background.range.start);
            let x1 = left + text.shaped.x_for_index(end) + pad(end == background.range.end);
            let bounds = Bounds::from_corners(point(x0, top), point(x1, top + height));
            Some((bounds, background.color))
        })
        .collect()
}

/// Joins fills of one colour that touch, left to right.
fn join_fills(mut fills: Vec<(Bounds<Pixels>, Hsla)>) -> Vec<(Bounds<Pixels>, Hsla)> {
    fills.sort_by(|a, b| f32::from(a.0.left()).total_cmp(&f32::from(b.0.left())));
    let mut joined: Vec<(Bounds<Pixels>, Hsla)> = Vec::with_capacity(fills.len());
    for (bounds, color) in fills {
        match joined.last_mut() {
            Some((last, last_color))
                if *last_color == color && bounds.left() <= last.right() + px(0.5) =>
            {
                *last = Bounds::from_corners(
                    point(last.left(), last.top().min(bounds.top())),
                    point(
                        last.right().max(bounds.right()),
                        last.bottom().max(bounds.bottom()),
                    ),
                );
            }
            _ => joined.push((bounds, color)),
        }
    }
    joined
}

fn same_block(a: &Surface, b: &Surface) -> bool {
    a.group == b.group && a.left == b.left
}

fn paint_surface(run: &OpenSurface, text_left: Pixels, theme: &Theme, window: &mut Window) {
    let bounds = Bounds::from_corners(
        point(text_left + run.surface.left, run.top),
        point(text_left + run.surface.left + run.surface.width, run.bottom),
    );
    window.paint_quad(fill(bounds, run.surface.color).corner_radii(theme.radius_md));
}

/// What painting a piece needs besides the piece.
struct PaintContext<'a> {
    text_left: Pixels,
    theme: &'a Theme,
    hovered_task: Option<usize>,
}

fn paint_line(placed: &PlacedLine, context: &PaintContext<'_>, window: &mut Window, cx: &mut App) {
    for row in &placed.visual.rows {
        let row_top = placed.top + row.top;
        for piece in &row.pieces {
            paint_piece(piece, context, row_top, window, cx);
        }
    }
    for piece in &placed.visual.decor.gutter {
        paint_piece(piece, context, placed.top, window, cx);
    }
}

/// A task's box: an outline that darkens under the pointer, filled with
/// the accent and checked when done.
fn paint_checkbox(
    piece: &Piece,
    bounds: Bounds<Pixels>,
    checked: bool,
    context: &PaintContext<'_>,
    window: &mut Window,
    cx: &mut App,
) {
    let theme = context.theme;
    let side = piece.height;
    let bounds = Bounds::new(bounds.origin, size(side, side));
    let hovered = matches!(&piece.hit, Hit::Checkbox { marker } if Some(marker.start) == context.hovered_task);
    let radius = theme.radius_sm * (side / theme.checkbox_size).min(1.);
    if checked {
        window.paint_quad(fill(bounds, theme.accent).corner_radii(radius));
        // Phosphor's regular check is a hairline at this size, so it's
        // drawn twice, a fraction of a pixel apart, to read as a mark.
        let inset = side * 0.1;
        for nudge in [px(0.), side * 0.03] {
            let mark = Bounds::new(
                point(bounds.left() + inset + nudge, bounds.top() + inset),
                size(side - inset * 2., side - inset * 2.),
            );
            let check = IconName::Check.path();
            let transform = TransformationMatrix::unit();
            report(window.paint_svg(mark, check, transform, theme.background, cx));
        }
        return;
    }
    let (border, background) = match hovered {
        true => (theme.text_muted, theme.tag_background),
        false => (theme.text_faint, transparent_black()),
    };
    window.paint_quad(quad(
        bounds,
        radius,
        background,
        theme.checkbox_border_width,
        border,
        BorderStyle::default(),
    ));
}

fn paint_piece(
    piece: &Piece,
    context: &PaintContext<'_>,
    row_top: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    let text_left = context.text_left;
    let origin = point(text_left + piece.x, row_top + piece.top);
    let bounds = Bounds::new(origin, size(piece.width, piece.height));
    match &piece.content {
        PieceContent::Text(text) if text.is_whole() => {
            report(text.shaped.paint(origin, text.line_height, window, cx));
        }
        PieceContent::Text(text) => {
            // A wrapped row paints the whole shaped chunk shifted so its
            // slice lands here; GPUI skips glyphs outside the mask.
            let shifted = point(origin.x - text.slice_x, origin.y);
            let mask = Bounds::new(
                point(origin.x, origin.y - text.line_height),
                size(piece.width, text.line_height * 3.),
            );
            window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
                report(text.shaped.paint(shifted, text.line_height, window, cx));
            });
        }
        PieceContent::Image { image, radius } => {
            report(window.paint_image(bounds, (*radius).into(), image.clone(), 0, false));
        }
        PieceContent::Icon { path, color } => {
            let icon = Bounds::new(origin, size(piece.height, piece.height));
            report(window.paint_svg(icon, path.clone(), TransformationMatrix::unit(), *color, cx));
        }
        PieceContent::Quad { color, radius } if color.a > 0. => {
            window.paint_quad(fill(bounds, *color).corner_radii(*radius));
        }
        PieceContent::Quad { .. } => {}
        PieceContent::Checkbox { checked } => {
            paint_checkbox(piece, bounds, *checked, context, window, cx);
        }
    }
}

/// Math previews float above the row they point at, over earlier lines.
fn paint_overlays(placed: &PlacedLine, text_left: Pixels, theme: &Theme, window: &mut Window) {
    let visual = &placed.visual;
    for overlay in &visual.overlays {
        let Some(row) = visual
            .row_for_offset(overlay.anchor)
            .map(|index| &visual.rows[index])
        else {
            continue;
        };
        let padding = theme.space_md;
        let width = overlay.width + padding * 2.;
        let height = overlay.height + padding * 2.;
        let x = text_left + row.x_for(overlay.anchor) - padding;
        let y = placed.top + row.top - height - theme.space_xs;
        let card = Bounds::new(point(x, y), size(width, height));
        paint_card(card, theme, window);
        let image_bounds = Bounds::new(
            point(x + padding, y + padding),
            size(overlay.width, overlay.height),
        );
        report(window.paint_image(
            image_bounds,
            Corners::default(),
            overlay.image.clone(),
            0,
            false,
        ));
    }
}

fn paint_card(bounds: Bounds<Pixels>, theme: &Theme, window: &mut Window) {
    let radius = theme.radius_md;
    window.paint_shadows(
        bounds,
        Corners::all(radius),
        &[BoxShadow {
            color: theme.shadow,
            offset: point(px(0.), theme.space_xs * 2.),
            blur_radius: theme.space_xl,
            spread_radius: px(0.),
        }],
    );
    window.paint_quad(quad(
        bounds,
        radius,
        theme.background,
        px(0.),
        transparent_black(),
        BorderStyle::default(),
    ));
}

fn report(result: anyhow::Result<()>) {
    if let Err(error) = result {
        eprintln!("paint failed: {error}");
    }
}
