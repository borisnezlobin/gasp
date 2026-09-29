//! The custom GPUI element that lays out and paints the editor's visible
//! lines, and records how long that takes.

use std::ops::Range;
use std::time::Instant;

use gpui::{
    App, AvailableSpace, BorderStyle, Bounds, BoxShadow, ContentMask, Corners, CursorStyle,
    DispatchPhase, Element, ElementId, ElementInputHandler, Entity, GlobalElementId, Hitbox,
    HitboxBehavior, Hsla, InspectorElementId, IntoElement, LayoutId, MouseMoveEvent, Pixels,
    SharedString, Style, TextRun, TransformationMatrix, UnderlineStyle, Window, fill, point, px,
    quad, relative, size, transparent_black,
};

use crate::code_copy::{CopyButton, blocks_on_screen, copied_width, copy_icon_size};
use crate::editor::{EditorView, HighlightKind};
use crate::frame::{FrameLayout, PlacedLine};
use crate::icons::IconName;
use crate::line_layout::{Hit, Piece, PieceContent, Surface};
use crate::prose::ProseFrame;
use crate::table_edit::paint::{DragMark, TableMarks};
use crate::theme::Theme;

/// Suggestions and hover previews draw above the text and the editor's
/// own overlays, below menus.
pub(crate) const SUGGESTION_LAYER: usize = 1;

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
    /// The table editor's fills, rings, handles and drag.
    tables: TableMarks,
    caret: Option<Bounds<Pixels>>,
    theme: Theme,
    /// The code block copy button, with its hitbox for the pointer.
    copy_button: Option<(CopyButton, Hitbox)>,
    /// Where the marker of the task under the pointer starts.
    hovered_task: Option<usize>,
    /// The line of the link card under the pointer, and whether the
    /// pointer is on its Open button.
    hovered_card: Option<(usize, bool)>,
    /// Sentence tints and grammar underlines.
    prose: ProseFrame,
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
        let _span = crate::trace::span("editor-prepaint");
        let _phase = crate::keytrace::span("(total) prepaint");
        self.view.update(cx, |view, cx| {
            // A table's drag shows no revealed markup, only the cells.
            view.reveals_at_cursor = !view.read_only
                && view.focus_handle.is_focused(window)
                && view.table_edit.drag.is_none();
            let mut frame = view.layout_frame(bounds, window);
            let phase = crate::keytrace::span("selection-and-highlights");
            frame.highlights = view.highlight_rects(&frame);
            let selection = view
                .selected_ranges()
                .iter()
                .flat_map(|range| frame.range_rects(range, &view.theme))
                .collect();
            let caret = frame.caret_bounds(view.cursor(), &view.theme);
            let focused = view.focus_handle.is_focused(window);
            let tables = view.table_marks(&frame, focused && !view.read_only);
            if tables.animating {
                window.request_animation_frame();
            }
            // Cells selected across are drawn whole, not as text, and
            // without a caret; a table's drag hides the caret too.
            let (selection, caret) = match &tables.block {
                Some(block) => (block.clone(), None),
                None if tables.drag.is_some() => (selection, None),
                None => (selection, caret),
            };
            drop(phase);
            let phase = crate::keytrace::span("prose");
            let prose = view.prose_frame(&frame, cx);
            drop(phase);
            let phase = crate::keytrace::span("popovers");
            if view.focus_handle.is_focused(window)
                && let Some(mut popover) = view.suggestion_popover(&frame, cx)
            {
                popover.layout_as_root(AvailableSpace::min_size(), window, cx);
                window.defer_draw(popover, window.element_offset(), SUGGESTION_LAYER);
            }
            view.draw_floating_toolbars(&frame, focused, window, cx);
            if let Some(mut chip) = view.card_offer_chip(&frame, cx) {
                chip.layout_as_root(AvailableSpace::min_size(), window, cx);
                window.defer_draw(chip, window.element_offset(), SUGGESTION_LAYER);
            }
            if let Some(mut popover) = view.hover_popover(&frame, window, cx) {
                // Laid out at its natural width, so a message wraps inside
                // its box rather than past it.
                let natural = gpui::size(AvailableSpace::MaxContent, AvailableSpace::MaxContent);
                popover.layout_as_root(natural, window, cx);
                window.defer_draw(popover, window.element_offset(), SUGGESTION_LAYER);
            }
            drop(phase);
            let phase = crate::keytrace::span("start-loads");
            // Math and code highlighting can wait for the first frame.
            if crate::first_frame::is_waiting() {
                let view = cx.entity().downgrade();
                crate::first_frame::defer(move |cx| {
                    view.update(cx, |view, cx| {
                        view.start_math_renders(cx);
                        view.start_code_loads(cx);
                        view.start_remote_images(cx);
                        view.find_vault_images(cx);
                    })
                    .ok();
                });
            } else {
                view.start_math_renders(cx);
                view.start_code_loads(cx);
                view.start_remote_images(cx);
                view.find_vault_images(cx);
            }
            drop(phase);
            let copy_button = view.copy_button_for(&frame, bounds).map(|button| {
                let hitbox = window.insert_hitbox(button.bounds, HitboxBehavior::Normal);
                (button, hitbox)
            });
            view.timings.layout.push(started.elapsed());
            Prepainted {
                frame,
                selection,
                tables,
                caret,
                theme: view.theme.clone(),
                copy_button,
                hovered_task: view.hovered_task,
                hovered_card: view.hovered_card,
                prose,
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
        let phase = crate::keytrace::span("paint");
        let focus_handle = self.view.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.view.clone()),
            cx,
        );
        let focused = focus_handle.is_focused(window);
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            paint_contents(prepainted, focused, window, cx);
            if let Some((button, hitbox)) = &prepainted.copy_button {
                window.set_cursor_style(CursorStyle::PointingHand, hitbox);
                paint_copy_button(button, &prepainted.theme, window, cx);
            }
        });
        drop(phase);
        // A drag selection follows the pointer anywhere in the window, not
        // only over the note, so it can run past the edge and scroll; so
        // does a table handle's drag.
        let view = self.view.read(cx);
        if view.is_selecting || view.table_edit.drag.is_some() {
            let view = self.view.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    view.update(cx, |view, cx| view.drag_moved(event.position, window, cx));
                }
            });
        }
        let frame = prepainted.frame.clone();
        self.view
            .update(cx, |view, _| view.finish_frame(frame, started));
        // A preview's popover is as tall as its note, which is only known
        // once the lines are laid out: draw again until it settles.
        if self.view.read(cx).take_height_change() {
            window.request_animation_frame();
        }
        EditorView::schedule_bench_step(&self.view, window, cx);
    }
}

