//! Link cards, as the Link Embed plugin makes them: pasting a web address
//! on an empty line offers, in a small chip beside it, to turn it into a
//! card. Nothing happens unless the chip (or `link.make-card`) is used;
//! then the page's title, description and image are fetched in the
//! background and the line becomes an `embed` block, which live preview
//! draws as a card.

pub mod images;
pub use gasp_core::link_card::meta;
mod net;

use std::ops::Range;
use std::time::Duration;

use gasp_core::document::Selection;
use gasp_core::link_card::{LinkCard, is_web_url};
use gasp_core::transaction::{ChangeSet, Origin, Transaction};
use gpui::{
    AnyElement, AppContext, ClickEvent, Context, MouseButton, Task, anchored, div, point,
    prelude::*,
};

use crate::editor::EditorView;
use crate::frame::FrameLayout;
use crate::icons::{IconName, icon};

/// The command that makes a card, and the undo step it's recorded as.
pub const MAKE_CARD_COMMAND: &str = "link.make-card";

/// How long "Couldn't reach the page" stays before the chip goes.
const FAILURE_SHOWN_FOR: Duration = Duration::from_secs(4);

/// Reads a page and describes its card. Tests swap in their own.
pub type CardFetcher = fn(&str) -> Result<LinkCard, String>;

/// Where an offer is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OfferState {
    Offered,
    Fetching,
    Failed,
}

/// The chip beside a pasted address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardOffer {
    pub url: String,
    pub state: OfferState,
}

/// The editor's link card state.
pub struct LinkCards {
    offer: Option<CardOffer>,
    task: Option<Task<()>>,
    fetcher: CardFetcher,
}

impl Default for LinkCards {
    fn default() -> Self {
        Self {
            offer: None,
            task: None,
            fetcher: fetch_card,
        }
    }
}

/// The card for the page at `url`, read from the web.
pub fn fetch_card(url: &str) -> Result<LinkCard, String> {
    let bytes = net::get(url, meta::MAX_PAGE_BYTES)?;
    Ok(meta::card_from_html(url, &String::from_utf8_lossy(&bytes)))
}

impl EditorView {
    /// The chip beside a pasted address, if one shows.
    pub fn card_offer(&self) -> Option<&CardOffer> {
        self.cards.offer.as_ref()
    }

    /// Reads pages with `fetcher` instead of the web, for tests.
    pub fn set_card_fetcher(&mut self, fetcher: CardFetcher) {
        self.cards.fetcher = fetcher;
    }

    /// The cursor's line, without its break, when it holds a web address
    /// and nothing else.
    fn url_line(&self) -> Option<(Range<usize>, String)> {
        let line = self.source.line_of(self.cursor());
        let range = self.source.line_range(line);
        let text = self.source.text()[range.clone()].trim();
        is_web_url(text).then(|| (range, text.to_owned()))
    }

    /// After a paste onto an empty line: offers a card when what landed
    /// is an address alone on its line.
    pub(crate) fn offer_card_after_paste(&mut self, line_was_empty: bool, cx: &mut Context<Self>) {
        if !line_was_empty || self.read_only {
            return;
        }
        if let Some((_, url)) = self.url_line() {
            self.cards.offer = Some(CardOffer {
                url,
                state: OfferState::Offered,
            });
            cx.notify();
        }
    }

    /// Drops an offer once the cursor leaves its line or the line no
    /// longer holds just the address. A card being fetched stays.
    pub(crate) fn keep_card_offer(&mut self, cx: &mut Context<Self>) {
        let Some(offer) = &self.cards.offer else {
            return;
        };
        if offer.state == OfferState::Fetching {
            return;
        }
        let still_there = self.url_line().is_some_and(|(_, url)| url == offer.url);
        if !still_there {
            self.cards.offer = None;
            self.cards.task = None;
            cx.notify();
        }
    }

    /// Hides the chip, as Escape does. Returns whether one showed.
    pub(crate) fn dismiss_card_offer(&mut self, cx: &mut Context<Self>) -> bool {
        if self.cards.offer.take().is_none() {
            return false;
        }
        self.cards.task = None;
        cx.notify();
        true
    }

    /// `link.make-card`: fetches the page for the address on the cursor's
    /// line, then replaces the line with its card.
    pub fn make_card(&mut self, cx: &mut Context<Self>) {
        let Some((_, url)) = self.url_line() else {
            return;
        };
        self.cards.offer = Some(CardOffer {
            url: url.clone(),
            state: OfferState::Fetching,
        });
        let fetcher = self.cards.fetcher;
        let fetch = cx.background_spawn({
            let url = url.clone();
            async move { fetcher(&url) }
        });
        self.cards.task = Some(cx.spawn(async move |view, cx| {
            let fetched = fetch.await;
            view.update(cx, |view, cx| view.place_card(&url, fetched, cx))
                .ok();
        }));
        cx.notify();
    }

