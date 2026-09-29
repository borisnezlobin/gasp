//! The copy button a code block shows at its top right while the pointer
//! is over it. A click copies the code (without its fences) and the
//! button says "Copied" with a check for a moment. `code.copy-block` does
//! the same for the block the cursor is in, so the keyboard has it too.
//!
//! The blocks on screen are found from the frame each paint, one syntax
//! tree lookup per block, and the pointer is checked against them only
//! when it moves; the editor redraws only when what's hovered changes.

use std::ops::Range;
use std::time::Duration;

use gasp_core::syntax::{NodeKind, SyntaxTree};
use gpui::{Bounds, ClipboardItem, Context, Pixels, Point, Task, point, size};

use crate::editor::EditorView;
use crate::frame::FrameLayout;
use crate::theme::Theme;

/// How long the button says "Copied".
pub const COPIED_FOR: Duration = Duration::from_millis(1500);

/// A code block with some of it on screen.
#[derive(Clone, Debug, PartialEq)]
pub struct CodeBlockOnScreen {
    /// The block's surface, in window coordinates.
    pub bounds: Bounds<Pixels>,
    /// Where the block starts in the document; it names the block.
    pub start: usize,
    /// The code between the fences.
    pub code: Range<usize>,
}

/// What the button shows and where.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CopyButton {
    pub bounds: Bounds<Pixels>,
    /// The pointer is on the button itself.
    pub pressed_on: bool,
    /// The code was just copied.
    pub copied: bool,
}

/// The copy button's state in an editor.
#[derive(Default)]
pub struct CodeCopy {
    blocks: Vec<CodeBlockOnScreen>,
    /// The editor's visible area, in window coordinates.
    viewport: Bounds<Pixels>,
    pointer: Option<Point<Pixels>>,
    /// The block whose code was just copied.
    copied: Option<usize>,
    reset: Option<Task<()>>,
}

/// The code between a block's fences, or `None` when the node isn't a
/// fenced code block.
fn code_range(tree: &SyntaxTree, start: usize) -> Option<Range<usize>> {
    let node = tree
        .path_at(start)
        .into_iter()
        .map(|id| tree.node(id))
        .find(|node| matches!(node.kind, NodeKind::CodeBlock(_)) && node.range.start == start)?;
    let open = node.markup.first()?;
    let code_start = (open.range.end + 1).min(node.range.end);
    let code_end = node
        .markup
        .get(1)
        .map_or(node.range.end, |close| close.range.start);
    Some(code_start..code_end.max(code_start))
}

/// The code blocks the frame shows, with the area each one's surface
/// covers on screen.
pub fn blocks_on_screen(frame: &FrameLayout, tree: &SyntaxTree) -> Vec<CodeBlockOnScreen> {
    let mut blocks: Vec<CodeBlockOnScreen> = Vec::new();
    let mut not_code: Vec<usize> = Vec::new();
    for placed in frame
        .lines
        .iter()
        .filter(|line| !line.visual.is_collapsed())
    {
        for surface in &placed.visual.decor.surfaces {
            if let Some(block) = blocks.iter_mut().find(|b| b.start == surface.group) {
                block.bounds.size.height = placed.bottom() - block.bounds.origin.y;
                continue;
            }
            if not_code.contains(&surface.group) {
                continue;
            }
            let Some(code) = code_range(tree, surface.group) else {
                not_code.push(surface.group);
                continue;
            };
            blocks.push(CodeBlockOnScreen {
                bounds: Bounds::new(
                    point(frame.text_left + surface.left, placed.top),
                    size(surface.width, placed.bottom() - placed.top),
                ),
                start: surface.group,
                code,
            });
        }
    }
    blocks
}

/// The copy and check icons' size: a touch over the small text beside
/// them, so the glyph holds its own on the code's fill.
pub fn copy_icon_size(theme: &Theme) -> Pixels {
    theme.small_font_size + theme.space_xs
}

/// The button's square, at the block's top right. It stays in view while
/// the block's top has scrolled off, until the block's bottom reaches it.
pub fn button_bounds(
    block: Bounds<Pixels>,
    viewport: Bounds<Pixels>,
    theme: &Theme,
) -> Bounds<Pixels> {
    let side = copy_icon_size(theme) + theme.space_sm * 3.;
    let margin = theme.space_sm;
    let lowest = block.bottom() - side - margin;
    let top = (block.top() + margin)
        .max(viewport.top() + margin)
        .min(lowest);
    Bounds::new(point(block.right() - margin - side, top), size(side, side))
}

/// How wide the button grows to say "Copied" beside its check.
pub fn copied_width(label_width: Pixels, theme: &Theme) -> Pixels {
    let side = copy_icon_size(theme) + theme.space_sm * 3.;
    side + label_width + theme.space_sm
}

impl CodeCopy {
    /// Whether the button could show: the pointer is over the editor or
    /// a block was just copied.
    pub fn is_active(&self) -> bool {
        self.pointer.is_some() || self.copied.is_some()
    }

