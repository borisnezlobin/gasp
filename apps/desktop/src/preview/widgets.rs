//! Pieces and rows for the planner's widgets: math, images, checkboxes,
//! bullets, footnote marks, callout headers, rules, code block headers and
//! tables.

use std::ops::Range;
use std::sync::Arc;

use editor_core::render::{StyleKey, WidgetKind};
use editor_core::syntax::{CalloutKind, Fold};
use gpui::{Pixels, RenderImage, SharedString, px};

use crate::icons::IconName;
use crate::images::display_size;
use crate::line_layout::{Hit, Overlay, Piece, PieceContent, RowKind, VisualRow};
use crate::preview::items::{Attached, LineItems};
use crate::preview::layout::LineLayouter;
use crate::preview::math::{MathImage, MathKey, MathState};
use crate::preview::wrap::{Chunk, Extent, RowBuilder};
use crate::styling::text_run;

const CALLOUT_ICONS: [(CalloutKind, IconName); 14] = [
    (CalloutKind::Note, IconName::PencilSimple),
    (CalloutKind::Abstract, IconName::ClipboardText),
    (CalloutKind::Info, IconName::Info),
    (CalloutKind::Todo, IconName::CheckCircle),
    (CalloutKind::Tip, IconName::Flame),
    (CalloutKind::Success, IconName::Check),
    (CalloutKind::Question, IconName::Question),
    (CalloutKind::Warning, IconName::Warning),
    (CalloutKind::Failure, IconName::X),
    (CalloutKind::Danger, IconName::Lightning),
    (CalloutKind::Bug, IconName::Bug),
    (CalloutKind::Example, IconName::List),
    (CalloutKind::Quote, IconName::Quotes),
    (CalloutKind::Custom, IconName::PencilSimple),
];

pub fn callout_icon(kind: CalloutKind) -> IconName {
    CALLOUT_ICONS
        .iter()
        .find(|(candidate, _)| *candidate == kind)
        .map_or(IconName::PencilSimple, |(_, icon)| *icon)
}

fn blank_piece(range: Range<usize>, width: Pixels, height: Pixels, content: PieceContent) -> Piece {
    Piece {
        range,
        x: px(0.),
        top: px(0.),
        width,
        height,
        content,
        hit: Hit::Widget,
    }
}

impl LineLayouter<'_, '_> {
    fn absolute(&self, range: &Range<usize>) -> Range<usize> {
        self.plan.range.start + range.start..self.plan.range.start + range.end
    }

    fn math(&mut self, tex: &str, display: bool, font_size: Pixels) -> MathState {
        let key = MathKey::new(
            tex,
            display,
            font_size,
            self.context.scale_factor,
            self.theme().text,
        );
        self.resources.math.lookup(key)
    }

    pub(super) fn place_inline(
        &mut self,
        range: &Range<usize>,
        kind: &WidgetKind,
        builder: &mut RowBuilder,
    ) {
        match kind {
            WidgetKind::InlineMath { tex, .. } => self.inline_math(range, tex, builder),
            WidgetKind::Image {
                target,
                width,
                height,
                ..
            } => {
                let image = self.resources.images.image(target);
                let limit = builder.limit() - builder.x().min(builder.limit());
                let piece = self.image_piece(range, image, (*width, *height), limit);
                let extent = Extent::on_baseline(piece.height);
                builder.push_atomic(piece, extent);
                builder.advance(self.theme().image_gap);
            }
            WidgetKind::Checkbox { checked } => self.checkbox(range, *checked, builder),
            WidgetKind::ListBullet {
                ordered, number, ..
            } => self.bullet(range, ordered.then_some(number.unwrap_or(1)), builder),
            WidgetKind::FootnoteSuperscript { label } => self.superscript(range, label, builder),
            WidgetKind::CalloutHeader {
                kind,
                title,
                default_title,
                fold,
                folded,
                ..
            } => {
                let header = CalloutHeader {
                    kind: *kind,
                    default_title: title.is_none().then_some(default_title.as_str()),
                    fold: *fold,
                    folded: *folded,
                };
                self.callout_header(range, &header, builder)
            }
            _ => {}
        }
    }

