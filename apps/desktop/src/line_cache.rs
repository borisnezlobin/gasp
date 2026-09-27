//! Laid-out lines kept between frames, so a keystroke lays out the line
//! it changed and reuses the rest of the screen.
//!
//! A line is found by what its layout depends on: its text, its plan with
//! offsets counted from the line's start (so an edit above it doesn't
//! matter), its frame (insets, padding, fills and line number, which read
//! the lines around it), and its code colours. Everything a frame shares —
//! theme, zoom, column width, scale — clears the cache when it changes.
//! Lines whose look arrives later, such as images and rendered math, are
//! laid out every frame as before.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use editor_core::render::{LinePlan, WidgetKind};
use gpui::{Hsla, Pixels};

use crate::line_layout::{Hit, LineDecor, Surface, VisualLine};
use crate::preview::code_highlight::LineSpans;
use crate::preview::decor::LineFrame;

/// Lines kept before the cache starts over; far more than a screen.
const LIMIT: usize = 2048;

/// What every cached line was laid out against.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayoutEpoch {
    pub column_width: Pixels,
    pub zoom: f32,
    pub scale_factor: f32,
    pub code_line_numbers: bool,
}

struct Entry {
    text: String,
    widgets: Vec<WidgetKind>,
    visual: VisualLine,
}

/// Laid-out lines by what they depend on.
#[derive(Default)]
pub struct LineCache {
    epoch: LayoutEpoch,
    lines: HashMap<u64, Entry>,
}

/// A hash of what a line's layout depends on. A hit also checks the
/// line's text and widgets, so a collision can't show the wrong line.
pub struct LineKey {
    hash: u64,
}

impl LineCache {
    /// Forgets every line, as when the theme changes.
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    /// Starts over when what lines are laid out against changed.
    pub fn begin_frame(&mut self, epoch: LayoutEpoch) {
        if epoch != self.epoch {
            self.epoch = epoch;
            self.lines.clear();
        }
    }

    /// The key for a planned line of `text`, or `None` when the line
    /// can't be cached.
    pub fn key(
        plan: &LinePlan,
        text: &str,
        frame: &LineFrame,
        spans: Option<&LineSpans>,
    ) -> Option<LineKey> {
        if !plan.widgets.iter().all(|widget| is_settled(&widget.kind)) {
            return None;
        }
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        hash_plan(plan, &mut hasher);
        hash_frame(frame, &mut hasher);
        spans.map(|spans| &spans[..]).hash(&mut hasher);
        Some(LineKey {
            hash: hasher.finish(),
        })
    }

    /// The cached layout for `key`, moved to where `plan` puts the line
    /// and given `frame`'s fills.
    pub fn get(
        &self,
        key: &LineKey,
        plan: &LinePlan,
        text: &str,
        frame: &LineFrame,
    ) -> Option<VisualLine> {
        let entry = self.lines.get(&key.hash)?;
        let same = entry.text == text
            && entry
                .widgets
                .iter()
                .eq(plan.widgets.iter().map(|widget| &widget.kind));
        if !same {
            return None;
        }
        let mut visual = entry.visual.clone();
        rebase(&mut visual, plan);
        visual.decor.surfaces.clone_from(&frame.decor.surfaces);
        visual.decor.bands.clone_from(&frame.decor.bands);
        Some(visual)
    }

    pub fn insert(&mut self, key: LineKey, plan: &LinePlan, text: &str, visual: &VisualLine) {
        if self.lines.len() >= LIMIT {
            self.lines.clear();
        }
        let entry = Entry {
            text: text.to_owned(),
            widgets: plan
                .widgets
                .iter()
                .map(|widget| widget.kind.clone())
                .collect(),
            visual: visual.clone(),
        };
        self.lines.insert(key.hash, entry);
    }
}

/// Whether a widget looks the same every time it's laid out. Images,
/// math and link cards load in the background, so their lines aren't
/// cached. Neither are tables: their cells, which may hold math, are on
/// lines the key doesn't read.
fn is_settled(kind: &WidgetKind) -> bool {
    !matches!(
        kind,
        WidgetKind::InlineMath { .. }
            | WidgetKind::MathBlock { .. }
            | WidgetKind::MathPreview { .. }
            | WidgetKind::Image { .. }
            | WidgetKind::LinkCard(_)
            | WidgetKind::Table { .. }
    )
}

/// Hashes a plan with its offsets counted from the line's start.
fn hash_plan(plan: &LinePlan, hasher: &mut DefaultHasher) {
    let start = plan.range.start;
    let relative = |range: &std::ops::Range<usize>| (range.start - start, range.end - start);
    plan.range.len().hash(hasher);
    plan.collapsed.hash(hasher);
    plan.line_styles.hash(hasher);
    for run in &plan.runs {
        relative(&run.range).hash(hasher);
        run.styles.hash(hasher);
    }
    for hidden in &plan.hidden {
        relative(hidden).hash(hasher);
    }
    for widget in &plan.widgets {
        // Widgets can reach past the line, as a table does.
        (widget.range.start.wrapping_sub(start), widget.range.len()).hash(hasher);
        (widget.placement as u8).hash(hasher);
    }
}

/// Hashes everything in a frame but where its fills' blocks start, which
/// moves with edits above and only matters for painting.
fn hash_frame(frame: &LineFrame, hasher: &mut DefaultHasher) {
    for pixels in [frame.left, frame.right, frame.pad_top, frame.pad_bottom] {
        hash_pixels(pixels, hasher);
    }
    frame.line_number.hash(hasher);
    hash_decor(&frame.decor, hasher);
}

fn hash_decor(decor: &LineDecor, hasher: &mut DefaultHasher) {
    hash_pixels(decor.margin_top, hasher);
    for surface in decor.surfaces.iter().chain(&decor.bands) {
        hash_surface(surface, hasher);
    }
    decor.bands.len().hash(hasher);
    for bar in &decor.bars {
        hash_pixels(bar.x, hasher);
        hash_pixels(bar.width, hasher);
        hash_color(bar.color, hasher);
    }
    decor.gutter.len().hash(hasher);
}

fn hash_surface(surface: &Surface, hasher: &mut DefaultHasher) {
    hash_pixels(surface.left, hasher);
    hash_pixels(surface.width, hasher);
    hash_color(surface.color, hasher);
}

fn hash_pixels(pixels: Pixels, hasher: &mut DefaultHasher) {
    f32::from(pixels).to_bits().hash(hasher);
}

fn hash_color(color: Hsla, hasher: &mut DefaultHasher) {
    for part in [color.h, color.s, color.l, color.a] {
        part.to_bits().hash(hasher);
    }
}

/// Moves a cached line to where `plan` puts it: its number, its start,
/// and the note offsets its controls act on.
fn rebase(visual: &mut VisualLine, plan: &LinePlan) {
    let (from, to) = (visual.start, plan.range.start);
    visual.line = plan.line;
    visual.start = to;
    if from == to {
        return;
    }
    let pieces = visual
        .rows
        .iter_mut()
        .flat_map(|row| row.pieces.iter_mut())
        .chain(visual.decor.gutter.iter_mut());
    for piece in pieces {
        match &mut piece.hit {
            Hit::Checkbox { marker } => {
                *marker = marker.start - from + to..marker.end - from + to;
            }
            Hit::Fold { header, .. } => *header = *header - from + to,
            _ => {}
        }
    }
}