    /// Takes the blocks the frame just laid out.
    pub fn set_blocks(&mut self, blocks: Vec<CodeBlockOnScreen>, viewport: Bounds<Pixels>) {
        self.blocks = blocks;
        self.viewport = viewport;
    }

    /// The block under the pointer, or the one just copied.
    fn shown_block(&self) -> Option<&CodeBlockOnScreen> {
        let hovered = self.pointer.and_then(|pointer| {
            self.blocks
                .iter()
                .find(|block| block.bounds.contains(&pointer))
        });
        hovered.or_else(|| {
            let copied = self.copied?;
            self.blocks.iter().find(|block| block.start == copied)
        })
    }

    /// The button to draw, if any.
    pub fn button(&self, theme: &Theme) -> Option<CopyButton> {
        let block = self.shown_block()?;
        let bounds = button_bounds(block.bounds, self.viewport, theme);
        Some(CopyButton {
            bounds,
            pressed_on: self
                .pointer
                .is_some_and(|pointer| bounds.contains(&pointer)),
            copied: self.copied == Some(block.start),
        })
    }

    /// The block whose button is under `position`.
    fn block_under_button(
        &self,
        position: Point<Pixels>,
        theme: &Theme,
    ) -> Option<&CodeBlockOnScreen> {
        let block = self.shown_block()?;
        button_bounds(block.bounds, self.viewport, theme)
            .contains(&position)
            .then_some(block)
    }

    /// Follows the pointer; says whether what's drawn changes.
    pub fn move_pointer(&mut self, position: Option<Point<Pixels>>, theme: &Theme) -> bool {
        let before = self.button(theme);
        self.pointer = position;
        self.button(theme) != before
    }
}

impl EditorView {
    /// Follows the pointer for the copy button, redrawing only when the
    /// button appears, goes or changes.
    pub(crate) fn hover_code(&mut self, position: Option<Point<Pixels>>, cx: &mut Context<Self>) {
        if let Some(frame) = &self.frame
            && position.is_some()
        {
            let blocks = blocks_on_screen(frame, self.source.tree());
            self.code_copy.set_blocks(blocks, frame.bounds);
        }
        if self.code_copy.move_pointer(position, &self.theme) {
            cx.notify();
        }
    }

    /// A click on a copy button copies its block. Says whether it was one.
    pub(crate) fn click_copy_button(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(block) = self
            .code_copy
            .block_under_button(position, &self.theme)
            .cloned()
        else {
            return false;
        };
        self.copy_code(block.start, block.code, cx);
        true
    }

    /// `code.copy-block`: copies the code of the block the cursor is in.
    pub fn copy_code_block_at_cursor(&mut self, cx: &mut Context<Self>) {
        let cursor = self.cursor();
        let tree = self.source.tree();
        let block = tree
            .path_at(cursor)
            .into_iter()
            .map(|id| tree.node(id))
            .find(|node| matches!(node.kind, NodeKind::CodeBlock(_)))
            .map(|node| node.range.start);
        let Some(start) = block else {
            return;
        };
        if let Some(code) = code_range(tree, start) {
            self.copy_code(start, code, cx);
        }
    }

    fn copy_code(&mut self, start: usize, code: Range<usize>, cx: &mut Context<Self>) {
        let text = self.source.text()[code].trim_end_matches('\n').to_owned();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.code_copy.copied = Some(start);
        self.code_copy.reset = Some(cx.spawn(async move |view, cx| {
            cx.background_executor().timer(COPIED_FOR).await;
            view.update(cx, |view, cx| {
                view.code_copy.copied = None;
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// Which block shows "Copied", for tests.
    pub fn copied_code_block(&self) -> Option<usize> {
        self.code_copy.copied
    }

    /// The copy button as last drawn, for tests.
    pub fn copy_button(&self) -> Option<CopyButton> {
        self.code_copy.button(&self.theme)
    }
}

#[cfg(test)]
mod tests {
    use gpui::px;

    use super::*;

    fn theme() -> Theme {
        Theme::default()
    }

    #[test]
    fn the_button_sits_top_right_and_stays_in_view() {
        let theme = theme();
        let viewport = Bounds::new(point(px(0.), px(0.)), size(px(800.), px(600.)));
        let block = Bounds::new(point(px(100.), px(50.)), size(px(500.), px(200.)));
        let button = button_bounds(block, viewport, &theme);
        assert_eq!(button.right(), block.right() - theme.space_sm);
        assert_eq!(button.top(), block.top() + theme.space_sm);
        // The block's top is above the viewport: the button stays in view.
        let scrolled = Bounds::new(point(px(100.), px(-120.)), size(px(500.), px(400.)));
        let button = button_bounds(scrolled, viewport, &theme);
        assert_eq!(button.top(), viewport.top() + theme.space_sm);
        // Until the block's bottom pushes it up.
        let leaving = Bounds::new(point(px(100.), px(-180.)), size(px(500.), px(200.)));
        let button = button_bounds(leaving, viewport, &theme);
        assert!(button.bottom() <= leaving.bottom());
    }
}
