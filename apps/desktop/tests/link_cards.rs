//! Link cards: pasting an address on an empty line offers a card, the
//! offer turns the line into a Link Embed block, and live preview draws
//! the block as a card. Pages are read by a stand-in, not the web.

use editor_config::{Platform, RuleSet};
use editor_core::link_card::LinkCard;
use editor_desktop::EditorView;
use editor_desktop::actions::bind_keys;
use editor_desktop::keymap::editor_bindings;
use editor_desktop::link_cards::OfferState;
use gpui::{ClipboardItem, Entity, Focusable, Modifiers, TestAppContext, VisualTestContext};

const URL: &str = "https://physics.example.org/waves";

fn open<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
    cursor: usize,
) -> (Entity<EditorView>, &'a mut VisualTestContext) {
    cx.update(bind_keys);
    let text = text.to_owned();
    let (view, cx) = cx.add_window_view(move |_, cx| {
        let mut view = EditorView::new(&text, Vec::new(), cx);
        view.set_card_fetcher(|url| {
            Ok(LinkCard {
                url: url.to_owned(),
                title: "Wave packets".into(),
                description: "How a group of waves moves together.".into(),
                image: None,
                favicon: None,
            })
        });
        view
    });
    cx.update(|window, cx| window.focus(&view.focus_handle(cx)));
    view.update(cx, |view, cx| view.move_to(cursor, false, cx));
    cx.run_until_parked();
    (view, cx)
}

fn paste(cx: &mut VisualTestContext, text: &str) {
    cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
    let keystroke = editor_bindings(&RuleSet::defaults(), Platform::current())
        .into_iter()
        .find(|(_, id)| id == "edit.paste")
        .map(|(keystroke, _)| keystroke)
        .expect("paste has a key");
    cx.simulate_keystrokes(&keystroke);
    cx.run_until_parked();
}

fn offer(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> Option<OfferState> {
    view.read_with(cx, |view, _| {
        view.card_offer().map(|offer| offer.state.clone())
    })
}

fn text(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |view, _| view.text())
}

#[gpui::test]
fn an_address_pasted_on_an_empty_line_offers_a_card(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "Intro\n\n", 7);
    paste(cx, URL);
    assert_eq!(offer(&view, cx), Some(OfferState::Offered));
    assert!(cx.debug_bounds("card-offer").is_some(), "the chip shows");
    cx.simulate_keystrokes("escape");
    assert_eq!(offer(&view, cx), None);
    assert_eq!(
        text(&view, cx),
        format!("Intro\n\n{URL}"),
        "nothing changes by itself"
    );
}

#[gpui::test]
fn an_address_in_a_sentence_is_just_pasted(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "See ", 4);
    paste(cx, URL);
    assert_eq!(offer(&view, cx), None);
}

#[gpui::test]
fn the_chip_turns_the_line_into_a_card(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "Intro\n\n\nAfter.", 7);
    paste(cx, URL);
    let chip = cx.debug_bounds("card-offer").expect("the chip shows");
    cx.simulate_click(chip.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(offer(&view, cx), None);
    let expected = format!(
        "Intro\n\n```embed\ntitle: \"Wave packets\"\n\
         description: \"How a group of waves moves together.\"\nurl: \"{URL}\"\n```\nAfter."
    );
    assert_eq!(text(&view, cx), expected);
    let drawn_as_card = view.read_with(cx, |view, _| {
        view.frame().is_some_and(|frame| {
            frame.lines.iter().any(|placed| {
                placed.visual.rows.iter().any(|row| {
                    row.pieces
                        .iter()
                        .any(|piece| matches!(&piece.hit, editor_desktop::line_layout::Hit::Link { url } if url == URL))
                })
            })
        })
    });
    assert!(drawn_as_card, "live preview draws the block as a card");
    view.update(cx, |view, cx| view.undo(cx));
    assert_eq!(
        text(&view, cx),
        format!("Intro\n\n{URL}\nAfter."),
        "one undo step"
    );
}

#[gpui::test]
fn moving_away_withdraws_the_offer(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "Intro\n\n", 7);
    paste(cx, URL);
    view.update(cx, |view, cx| view.move_to(0, false, cx));
    assert_eq!(offer(&view, cx), None);
}

#[gpui::test]
fn a_page_that_cant_be_reached_leaves_the_address(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "", 0);
    view.update(cx, |view, _| {
        view.set_card_fetcher(|_| Err("offline".into()))
    });
    paste(cx, URL);
    view.update(cx, |view, cx| view.make_card(cx));
    cx.run_until_parked();
    assert_eq!(offer(&view, cx), Some(OfferState::Failed));
    assert_eq!(text(&view, cx), URL);
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(5));
    cx.run_until_parked();
    assert_eq!(offer(&view, cx), None);
}