impl EditorView {
    /// The copy button to draw over this frame: only while the pointer is
    /// over the editor or a block was just copied, so typing pays nothing.
    fn copy_button_for(
        &mut self,
        frame: &FrameLayout,
        bounds: Bounds<Pixels>,
    ) -> Option<CopyButton> {
        if !self.code_copy.is_active() {
            return None;
        }
        let blocks = blocks_on_screen(frame, self.source.tree());
        self.code_copy.set_blocks(blocks, bounds);
        self.code_copy.button(&self.theme)
    }

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
        let shows =
            |range: &&Range<usize>| range.start <= visible.end && range.end >= visible.start;
        let mut rects: Vec<_> = self
            .highlights
            .iter()
            .flat_map(|(kind, ranges)| {
                ranges
                    .iter()
                    .filter(shows)
                    .flat_map(|range| frame.range_rects(range, &self.theme))
                    .map(move |rect| (*kind, rect))
            })
            .collect();
        let stops = self.pending_tab_stops();
        let stops = stops
            .iter()
            .filter(shows)
            .flat_map(|range| self.tab_stop_rects(frame, range));
        rects.extend(stops.map(|rect| (HighlightKind::TabStop, rect)));
        rects
    }

    /// The mark on a tab stop: its text's background. An empty stop is
    /// a block the line lays out with room of its own.
    fn tab_stop_rects(&self, frame: &FrameLayout, range: &Range<usize>) -> Vec<Bounds<Pixels>> {
        if range.is_empty() {
            return Vec::new();
        }
        frame.range_rects(range, &self.theme)
    }
}