    fn place_card(&mut self, url: &str, fetched: Result<LinkCard, String>, cx: &mut Context<Self>) {
        let card = match fetched {
            Ok(card) => card,
            Err(error) => return self.card_failed(url, &error, cx),
        };
        self.cards.offer = None;
        self.cards.task = None;
        let Some(line) = self.line_holding(url) else {
            return cx.notify();
        };
        let markdown = card.to_markdown();
        let text = self.source.text();
        let end = if text[line.end..].starts_with('\n') {
            line.end + 1
        } else {
            line.end
        };
        let cursor = line.start + markdown.len();
        let transaction = Transaction::new(
            ChangeSet::replace(line.start..end, markdown),
            Origin::command(MAKE_CARD_COMMAND),
            self.now_ms(),
        )
        .with_selection(Selection::cursor(cursor));
        self.apply_transaction(transaction, cx);
    }

    fn card_failed(&mut self, url: &str, error: &str, cx: &mut Context<Self>) {
        eprintln!("could not make a card for {url}: {error}");
        self.cards.offer = Some(CardOffer {
            url: url.to_owned(),
            state: OfferState::Failed,
        });
        self.cards.task = Some(cx.spawn(async move |view, cx| {
            cx.background_executor().timer(FAILURE_SHOWN_FOR).await;
            view.update(cx, |view, cx| view.dismiss_card_offer(cx)).ok();
        }));
        cx.notify();
    }

    /// The line holding just `url`: the cursor's, else the first one.
    fn line_holding(&self, url: &str) -> Option<Range<usize>> {
        if let Some((range, found)) = self.url_line()
            && found == url
        {
            return Some(range);
        }
        let text = self.source.text();
        (0..self.source.line_count())
            .map(|line| self.source.line_range(line))
            .find(|range| text[range.clone()].trim() == url)
    }

    fn make_card_clicked(&mut self, _: &ClickEvent, _: &mut gpui::Window, cx: &mut Context<Self>) {
        self.make_card(cx);
    }

    /// The chip, just after the address on its line. `None` when there's
    /// no offer or the line is off screen.
    pub(crate) fn card_offer_chip(
        &self,
        frame: &FrameLayout,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let offer = self.cards.offer.as_ref()?;
        let line = self.line_holding(&offer.url)?;
        let caret = frame.caret_bounds(line.end, &self.theme)?;
        let theme = crate::ui::ui_theme(cx);
        let height = theme.card_chip_height;
        let (label, clickable) = match offer.state {
            OfferState::Offered => ("Make a card", true),
            OfferState::Fetching => ("Making a card\u{2026}", false),
            OfferState::Failed => ("Couldn’t reach the page", false),
        };
        let chip = div()
            .id("card-offer")
            .debug_selector(|| "card-offer".into())
            .occlude()
            .flex()
            .items_center()
            .gap(theme.space_sm)
            .h(height)
            .px(theme.space_md)
            .rounded(theme.menu_row_radius)
            .bg(theme.menu_background)
            // A chip in the text sits lower than a menu: a lift, not a float.
            .shadow(theme.tab_shadows())
            .font_family(theme.font_family.clone())
            .text_size(theme.small_font_size)
            .text_color(if clickable {
                theme.text
            } else {
                theme.text_muted
            })
            .whitespace_nowrap()
            .when(clickable, |chip| {
                chip.cursor_pointer()
                    .hover(|style| {
                        style.bg(crate::theme::over(
                            theme.control_hover,
                            theme.menu_background,
                        ))
                    })
                    .on_mouse_down(MouseButton::Left, |_, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                    })
                    .on_click(cx.listener(Self::make_card_clicked))
            })
            .child(
                icon(IconName::Article)
                    .size(theme.small_icon_size)
                    .text_color(theme.icon),
            )
            .child(label);
        let y = caret.top() + (caret.size.height - height) / 2.;
        Some(
            anchored()
                .position(point(caret.right() + theme.space_md, y))
                .snap_to_window_with_margin(theme.space_md)
                .child(chip)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_addresses_make_cards() {
        assert!(is_web_url("https://example.com/a?b=c"));
        assert!(!is_web_url("mailto:me@example.com"));
        assert!(!is_web_url("https://"));
        assert!(!is_web_url("see https://example.com"));
    }
}
