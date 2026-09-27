//! Look up: the system dictionary's popover for a word, which macOS shows
//! on a force click or three-finger tap, on `edit.look-up` (Ctrl+Cmd+D)
//! and from the note's right-click menu. Working out which text and where
//! it's drawn happens here on every platform; [`macos`] hears the gesture
//! and shows the popover.

use std::ops::Range;

use editor_core::document::Document;
use editor_core::motion;
use gpui::{App, FontId, LineLayout, Pixels, Point, SharedString, Window, point};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::editor::EditorView;
use crate::frame::FrameLayout;
use crate::line_layout::{PieceContent, TextPiece};
use crate::workspace::Workspace;

#[cfg(target_os = "macos")]
mod macos;

/// What Look up shows, and where the text it shows is drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct LookUp {
    pub range: Range<usize>,
    pub text: String,
    /// Window position of the text's baseline at its left edge: the
    /// popover draws the text again there, highlighted.
    pub baseline: Point<Pixels>,
    pub font_family: SharedString,
    pub font_size: Pixels,
}

/// What a look-up at `offset` means: the selection when `offset` is in
/// it, otherwise the word at `offset` or the one ending there, so the
/// caret just after a word finds it. None over spaces and punctuation.
pub fn range_to_look_up(
    doc: &Document,
    selection: Range<usize>,
    offset: usize,
) -> Option<Range<usize>> {
    if !selection.is_empty() && selection.start <= offset && offset <= selection.end {
        return Some(selection);
    }
    [offset, doc.prev_char_boundary(offset)]
        .into_iter()
        .map(|at| motion::word_at(doc, at))
        .find(|word| doc.slice(word.clone()).chars().any(char::is_alphanumeric))
}

/// Where the text at `offset` is drawn: its baseline's window position at
/// that offset, the shaped text there and the byte of it the offset
/// falls on. None when its line is off screen or it's drawn as a widget.
fn drawn_at(frame: &FrameLayout, offset: usize) -> Option<(Point<Pixels>, &TextPiece, usize)> {
    let placed = frame.line_containing(offset)?;
    let visual = &placed.visual;
    let relative = offset - visual.start;
    let row = &visual.rows[visual.row_for_offset(relative)?];
    // Hidden markup before the text, such as `**`, has no piece.
    let piece = row
        .pieces
        .iter()
        .find(|piece| piece.is_text() && relative < piece.range.end)?;
    let PieceContent::Text(text) = &piece.content else {
        return None;
    };
    // As GPUI paints a shaped line: centred in its line height.
    let shaped = &text.shaped;
    let baseline = (text.line_height - shaped.ascent - shaped.descent) / 2. + shaped.ascent;
    let at = point(
        frame.text_left + row.x_for(relative),
        placed.top + row.top + piece.top + baseline,
    );
    let index = text.slice.start + relative.saturating_sub(piece.range.start);
    Some((at, text, index))
}

/// The font of the shaped run the byte at `index` is in.
fn font_at(layout: &LineLayout, index: usize) -> Option<FontId> {
    layout
        .runs
        .iter()
        .rev()
        .find(|run| run.glyphs.first().is_some_and(|glyph| glyph.index <= index))
        .or(layout.runs.first())
        .map(|run| run.font_id)
}

impl EditorView {
    /// What a force click or tap at window `position` looks up, when it
    /// lands on this editor's text.
    pub fn look_up_at_point(&self, position: Point<Pixels>, cx: &App) -> Option<LookUp> {
        let frame = self.frame.as_ref()?;
        let (_, piece) = frame.piece_at(position)?;
        if !frame.bounds.contains(&position) || !piece.is_text() {
            return None;
        }
        self.look_up_at_offset(frame.offset_at(position)?, cx)
    }

    /// What `edit.look-up` looks up: the selection, or the word at the
    /// caret.
    pub fn look_up_at_cursor(&self, cx: &App) -> Option<LookUp> {
        self.look_up_at_offset(self.cursor(), cx)
    }

    fn look_up_at_offset(&self, offset: usize, cx: &App) -> Option<LookUp> {
        let range = range_to_look_up(self.doc(), self.selected_range(), offset)?;
        let (baseline, text, index) = drawn_at(self.frame.as_ref()?, range.start)?;
        let font = font_at(&text.shaped, index).and_then(|id| cx.text_system().get_font_for_id(id));
        Some(LookUp {
            text: self.doc().slice(range.clone()),
            range,
            baseline,
            font_family: font.map(|font| font.family).unwrap_or_default(),
            font_size: text.shaped.font_size,
        })
    }

