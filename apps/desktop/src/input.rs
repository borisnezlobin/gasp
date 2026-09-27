//! Platform text input, including IME composition, through GPUI's
//! `EntityInputHandler`. GPUI speaks UTF-16 offsets here.

use std::ops::Range;

use gpui::{Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window};

use crate::editor::EditorView;
use crate::text_offsets::{offset_to_utf16, range_from_utf16, range_to_utf16};

impl EditorView {
    /// The byte range an input method edit applies to: the given UTF-16
    /// range, else the composition, else the selection.
    fn input_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        range_utf16
            .map(|range| range_from_utf16(self.doc(), &range))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selected_range())
    }
}

impl EntityInputHandler for EditorView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let doc = self.doc();
        let range = range_from_utf16(doc, &range_utf16);
        adjusted_range.replace(range_to_utf16(doc, &range));
        Some(doc.slice(range))
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: range_to_utf16(self.doc(), &self.selected_range()),
            reversed: self.cursor() < self.anchor(),
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked
            .as_ref()
            .map(|range| range_to_utf16(self.doc(), range))
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.input_range(range_utf16);
        self.replace(range, text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.input_range(range_utf16);
        let inserted = self.replace(range, new_text, cx);
        self.marked = (!inserted.is_empty()).then(|| inserted.clone());
        let Some(selected_utf16) = new_selected_range_utf16 else {
            return;
        };
        let base = offset_to_utf16(self.doc(), inserted.start);
        let selected = range_from_utf16(
            self.doc(),
            &(base + selected_utf16.start..base + selected_utf16.end),
        );
        let selected = selected.start.min(inserted.end)..selected.end.min(inserted.end);
        self.select(selected.start, selected.end, cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = range_from_utf16(self.doc(), &range_utf16);
        self.frame.as_ref()?.range_bounds(&range)
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let frame = self.frame.as_ref()?;
        let placed = frame.line_at_y(point.y)?;
        let offset = placed.visual.start + placed.visual.offset_for_x(point.x - frame.text_left);
        Some(offset_to_utf16(self.doc(), offset))
    }
}
