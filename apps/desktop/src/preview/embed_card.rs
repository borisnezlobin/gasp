//! An embedded note's card: a quiet surface with the note's name at the
//! top, which opens it, and room below for the note's text, which the
//! editor draws with a read-only view of its own. A note that isn't there
//! gets a card saying so with a button that makes it; a heading that
//! isn't there, or a note embedding itself, gets a line saying so.
//! Clicking the card puts the cursor in its `![[…]]`, as clicking any
//! widget does.

use std::ops::Range;

use gpui::{Pixels, px};

use crate::embeds::{EmbedKey, EmbedLook};
use crate::icons::IconName;
use crate::line_layout::{Hit, Piece, PieceContent};
use crate::preview::layout::LineLayouter;

impl LineLayouter<'_, '_> {
    pub(super) fn embed_card(
        &mut self,
        range: &Range<usize>,
        key: &EmbedKey,
        left: Pixels,
        width: Pixels,
    ) -> Vec<Piece> {
        let theme = self.theme();
        let top = theme.space_sm;
        let look = self.resources.embeds.look(key);
        let missing = look == EmbedLook::Missing;
        let (header, header_height) = self.embed_header(range, key, (left, top), width, missing);
        let body_top = top + header_height;
        let (body, body_height) = self.embed_body(range, key, &look, (left, body_top), width);
        let height = header_height + body_height;
        let card = Hit::Card { url: key.link() };
        let mut surface = self.card_quad(
            range,
            (left, top),
            (width, height),
            theme.embed.card_fill,
            theme.radius_md,
        );
        surface.hit = card.clone();
        let mut pieces = vec![surface];
        pieces.extend(body);
        pieces.extend(header);
        pieces.push(self.spacer(range, left, top + height + theme.space_sm));
        pieces
    }

    /// The note's name with an arrow, one control that opens the note;
    /// a missing note's name alone. Answers the pieces and the header's
    /// height.
    fn embed_header(
        &self,
        range: &Range<usize>,
        key: &EmbedKey,
        (left, top): (Pixels, Pixels),
        width: Pixels,
        missing: bool,
    ) -> (Vec<Piece>, Pixels) {
        let theme = self.theme();
        let style = self.small_style(theme.embed.title);
        let pad = theme.space_lg;
        let icon = theme.icon_size;
        let row = style.line_height.max(icon) + theme.space_sm * 2.;
        let height = theme.space_sm + row;
        let row_top = top + theme.space_sm;
        let icon_top = row_top + (row - icon) / 2.;
        let text_top = row_top + (row - style.line_height) / 2.;
        let text_left = left + pad + icon + theme.space_sm;
        let arrow_room = if missing {
            px(0.)
        } else {
            icon + theme.space_xs
        };
        let room = (width - pad * 2. - icon - theme.space_sm - arrow_room).max(px(0.));
        let title = self.fitted(range, &key.title(), &style, (text_left, text_top), room);
        let title_right = title.x + title.width;
        let mut pieces = vec![
            self.icon_piece(range, IconName::FileText, (left + pad, icon_top), icon),
            title,
        ];
        if missing {
            return (pieces, height);
        }
        let arrow_left = title_right + theme.space_xs;
        pieces.push(self.icon_piece(
            range,
            IconName::ArrowSquareOut,
            (arrow_left, icon_top),
            icon,
        ));
        let control_left = left + pad - theme.space_sm;
        let control = self.card_quad(
            range,
            (control_left, row_top),
            (arrow_left + icon + theme.space_sm - control_left, row),
            gpui::transparent_black(),
            theme.radius_sm,
        );
        pieces.insert(0, control);
        let open = Hit::Open { target: key.link() };
        for piece in &mut pieces {
            piece.hit = open.clone();
        }
        (pieces, height)
    }

