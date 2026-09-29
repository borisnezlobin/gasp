//! A link card: a web page's title, description and address on a quiet
//! surface, with its preview image on the right. The card keeps its size
//! while the image downloads, so nothing moves when it arrives. Clicking
//! it puts the cursor in its source, as clicking any widget does; its
//! Open button, or Mod+click, opens the page.

use std::ops::Range;

use gasp_core::link_card::LinkCard;
use gpui::{Font, Hsla, Pixels, TextRun, px};

use crate::icons::IconName;
use crate::line_layout::{Hit, Piece, PieceContent};
use crate::preview::layout::LineLayouter;

/// The ellipsis that ends a cut line.
const ELLIPSIS: &str = "\u{2026}";

/// How a line of card text is set.
#[derive(Clone)]
struct Style {
    font: Font,
    color: Hsla,
    size: Pixels,
    line_height: Pixels,
}

impl Style {
    fn run(&self) -> TextRun {
        TextRun {
            len: 0,
            font: self.font.clone(),
            color: self.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }
}

impl LineLayouter<'_, '_> {
    pub(super) fn link_card(
        &mut self,
        range: &Range<usize>,
        card: &LinkCard,
        left: Pixels,
        width: Pixels,
    ) -> Vec<Piece> {
        let theme = self.theme();
        let width = width.min(theme.link_card_max_width);
        let height = theme.link_card_height;
        let top = theme.space_sm;
        let pad = theme.space_lg;
        let ring = theme.link_card_ring;
        let mut pieces = vec![
            self.card_quad(
                range,
                (left, top),
                (width, height),
                theme.divider,
                theme.radius_md,
            ),
            self.card_quad(
                range,
                (left + ring, top + ring),
                (width - ring * 2., height - ring * 2.),
                theme.background,
                theme.radius_md - ring,
            ),
        ];
        let mut text_right = left + width - pad;
        if let Some(image) = &card.image {
            let thumb_height = height - pad * 2.;
            let thumb_width = thumb_height * theme.link_card_image_aspect;
            let x = left + width - pad - thumb_width;
            pieces.push(self.thumbnail(range, image, (x, top + pad), (thumb_width, thumb_height)));
            text_right = x - pad;
        }
        let text_left = left + pad;
        let text_width = (text_right - text_left).max(px(0.));
        pieces.extend(self.card_text(range, card, (text_left, top + pad), text_width));
        let footer_top = top + height - pad - self.small_style(theme.text_faint).line_height;
        pieces.extend(self.card_footer(range, card, (text_left, footer_top), text_width));
        pieces.push(self.spacer(range, left, top + height + theme.space_sm));
        let hit = Hit::Card {
            url: card.url.clone(),
        };
        for piece in &mut pieces {
            piece.hit = hit.clone();
        }
        pieces.extend(self.open_button(range, &card.url, (left + width, top)));
        pieces
    }

    /// The Open button at the card's top right, shaped like a code
    /// block's copy button. The editor draws it only while the pointer is
    /// over the card.
    fn open_button(
        &self,
        range: &Range<usize>,
        url: &str,
        (right, top): (Pixels, Pixels),
    ) -> [Piece; 2] {
        let theme = self.theme();
        let icon = crate::code_copy::copy_icon_size(theme);
        let side = icon + theme.space_sm * 3.;
        let (x, y) = (right - theme.space_sm - side, top + theme.space_sm);
        let hit = Hit::Link {
            url: url.to_owned(),
        };
        let square = Piece {
            hit: hit.clone(),
            ..self.card_quad(range, (x, y), (side, side), theme.surface, theme.radius_sm)
        };
        let inset = (side - icon) / 2.;
        let glyph = Piece {
            range: range.clone(),
            x: x + inset,
            top: y + inset,
            width: icon,
            height: icon,
            content: PieceContent::Icon {
                path: IconName::ArrowSquareOut.path(),
                color: theme.text_muted,
            },
            hit,
        };
        [square, glyph]
    }

    fn small_style(&self, color: Hsla) -> Style {
        let theme = self.theme();
        Style {
            font: theme.ui_font(),
            color,
            size: theme.small_font_size,
            line_height: theme.small_font_size * theme.ui_line_height_factor,
        }
    }

    /// The title on one line and the description on up to two.
    fn card_text(
        &self,
        range: &Range<usize>,
        card: &LinkCard,
        (x, top): (Pixels, Pixels),
        width: Pixels,
    ) -> Vec<Piece> {
        let theme = self.theme();
        let mut title_font = theme.ui_font();
        title_font.weight = theme.bold_weight;
        let title = Style {
            font: title_font,
            color: theme.text,
            size: theme.body_font_size,
            line_height: theme.body_font_size * theme.ui_line_height_factor,
        };
        let mut pieces = vec![self.fitted(range, &card.title, &title, (x, top), width)];
        let description = self.small_style(theme.text_muted);
        let mut line_top = top + title.line_height + theme.space_xs;
        for line in self.wrapped(&card.description, &description, width, 2) {
            pieces.push(self.fitted(range, &line, &description, (x, line_top), width));
            line_top += description.line_height;
        }
        pieces
    }