fn paint_contents(prepainted: &Prepainted, focused: bool, window: &mut Window, cx: &mut App) {
    let theme = &prepainted.theme;
    let frame = &prepainted.frame;
    paint_surfaces(frame, theme, window);
    paint_bands(frame, theme, window);
    for (bounds, color) in &prepainted.tables.fills {
        window.paint_quad(fill(*bounds, *color));
    }
    for (rect, color) in &prepainted.prose.tints {
        window.paint_quad(fill(*rect, *color).corner_radii(theme.radius_sm));
    }
    paint_text_backgrounds(frame, theme, window);
    paint_highlights(frame, theme, |kind| !drawn_over_selection(kind), window);
    let selection_color = match prepainted.tables.block {
        Some(_) => theme.table.selection,
        None => theme.selection,
    };
    for rect in &prepainted.selection {
        window.paint_quad(fill(*rect, selection_color));
    }
    paint_highlights(frame, theme, drawn_over_selection, window);
    let context = PaintContext {
        text_left: frame.text_left,
        theme,
        hovered_task: prepainted.hovered_task,
        hovered_card: prepainted.hovered_card,
    };
    for placed in &frame.lines {
        paint_line(placed, &context, window, cx);
    }
    paint_flag_underlines(&prepainted.prose, theme, window);
    if let Some(ring) = prepainted.tables.ring.filter(|_| focused) {
        paint_cell_ring(ring, theme, window);
    }
    if let Some(caret) = prepainted.caret.filter(|_| focused) {
        window.paint_quad(fill(caret, theme.cursor));
    }
    paint_table_handles(&prepainted.tables, theme, window, cx);
    if let Some(drag) = &prepainted.tables.drag {
        paint_table_drag(drag, &context, window, cx);
    }
    for placed in &frame.lines {
        paint_overlays(placed, frame.text_left, theme, window);
    }
}

/// The find bar selects the match it's on, so that match is drawn over
/// the selection, or the selection would hide what marks it as current.
fn drawn_over_selection(kind: HighlightKind) -> bool {
    kind == HighlightKind::ActiveSearchMatch
}

/// The highlights of the kinds `include` takes: find matches and tab
/// stops as fills, footnote problems underlined.
fn paint_highlights(
    frame: &FrameLayout,
    theme: &Theme,
    include: impl Fn(HighlightKind) -> bool,
    window: &mut Window,
) {
    for (kind, rect) in frame.highlights.iter().filter(|(kind, _)| include(*kind)) {
        let color = match kind {
            HighlightKind::SearchMatch => theme.search_match,
            HighlightKind::ActiveSearchMatch => theme.active_search_match,
            HighlightKind::FootnoteProblem => {
                paint_problem_underline(*rect, theme, window);
                continue;
            }
            HighlightKind::TabStop => theme.tab_stop,
        };
        window.paint_quad(fill(*rect, color).corner_radii(theme.radius_sm / 2.));
    }
}

/// The grammar checker's wavy underlines, over the text so a descender
/// never hides one.
fn paint_flag_underlines(prose: &ProseFrame, theme: &Theme, window: &mut Window) {
    for (origin, width, color) in &prose.underlines {
        let style = UnderlineStyle {
            thickness: theme.flag_underline_thickness,
            color: Some(*color),
            wavy: true,
        };
        window.paint_underline(*origin, *width, &style);
    }
}

/// A footnote problem's underline: a line in the error colour just under
/// the text, like a spelling mark, so the text itself stays readable.
fn paint_problem_underline(rect: Bounds<Pixels>, theme: &Theme, window: &mut Window) {
    let thickness = theme.problem_underline_thickness;
    let line = Bounds {
        origin: point(rect.left(), rect.bottom() - thickness - theme.space_xs),
        size: size(rect.size.width, thickness),
    };
    window.paint_quad(fill(line, theme.error).corner_radii(thickness / 2.));
}