    fn inline_math(&mut self, range: &Range<usize>, tex: &str, builder: &mut RowBuilder) {
        let font_size = self.font_size();
        match self.math(tex, false, font_size) {
            MathState::Ready(image) => {
                let extent = Extent {
                    ascent: image.baseline,
                    descent: image.height - image.baseline,
                };
                builder.push_atomic(self.math_piece(range, &image), extent);
            }
            MathState::Pending => self.source_text(range, &[StyleKey::MathSource], None, builder),
            MathState::Failed(_) => {
                let error = self.theme().error;
                self.source_text(range, &[StyleKey::MathSource], Some(error), builder)
            }
        }
    }

    /// Shows a widget's source as styled text, as while its math renders.
    fn source_text(
        &mut self,
        range: &Range<usize>,
        styles: &[StyleKey],
        color: Option<gpui::Hsla>,
        builder: &mut RowBuilder,
    ) {
        let theme = self.theme();
        let mut run = text_run(range.len(), styles, &self.tone, false, theme);
        if let Some(color) = color {
            run.color = color;
        }
        let font_size = self.font_size() * theme.code_scale;
        let chunk = Chunk {
            range: range.clone(),
            text: self.text[range.clone()].replace(['\t', '\n'], " "),
            font_size,
            line_height: self.line_height(),
            runs: vec![run],
        };
        builder.push_chunk(&chunk, &self.shaper());
    }

    fn math_piece(&self, range: &Range<usize>, image: &MathImage) -> Piece {
        blank_piece(
            range.clone(),
            image.width,
            image.height,
            PieceContent::Image {
                image: image.image.clone(),
                radius: px(0.),
            },
        )
    }

    fn image_piece(
        &self,
        range: &Range<usize>,
        image: Arc<RenderImage>,
        requested: (Option<u32>, Option<u32>),
        max_width: Pixels,
    ) -> Piece {
        let theme = self.theme();
        let shown = display_size(&image, requested, self.context.zoom, max_width);
        blank_piece(
            range.clone(),
            shown.width,
            shown.height,
            PieceContent::Image {
                image,
                radius: theme.image_corner_radius,
            },
        )
    }

    fn marker_slot(&self) -> Pixels {
        self.theme().list_marker_width * (self.font_size() / self.theme().body_font_size)
    }

    fn checkbox(&mut self, range: &Range<usize>, checked: bool, builder: &mut RowBuilder) {
        let theme = self.theme();
        let size = self.font_size();
        let (icon, color) = if checked {
            (IconName::CheckSquare, theme.accent)
        } else {
            (IconName::Square, theme.text_muted)
        };
        let mut piece = blank_piece(
            range.clone(),
            self.marker_slot(),
            size,
            PieceContent::Icon {
                path: icon.path(),
                color,
            },
        );
        piece.hit = Hit::Checkbox {
            marker: self.absolute(range),
        };
        let extent = Extent {
            ascent: size * 0.85,
            descent: size * 0.15,
        };
        builder.push_atomic(piece, extent);
    }

    fn bullet(&mut self, range: &Range<usize>, number: Option<u64>, builder: &mut RowBuilder) {
        let theme = self.theme();
        let slot = self.marker_slot();
        let Some(number) = number else {
            let size = theme.bullet_size * (self.font_size() / theme.body_font_size);
            let piece = blank_piece(
                range.clone(),
                size,
                size,
                PieceContent::Quad {
                    color: theme.text_muted,
                    radius: size / 2.,
                },
            );
            let lift = self.font_size() * 0.3;
            let extent = Extent {
                ascent: lift + size / 2.,
                descent: size / 2. - lift,
            };
            let side = (slot - size) / 2.;
            builder.advance(side);
            builder.push_atomic(piece, extent);
            return builder.advance(side);
        };
        let run = text_run(1, &[StyleKey::MarkupDimmed], &self.tone, false, theme);
        let (mut piece, extent) = self.label(
            &format!("{number}."),
            run,
            self.font_size(),
            self.line_height(),
        );
        piece.range = range.clone();
        piece.width = piece.width.max(slot);
        builder.push_atomic(piece, extent);
    }