    /// What's under the header: room for the note's view, what's missing,
    /// or a message. Answers the pieces and their height.
    fn embed_body(
        &self,
        range: &Range<usize>,
        key: &EmbedKey,
        look: &EmbedLook,
        (left, top): (Pixels, Pixels),
        width: Pixels,
    ) -> (Vec<Piece>, Pixels) {
        let theme = self.theme();
        match look {
            EmbedLook::Pending => (Vec::new(), self.line_height() + theme.space_lg * 2.),
            EmbedLook::Note {
                body_height,
                clipped,
            } => self.embed_note_room(range, key, (left, top), width, *body_height, *clipped),
            EmbedLook::Missing => self.embed_missing(range, key, (left, top), width),
            EmbedLook::Message(message) => {
                let style = self.small_style(theme.embed.message);
                let pad = theme.space_lg;
                let text = self.fitted(
                    range,
                    message,
                    &style,
                    (left + pad, top + theme.space_sm),
                    width - pad * 2.,
                );
                (vec![text], style.line_height + theme.space_sm + pad)
            }
        }
    }

    /// The room the note's view is drawn in, and, when the note is longer
    /// than the card shows, a line that opens it to read the rest.
    fn embed_note_room(
        &self,
        range: &Range<usize>,
        key: &EmbedKey,
        (left, top): (Pixels, Pixels),
        width: Pixels,
        body_height: Pixels,
        clipped: bool,
    ) -> (Vec<Piece>, Pixels) {
        let theme = self.theme();
        let room = Piece {
            range: range.clone(),
            x: left,
            top,
            width,
            height: body_height,
            content: PieceContent::Embed { key: key.clone() },
            hit: Hit::Card { url: key.link() },
        };
        if !clipped {
            return (vec![room], body_height);
        }
        let style = self.small_style(theme.embed.title);
        let pad = theme.space_lg;
        let mut more = self.fitted(
            range,
            "Open the note to read the rest",
            &style,
            (left + pad, top + body_height),
            width - pad * 2.,
        );
        more.hit = Hit::Open { target: key.link() };
        (vec![room, more], body_height + style.line_height + pad)
    }

    /// A note that isn't there: says so, with a button that makes it.
    fn embed_missing(
        &self,
        range: &Range<usize>,
        key: &EmbedKey,
        (left, top): (Pixels, Pixels),
        width: Pixels,
    ) -> (Vec<Piece>, Pixels) {
        let theme = self.theme();
        let pad = theme.space_lg;
        let style = self.small_style(theme.embed.message);
        let label_style = self.small_style(theme.text);
        let button_height = label_style.line_height + theme.space_sm * 2.;
        let (label, _) = self.label(
            "Create note",
            label_style.run(),
            label_style.size,
            label_style.line_height,
        );
        let button_width = label.width + theme.space_md * 2.;
        let button_left = left + width - pad - button_width;
        let button_top = top + theme.space_xs;
        let message_top = button_top + (button_height - style.line_height) / 2.;
        let room = (button_left - left - pad * 2.).max(px(0.));
        let message = self.fitted(
            range,
            "This note doesn’t exist yet.",
            &style,
            (left + pad, message_top),
            room,
        );
        let create = Hit::Open { target: key.link() };
        let mut button = self.card_quad(
            range,
            (button_left, button_top),
            (button_width, button_height),
            theme.embed.button_fill,
            theme.radius_sm,
        );
        button.hit = create.clone();
        let label = Piece {
            range: range.clone(),
            x: button_left + theme.space_md,
            top: button_top + theme.space_sm,
            hit: create,
            ..label
        };
        (
            vec![message, button, label],
            button_height + theme.space_xs + pad,
        )
    }

    fn icon_piece(
        &self,
        range: &Range<usize>,
        icon: IconName,
        (x, top): (Pixels, Pixels),
        side: Pixels,
    ) -> Piece {
        Piece {
            range: range.clone(),
            x,
            top,
            width: side,
            height: side,
            content: PieceContent::Icon {
                path: icon.path(),
                color: self.theme().embed.title,
            },
            hit: Hit::Widget,
        }
    }
}
