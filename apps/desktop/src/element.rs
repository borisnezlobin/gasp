//! The custom GPUI element that lays out and paints the editor's visible
//! lines, and records how long that takes.

use std::time::Instant;

use gpui::{
    App, Bounds, ContentMask, Element, ElementId, ElementInputHandler, Entity, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Style, Window, fill, point, relative, size,
};

use crate::editor::EditorView;
use crate::frame::{FrameLayout, PlacedLine};
use crate::line_layout::PieceContent;
use crate::theme::Theme;

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
        self.view.update(cx, |view, _| {
            let frame = view.layout_frame(bounds, window);
            let selection = frame.selection_rects(&view.selected_range(), &view.theme);
            let caret = frame.caret_bounds(view.cursor(), &view.theme);
            view.timings.layout.push(started.elapsed());
            Prepainted {
                frame,
                selection,
                caret,
                theme: view.theme.clone(),
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

fn paint_contents(prepainted: &Prepainted, focused: bool, window: &mut Window, cx: &mut App) {
    let theme = &prepainted.theme;
    for rect in &prepainted.selection {
        window.paint_quad(fill(*rect, theme.selection));
    }
    for placed in &prepainted.frame.lines {
        paint_line(placed, prepainted.frame.text_left, theme, window, cx);
    }
    if let Some(caret) = prepainted.caret.filter(|_| focused) {
        window.paint_quad(fill(caret, theme.cursor));
    }
}

fn paint_line(
    placed: &PlacedLine,
    text_left: Pixels,
    theme: &Theme,
    window: &mut Window,
    cx: &mut App,
) {
    let visual = &placed.visual;
    for piece in &visual.pieces {
        let left = text_left + piece.x;
        match &piece.content {
            PieceContent::Text(shaped) => {
                let origin = point(left, placed.text_top());
                report(shaped.paint_background(origin, visual.text_height, window, cx));
                report(shaped.paint(origin, visual.text_height, window, cx));
            }
            PieceContent::Image(image) => {
                let top = placed.top + (visual.height - piece.height) / 2.;
                let image_bounds = Bounds::new(
                    point(left, top),
                    size(piece.width - theme.image_gap, piece.height),
                );
                report(window.paint_image(
                    image_bounds,
                    theme.image_corner_radius.into(),
                    image.clone(),
                    0,
                    false,
                ));
            }
        }
    }
}

fn report(result: anyhow::Result<()>) {
    if let Err(error) = result {
        eprintln!("paint failed: {error}");
    }
}