    fn superscript(&mut self, range: &Range<usize>, label: &str, builder: &mut RowBuilder) {
        let theme = self.theme();
        let size = self.font_size() * 0.7;
        let run = text_run(1, &[StyleKey::FootnoteRef], &self.tone, false, theme);
        let (mut piece, mut extent) = self.label(label, run, size, size);
        let raise = self.font_size() * 0.4;
        extent.ascent += raise;
        extent.descent -= raise;
        piece.range = range.clone();
        piece.width += theme.space_xs;
        builder.push_atomic(piece, extent);
    }

    fn callout_header(
        &mut self,
        range: &Range<usize>,
        header: &CalloutHeader<'_>,
        builder: &mut RowBuilder,
    ) {
        let theme = self.theme();
        let color = theme.callout_color(header.kind);
        let size = theme.icon_size * (self.font_size() / theme.body_font_size);
        let hit = match header.fold {
            Some(_) => Hit::Fold {
                header: self.absolute(range).start,
                folded: header.folded,
            },
            None => Hit::Widget,
        };
        let icon_extent = Extent {
            ascent: size * 0.85,
            descent: size * 0.15,
        };
        let mut icon = blank_piece(
            range.clone(),
            size + theme.space_md,
            size,
            PieceContent::Icon {
                path: callout_icon(header.kind).path(),
                color,
            },
        );
        icon.hit = hit.clone();
        builder.push_atomic(icon, icon_extent);
        if let Some(title) = header.default_title {
            let tone = crate::styling::LineTone {
                callout: Some(header.kind),
                ..self.tone
            };
            let run = text_run(1, &[StyleKey::CalloutTitle], &tone, false, theme);
            let (mut piece, extent) = self.label(title, run, self.font_size(), self.line_height());
            piece.range = range.clone();
            piece.hit = hit.clone();
            builder.push_atomic(piece, extent);
        }
        if header.fold.is_some() {
            let caret = if header.folded {
                IconName::CaretRight
            } else {
                IconName::CaretDown
            };
            let mut piece = blank_piece(
                range.clone(),
                size,
                size,
                PieceContent::Icon {
                    path: caret.path(),
                    color: theme.text_muted,
                },
            );
            piece.x = builder.limit() - size;
            piece.hit = hit;
            builder.push_overlapping(piece, icon_extent);
        }
    }

    pub(super) fn block_row(
        &mut self,
        range: &Range<usize>,
        kind: &WidgetKind,
        builder: &RowBuilder,
    ) -> VisualRow {
        let left = self.frame.left;
        let width = builder.limit() - left;
        let pieces = match kind {
            WidgetKind::HorizontalRule => self.rule(range, left, width),
            WidgetKind::MathBlock { tex }
            | WidgetKind::InlineMath { tex, .. }
            | WidgetKind::MathPreview { tex, .. } => self.math_block(range, tex, left, width),
            WidgetKind::CodeBlock { title, .. } => self.code_header(range, title.as_deref(), left),
            WidgetKind::LinkCard(card) => self.link_card(range, card, left, width),
            WidgetKind::Table { alignments, rows } => self.table(
                range,
                &super::table::TableSpec { alignments, rows },
                left,
                width,
            ),
            WidgetKind::Image {
                target,
                width: w,
                height: h,
                ..
            } => {
                let image = self.resources.images.image(target);
                let mut piece = self.image_piece(range, image, (*w, *h), width);
                piece.x = left;
                piece.top = self.theme().image_gap;
                vec![piece]
            }
            _ => Vec::new(),
        };
        self.block_from_pieces(RowKind::Block, range, pieces)
    }

    /// A row holding `pieces`, as tall as its tallest piece plus padding.
    pub(super) fn block_from_pieces(
        &self,
        kind: RowKind,
        range: &Range<usize>,
        pieces: Vec<Piece>,
    ) -> VisualRow {
        let bottom = pieces
            .iter()
            .map(|piece| piece.top + piece.height)
            .fold(px(0.), Pixels::max);
        let height = bottom.max(self.theme().space_md);
        VisualRow {
            kind,
            top: px(0.),
            height,
            range: range.clone(),
            soft_end: range.start,
            caret_top: px(0.),
            caret_height: height.min(self.line_height()),
            left: self.frame.left,
            pieces,
        }
    }

