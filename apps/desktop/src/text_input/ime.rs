//! Platform text input for [`TextInput`]: typed text, IME composition and
//! the candidate window's position. Platforms count in UTF-16 code units;
//! the input stores byte offsets.

use std::ops::Range;

use gasp_core::document::Document;
use gpui::{Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window, point};

use super::TextInput;
use super::state::EditKind;
use crate::text_offsets::{offset_to_utf16, range_from_utf16, range_to_utf16};

impl TextInput {
    fn doc(&self) -> Document {
        Document::from(self.state.text())
    }

    /// The byte range an input method edit applies to: the one it names,
    /// else the composition, else the selection.
    fn input_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        range_utf16
            .map(|range| range_from_utf16(&self.doc(), &range))
            .or_else(|| self.state.marked())
            .unwrap_or_else(|| self.state.selected_range())
    }

    /// The x of byte `offset` in the painted line.
    fn x_for_offset(&self, offset: usize) -> Option<Pixels> {
        let painted = self.painted.as_ref()?;
        let x = painted.line.as_ref().map_or(Pixels::ZERO, |line| {
            line.x_for_index(self.shown_offset(offset))
        });
        Some(painted.origin.x + x)
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let doc = self.doc();
        let range = range_from_utf16(&doc, &range_utf16);
        adjusted_range.replace(range_to_utf16(&doc, &range));
        Some(self.state.text()[range].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: range_to_utf16(&self.doc(), &self.state.selected_range()),
            reversed: self.state.is_reversed(),
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.state
            .marked()
            .map(|range| range_to_utf16(&self.doc(), &range))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.state.unmark();
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.input_range(range_utf16);
        let changed = self.state.edit(range, text, EditKind::Typing).changed;
        self.after_edit(changed, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.input_range(range_utf16);
        let before = self.state.text().to_owned();
        // The selection comes in UTF-16 units within the new text.
        let composed = Document::from(new_text);
        let selected = new_selected_range_utf16.map(|range| range_from_utf16(&composed, &range));
        self.state.compose(range, new_text, selected);
        self.after_edit(self.state.text() != before, cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = range_from_utf16(&self.doc(), &range_utf16);
        Some(Bounds::from_corners(
            point(self.x_for_offset(range.start)?, bounds.top()),
            point(self.x_for_offset(range.end)?, bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        self.painted.as_ref()?;
        let offset = self.offset_for_position(point);
        Some(offset_to_utf16(&self.doc(), offset))
    }
}