    /// Shows Look up for the word at `position`, or else for the
    /// selection or the word at the caret. Answers whether it had
    /// something to show.
    pub fn look_up(&self, position: Option<Point<Pixels>>, window: &Window, cx: &App) -> bool {
        let found = position
            .and_then(|position| self.look_up_at_point(position, cx))
            .or_else(|| self.look_up_at_cursor(cx));
        found.is_some_and(|found| show(&found, window))
    }
}

impl Workspace {
    /// What a force click or tap at window `position` looks up, in
    /// whichever pane's note is there.
    pub fn look_up_at_point(&self, position: Point<Pixels>, cx: &App) -> Option<LookUp> {
        self.panes()
            .iter()
            .filter_map(|pane| pane.read(cx).active_editor())
            .find_map(|editor| editor.read(cx).look_up_at_point(position, cx))
    }
}

/// The address of the native view `window` draws into, which AppKit
/// names when a gesture lands on it. None off macOS.
pub fn native_view(window: &Window) -> Option<usize> {
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::AppKit(handle) => Some(handle.ns_view.as_ptr() as usize),
        _ => None,
    }
}

/// Shows Look up for a gesture at `at` on the native view at `view`, in
/// whichever workspace window draws into it. `at` is in the view's
/// coordinates, whose y runs up from its bottom edge.
pub fn look_up_in_window(view: usize, at: (f64, f64), cx: &mut App) {
    for handle in cx.windows() {
        let Some(workspace) = handle.downcast::<Workspace>() else {
            continue;
        };
        let _ = workspace.update(cx, |workspace, window, cx| {
            if native_view(window) != Some(view) {
                return;
            }
            let height = f64::from(window.viewport_size().height);
            let position = point(Pixels::from(at.0), Pixels::from(height - at.1));
            if let Some(found) = workspace.look_up_at_point(position, cx) {
                show(&found, window);
            }
        });
    }
}

/// Hears force clicks and three-finger taps on every window from now on.
#[cfg(target_os = "macos")]
pub fn install(cx: &mut App) {
    use futures::StreamExt;

    let (sender, mut gestures) = futures::channel::mpsc::unbounded();
    if !macos::listen(sender) {
        return;
    }
    cx.spawn(async move |cx| {
        while let Some(gesture) = gestures.next().await {
            let at = (gesture.x, gesture.y);
            if cx
                .update(|cx| look_up_in_window(gesture.view, at, cx))
                .is_err()
            {
                break;
            }
        }
    })
    .detach();
}

#[cfg(target_os = "macos")]
fn show(found: &LookUp, window: &Window) -> bool {
    let Some(view) = native_view(window) else {
        return false;
    };
    // The view's y runs up from its bottom edge.
    let height = window.viewport_size().height;
    let definition = macos::Definition {
        text: &found.text,
        font_family: &found.font_family,
        font_size: f64::from(found.font_size),
        baseline: (
            f64::from(found.baseline.x),
            f64::from(height - found.baseline.y),
        ),
    };
    macos::show_definition(view, &definition);
    true
}

/// Only macOS has the popover.
#[cfg(not(target_os = "macos"))]
fn show(_: &LookUp, _: &Window) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str) -> Document {
        Document::from(text)
    }

    #[test]
    fn finds_the_word_at_an_offset() {
        let doc = doc("say hello, world");
        assert_eq!(range_to_look_up(&doc, 0..0, 5), Some(4..9));
        assert_eq!(range_to_look_up(&doc, 0..0, 4), Some(4..9));
    }

    #[test]
    fn the_caret_just_after_a_word_finds_it() {
        let doc = doc("say hello, world");
        assert_eq!(range_to_look_up(&doc, 9..9, 9), Some(4..9));
        assert_eq!(range_to_look_up(&doc, 0..0, 16), Some(11..16));
    }

    #[test]
    fn spaces_and_punctuation_find_nothing() {
        let doc = doc("a  --  b");
        assert_eq!(range_to_look_up(&doc, 0..0, 3), None);
        assert_eq!(range_to_look_up(&doc, 0..0, 5), None);
        assert_eq!(range_to_look_up(&Document::from(""), 0..0, 0), None);
    }

    #[test]
    fn inside_the_selection_the_selection_is_looked_up() {
        let doc = doc("the Rosetta Stone here");
        assert_eq!(range_to_look_up(&doc, 4..17, 8), Some(4..17));
        assert_eq!(range_to_look_up(&doc, 4..17, 17), Some(4..17));
        assert_eq!(range_to_look_up(&doc, 4..17, 19), Some(18..22));
    }

    #[test]
    fn words_are_unicode() {
        let doc = doc("über café");
        assert_eq!(range_to_look_up(&doc, 0..0, 7), Some(6..11));
    }
}