    fn rule(&self, range: &Range<usize>, left: Pixels, width: Pixels) -> Vec<Piece> {
        let theme = self.theme();
        let height = self.line_height();
        let mut rule = blank_piece(
            range.clone(),
            width,
            theme.rule_thickness,
            PieceContent::Quad {
                color: theme.divider,
                radius: px(0.),
            },
        );
        rule.x = left;
        rule.top = (height - theme.rule_thickness) / 2.;
        let mut spacer = blank_piece(
            range.clone(),
            px(0.),
            height,
            PieceContent::Quad {
                color: gpui::transparent_black(),
                radius: px(0.),
            },
        );
        spacer.x = left;
        vec![spacer, rule]
    }

    fn math_block(
        &mut self,
        range: &Range<usize>,
        tex: &str,
        left: Pixels,
        width: Pixels,
    ) -> Vec<Piece> {
        let theme = self.theme();
        let padding = theme.space_md;
        match self.math(tex, true, theme.body_font_size) {
            MathState::Ready(image) => {
                let mut piece = self.math_piece(range, &image);
                piece.x = left + ((width - image.width) / 2.).max(px(0.));
                piece.top = padding;
                let mut pieces = vec![piece];
                pieces.push(self.spacer(range, left, image.height + padding * 2.));
                pieces
            }
            MathState::Pending => vec![self.spacer(range, left, self.line_height() + padding * 2.)],
            MathState::Failed(_) => {
                let mut run = text_run(1, &[StyleKey::MathSource], &self.tone, false, theme);
                run.color = theme.error;
                let tex = tex.replace('\n', " ");
                let (mut piece, _) = self.label(
                    &tex,
                    run,
                    self.font_size() * theme.code_scale,
                    self.line_height(),
                );
                piece.range = range.clone();
                piece.x = left;
                vec![piece]
            }
        }
    }

    /// An invisible piece that gives a block row its height.
    pub(super) fn spacer(&self, range: &Range<usize>, left: Pixels, height: Pixels) -> Piece {
        let mut spacer = blank_piece(
            range.clone(),
            px(0.),
            height,
            PieceContent::Quad {
                color: gpui::transparent_black(),
                radius: px(0.),
            },
        );
        spacer.x = left;
        spacer
    }

    fn code_header(&self, range: &Range<usize>, title: Option<&str>, left: Pixels) -> Vec<Piece> {
        let theme = self.theme();
        let Some(title) = title else {
            return vec![self.spacer(range, left, theme.space_sm)];
        };
        let mut run = text_run(1, &[], &crate::styling::LineTone::PLAIN, false, theme);
        run.font = theme.ui_font();
        run.color = theme.text_muted;
        let size = theme.small_font_size;
        let line_height = size * theme.ui_line_height_factor;
        let (mut piece, _) = self.label(title, run, size, line_height);
        piece.range = range.clone();
        piece.x = left;
        piece.top = theme.space_sm;
        vec![
            piece,
            self.spacer(range, left, line_height + theme.space_sm * 2.),
        ]
    }

    /// Rows for widgets drawn below the line, such as an image or a math
    /// block being edited.
    pub(super) fn place_below(&mut self, items: &LineItems, builder: &mut RowBuilder) {
        for Attached { range, kind } in &items.below {
            let row = self.block_row(range, kind, builder);
            let anchor = range.end..range.end;
            let mut row = VisualRow {
                range: anchor,
                ..row
            };
            row.kind = RowKind::Below;
            row.soft_end = range.end;
            builder.push_row(row);
        }
    }

    /// Rendered previews of the math under the cursor.
    pub(super) fn overlays(&mut self, items: &LineItems) -> Vec<Overlay> {
        let mut overlays = Vec::new();
        for Attached { range, kind } in &items.above {
            let WidgetKind::MathPreview { tex, display } = kind else {
                continue;
            };
            let size = self.theme().body_font_size;
            if let MathState::Ready(image) = self.math(tex, *display, size) {
                overlays.push(Overlay {
                    anchor: range.start,
                    image: image.image.clone(),
                    width: image.width,
                    height: image.height,
                });
            }
        }
        overlays
    }
}

/// A callout header widget's parts.
struct CalloutHeader<'a> {
    kind: CalloutKind,
    default_title: Option<&'a str>,
    fold: Option<Fold>,
    folded: bool,
}

/// The icon path for a callout type, for tests and painting.
pub fn callout_icon_path(kind: CalloutKind) -> SharedString {
    callout_icon(kind).path()
}