/// The copy button: a copy icon on the code's own fill, which a check and
/// "Copied" replace for a moment after a copy.
fn paint_copy_button(button: &CopyButton, theme: &Theme, window: &mut Window, cx: &mut App) {
    let icon_size = copy_icon_size(theme);
    let label = button.copied.then(|| {
        let run = TextRun {
            len: COPIED.len(),
            font: theme.ui_font(),
            color: theme.text_muted,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window.text_system().shape_line(
            SharedString::new_static(COPIED),
            theme.small_font_size,
            &[run],
            None,
        )
    });
    let mut bounds = button.bounds;
    if let Some(label) = &label {
        let width = copied_width(label.width, theme);
        bounds.origin.x = bounds.right() - width;
        bounds.size.width = width;
    }
    let background = if button.pressed_on {
        theme.divider
    } else {
        theme.code_background
    };
    window.paint_quad(fill(bounds, background).corner_radii(theme.radius_sm));
    let inset = (button.bounds.size.height - icon_size) / 2.;
    let icon_origin = point(bounds.left() + inset, bounds.top() + inset);
    let (icon, color) = if button.copied {
        (IconName::Check, theme.text)
    } else if button.pressed_on {
        (IconName::Copy, theme.text)
    } else {
        (IconName::Copy, theme.text_muted)
    };
    let icon_bounds = Bounds::new(icon_origin, size(icon_size, icon_size));
    report(window.paint_svg(
        icon_bounds,
        icon.path(),
        TransformationMatrix::unit(),
        color,
        cx,
    ));
    if let Some(label) = label {
        let line_height = theme.small_font_size * theme.ui_line_height_factor;
        let origin = point(
            icon_origin.x + icon_size + theme.space_sm,
            bounds.top() + (bounds.size.height - line_height) / 2.,
        );
        report(label.paint(origin, line_height, window, cx));
    }
}

const COPIED: &str = "Copied";

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
                // A block starting here keeps its margin outside its fill.
                None => open.push(OpenSurface {
                    surface: surface.clone(),
                    top: placed.top + placed.visual.decor.margin_top,
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

/// Paints each line's bands over the block surfaces, with a bar at the
/// band's left edge so a highlighted line reads without relying on tint.
fn paint_bands(frame: &FrameLayout, theme: &Theme, window: &mut Window) {
    for placed in &frame.lines {
        for band in &placed.visual.decor.bands {
            let origin = point(frame.text_left + band.left, placed.top);
            let height = placed.visual.height;
            window.paint_quad(fill(
                Bounds::new(origin, size(band.width, height)),
                band.color,
            ));
            let edge = Bounds::new(origin, size(theme.quote_bar_width, height));
            window.paint_quad(fill(edge, theme.text_faint));
        }
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
            let pad = |at_edge: bool| match at_edge {
                true => background.padding,
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
    hovered_card: Option<(usize, bool)>,
}

fn paint_line(placed: &PlacedLine, context: &PaintContext<'_>, window: &mut Window, cx: &mut App) {
    let card = context
        .hovered_card
        .filter(|(line, _)| *line == placed.visual.line);
    for row in &placed.visual.rows {
        let row_top = placed.top + row.top;
        for piece in &row.pieces {
            if !matches!(piece.hit, Hit::Link { .. }) {
                paint_piece(piece, context, row_top, window, cx);
            } else if let Some((_, on_button)) = card {
                let piece = open_button_piece(piece, on_button, context.theme);
                paint_piece(&piece, context, row_top, window, cx);
            }
        }
    }
    for piece in &placed.visual.decor.gutter {
        paint_piece(piece, context, placed.top, window, cx);
    }
}

/// A piece of a link card's Open button, darker under the pointer as a
/// code block's copy button is.
fn open_button_piece(piece: &Piece, on_button: bool, theme: &Theme) -> Piece {
    let mut piece = piece.clone();
    match &mut piece.content {
        PieceContent::Quad { color, .. } if on_button => *color = theme.divider,
        PieceContent::Icon { color, .. } if on_button => *color = theme.text,
        _ => {}
    }
    piece
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
            // A chip's text starts its padding in from the piece's edge.
            let inset = point(origin.x - text.slice_x, origin.y);
            report(text.shaped.paint(inset, text.line_height, window, cx));
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
        PieceContent::TabStop => {
            let theme = context.theme;
            window.paint_quad(fill(bounds, theme.tab_stop).corner_radii(theme.radius_sm / 2.));
        }
    }
}

/// The cell being edited: a tint with a ring inside its square edges,
/// laid over the cell's own rules.
fn paint_cell_ring(bounds: Bounds<Pixels>, theme: &Theme, window: &mut Window) {
    let look = &theme.table;
    window.paint_quad(quad(
        bounds,
        px(0.),
        look.active_fill,
        look.active_ring_width,
        look.active_ring,
        BorderStyle::default(),
    ));
}

/// The row and column handles: dots, a fill under the pointer or while
/// held, all faded in by the handle's opacity.
fn paint_table_handles(marks: &TableMarks, theme: &Theme, window: &mut Window, cx: &mut App) {
    let look = &theme.table;
    for handle in &marks.handles {
        let faded = |color: Hsla| Hsla {
            a: color.a * handle.opacity,
            ..color
        };
        let (fill_color, icon_color) = match handle.hot {
            true => (look.handle_fill, look.handle_icon_hover),
            false => (transparent_black(), look.handle_icon),
        };
        window.paint_quad(fill(handle.bounds, faded(fill_color)).corner_radii(look.handle_radius));
        report(window.paint_svg(
            handle.bounds,
            handle.icon.path(),
            TransformationMatrix::unit(),
            faded(icon_color),
            cx,
        ));
    }
}

/// A row or column being dragged: the line where it would drop, then
/// over it an opaque card on the note's own fill, raised with a shadow,
/// holding the row or column with its text a little faded.
fn paint_table_drag(
    drag: &DragMark,
    context: &PaintContext<'_>,
    window: &mut Window,
    cx: &mut App,
) {
    let look = &context.theme.table;
    let radius = context.theme.radius_sm;
    if let Some(source) = drag.source {
        window.paint_quad(fill(source, look.drag_source_veil));
    }
    if let Some(indicator) = drag.indicator {
        let round = indicator.size.height.min(indicator.size.width) / 2.;
        window.paint_quad(fill(indicator, look.drop_indicator).corner_radii(round));
    }
    // The card's edge is a hairline ring drawn as a shadow, not a border,
    // so its rounded corners stay even.
    window.paint_shadows(
        drag.card,
        Corners::all(radius),
        &[
            BoxShadow {
                color: look.drag_shadow,
                offset: point(px(0.), look.drag_shadow_blur / 4.),
                blur_radius: look.drag_shadow_blur,
                spread_radius: px(0.),
            },
            BoxShadow {
                color: look.drag_ring,
                offset: point(px(0.), px(0.)),
                blur_radius: px(crate::theme::RING_BLUR),
                spread_radius: px(1.),
            },
        ],
    );
    window.paint_quad(fill(drag.card, look.drag_fill).corner_radii(radius));
    for placed in &drag.lines {
        paint_line(placed, context, window, cx);
    }
    // The fade is the card's own fill laid thinly over its text, so the
    // card stays opaque.
    let veil = Hsla {
        a: 1. - look.drag_opacity,
        ..look.drag_fill
    };
    window.paint_quad(fill(drag.card, veil).corner_radii(radius));
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
        // Flush with the row: a gap would show a sliver of the line above,
        // such as the foot of its own rendered math, under the card.
        let y = placed.top + row.top - height;
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
    // The card covers part of the line above, so it takes the raised
    // popover colour and a ring. On the page's own colour it had no visible
    // edge, and whatever peeked out around it (that line's rendered math,
    // say) read as part of the preview.
    window.paint_quad(quad(
        bounds,
        radius,
        theme.popover,
        px(1.),
        theme.popover_ring,
        BorderStyle::default(),
    ));
}

fn report(result: anyhow::Result<()>) {
    if let Err(error) = result {
        eprintln!("paint failed: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_current_find_match_is_drawn_over_the_selection() {
        assert!(drawn_over_selection(HighlightKind::ActiveSearchMatch));
        for kind in [
            HighlightKind::SearchMatch,
            HighlightKind::FootnoteProblem,
            HighlightKind::TabStop,
        ] {
            assert!(!drawn_over_selection(kind));
        }
    }
}