    /// The page's icon and address.
    fn card_footer(
        &mut self,
        range: &Range<usize>,
        card: &LinkCard,
        (x, top): (Pixels, Pixels),
        width: Pixels,
    ) -> Vec<Piece> {
        let theme = self.theme();
        let style = self.small_style(theme.text_faint);
        let mut pieces = Vec::new();
        let mut text_x = x;
        if let Some(favicon) = &card.favicon {
            let size = theme.link_card_icon_size;
            let icon_top = top + (style.line_height - size) / 2.;
            if let Some(image) = self.resources.images.remote_image(favicon, None) {
                pieces.push(Piece {
                    range: range.clone(),
                    x,
                    top: icon_top,
                    width: size,
                    height: size,
                    content: PieceContent::Image {
                        image,
                        radius: px(0.),
                    },
                    hit: Hit::Widget,
                });
            }
            // Room is kept either way, so the address doesn't move when
            // the icon arrives.
            text_x += size + theme.space_sm;
        }
        let room = (width - (text_x - x)).max(px(0.));
        pieces.push(self.fitted(range, card.domain(), &style, (text_x, top), room));
        pieces
    }

    /// The preview image, or its empty frame while it downloads.
    fn thumbnail(
        &mut self,
        range: &Range<usize>,
        url: &str,
        (x, top): (Pixels, Pixels),
        (width, height): (Pixels, Pixels),
    ) -> Piece {
        let theme = self.theme();
        let aspect = theme.link_card_image_aspect;
        let content = match self.resources.images.remote_image(url, Some(aspect)) {
            Some(image) => PieceContent::Image {
                image,
                radius: theme.radius_sm,
            },
            None => PieceContent::Quad {
                color: theme.surface,
                radius: theme.radius_sm,
            },
        };
        Piece {
            range: range.clone(),
            x,
            top,
            width,
            height,
            content,
            hit: Hit::Widget,
        }
    }

    fn card_quad(
        &self,
        range: &Range<usize>,
        (x, top): (Pixels, Pixels),
        (width, height): (Pixels, Pixels),
        color: Hsla,
        radius: Pixels,
    ) -> Piece {
        Piece {
            range: range.clone(),
            x,
            top,
            width,
            height,
            content: PieceContent::Quad { color, radius },
            hit: Hit::Widget,
        }
    }

    /// One line of `text`, cut with an ellipsis to fit `width`.
    fn fitted(
        &self,
        range: &Range<usize>,
        text: &str,
        style: &Style,
        (x, top): (Pixels, Pixels),
        width: Pixels,
    ) -> Piece {
        let (mut piece, _) = self.label(text, style.run(), style.size, style.line_height);
        if piece.width > width {
            let cut = self.cut_at(text, style, width);
            let shown = format!("{}{ELLIPSIS}", text[..cut].trim_end());
            piece = self
                .label(&shown, style.run(), style.size, style.line_height)
                .0;
        }
        piece.range = range.clone();
        piece.x = x;
        piece.top = top;
        piece
    }

    /// Where to cut `text` so it and an ellipsis fit in `width`.
    fn cut_at(&self, text: &str, style: &Style, width: Pixels) -> usize {
        let shaper = self.shaper();
        let run = |len| TextRun { len, ..style.run() };
        let ellipsis = shaper.shape(ELLIPSIS, style.size, &[run(ELLIPSIS.len())]);
        let shaped = shaper.shape(text, style.size, &[run(text.len())]);
        let at = shaped.closest_index_for_x((width - ellipsis.width).max(px(0.)));
        floor_char_boundary(text, at)
    }

    /// `text` broken at spaces into at most `lines` lines of `width`; the
    /// last one keeps the rest, for [`Self::fitted`] to cut.
    fn wrapped(&self, text: &str, style: &Style, width: Pixels, lines: usize) -> Vec<String> {
        let shaper = self.shaper();
        let mut out = Vec::new();
        let mut rest = text.trim();
        while !rest.is_empty() && out.len() + 1 < lines {
            let run = TextRun {
                len: rest.len(),
                ..style.run()
            };
            let shaped = shaper.shape(rest, style.size, &[run]);
            if shaped.width <= width {
                break;
            }
            let fits = floor_char_boundary(rest, shaped.closest_index_for_x(width));
            let cut = rest[..fits].rfind(' ').filter(|&at| at > 0).unwrap_or(fits);
            if cut == 0 {
                break;
            }
            out.push(rest[..cut].to_owned());
            rest = rest[cut..].trim_start();
        }
        if !rest.is_empty() {
            out.push(rest.to_owned());
        }
        out
    }
}

fn floor_char_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}
