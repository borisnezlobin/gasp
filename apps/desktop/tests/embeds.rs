//! Embedded notes through GPUI's test platform: `![[Note]]`, a heading's
//! section and a block drawn in a card by a read-only view, the header
//! that opens the note, a missing note's Create note button, a note that
//! embeds itself, the source shown at the cursor, and the card following
//! its note as the note changes.

use std::cell::RefCell;
use std::rc::Rc;

use gasp_desktop::actions::bind_keys;
use gasp_desktop::embeds::{EmbedContent, EmbedKey};
use gasp_desktop::line_layout::{Hit, Piece, PieceContent, RowKind};
use gasp_desktop::vault_index::VaultIndex;
use gasp_desktop::{EditorEvent, EditorView};
use gpui::{
    AppContext, Bounds, Entity, Focusable, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent,
    Pixels, Point, TestAppContext, VisualTestContext, point,
};

const LEMMA: &str = "# Lemma\n\nEvery bounded sequence has a convergent subsequence.\n\n## Proof\n\nBisect the interval.\n\n## Remarks\n\nIt fails in infinite dimensions. ^remark\n";

fn open<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
    notes: &[(&str, &str)],
) -> (
    Entity<EditorView>,
    Entity<VaultIndex>,
    &'a mut VisualTestContext,
) {
    cx.update(bind_keys);
    let vault = tempfile::tempdir().unwrap();
    let text = text.to_owned();
    let (view, cx) = cx.add_window_view(move |_, cx| EditorView::new(&text, Vec::new(), cx));
    let root = vault.path().to_path_buf();
    let notes: Vec<(String, String)> = notes
        .iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect();
    let index = cx.new(move |_| {
        let borrowed: Vec<(&str, &str)> = notes
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        VaultIndex::with_notes(&root, &borrowed)
    });
    view.update(cx, |view, cx| view.set_vault_index(index.clone(), cx));
    cx.update(|window, cx| window.focus(&view.focus_handle(cx)));
    cx.run_until_parked();
    (view, index, cx)
}

/// The text of the view drawing `key`'s card.
fn card_text(view: &Entity<EditorView>, cx: &mut VisualTestContext, key: &EmbedKey) -> String {
    let inner = view.read_with(cx, |view, _| match &view.embeds().get(key)?.content {
        EmbedContent::Note { view, .. } => Some(view.clone()),
        _ => None,
    });
    let inner = inner.unwrap_or_else(|| panic!("no note in the card for {key:?}"));
    inner.read_with(cx, |inner, _| inner.text())
}

fn card_message(view: &Entity<EditorView>, cx: &mut VisualTestContext, key: &EmbedKey) -> String {
    view.read_with(cx, |view, _| {
        match &view.embeds().get(key).unwrap().content {
            EmbedContent::Message(message) => message.clone(),
            EmbedContent::Missing => "missing".into(),
            EmbedContent::Note { .. } => "note".into(),
        }
    })
}

/// Every piece drawn on `line`, with its window bounds and row kind.
fn pieces(
    view: &Entity<EditorView>,
    cx: &mut VisualTestContext,
    line: usize,
) -> Vec<(Piece, Bounds<Pixels>, RowKind)> {
    view.read_with(cx, |view, _| {
        let frame = view.frame().unwrap();
        let placed = frame.line(line).unwrap();
        placed
            .visual
            .rows
            .iter()
            .flat_map(|row| {
                row.pieces.iter().map(move |piece| {
                    let origin = point(frame.text_left + piece.x, placed.top + row.top + piece.top);
                    let bounds = Bounds::new(origin, gpui::size(piece.width, piece.height));
                    (piece.clone(), bounds, row.kind)
                })
            })
            .collect()
    })
}

fn opens(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> Rc<RefCell<Vec<String>>> {
    let opened = Rc::new(RefCell::new(Vec::new()));
    let sink = opened.clone();
    cx.update(|_, cx| {
        cx.subscribe(view, move |_, event: &EditorEvent, _| {
            if let EditorEvent::OpenLink(target) = event {
                sink.borrow_mut().push(target.clone());
            }
        })
        .detach();
    });
    opened
}

fn click(cx: &mut VisualTestContext, position: Point<Pixels>) {
    cx.simulate_event(MouseDownEvent {
        position,
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        position,
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 1,
    });
    cx.run_until_parked();
}

#[gpui::test]
fn a_note_embed_shows_the_note_in_a_card(cx: &mut TestAppContext) {
    let (view, _, cx) = open(cx, "Intro\n\n![[Lemma]]\n\nAfter\n", &[("Lemma.md", LEMMA)]);
    let key = EmbedKey::new("Lemma", None);
    assert_eq!(card_text(&view, cx, &key), LEMMA.trim_end());
    let drawn = pieces(&view, cx, 2);
    let room = drawn
        .iter()
        .find(|(piece, ..)| matches!(&piece.content, PieceContent::Embed { key: shown } if *shown == key))
        .expect("the card keeps room for the note");
    assert!(room.1.size.height > gpui::px(40.), "{:?}", room.1);
    let shows_source = drawn
        .iter()
        .any(|(piece, ..)| piece.is_text() && piece.hit == Hit::Text);
    assert!(!shows_source, "the ![[…]] hides away from the cursor");
}

#[gpui::test]
fn a_heading_or_block_embed_shows_only_that_part(cx: &mut TestAppContext) {
    let text = "![[Lemma#Proof]]\n\n![[Lemma#^remark]]\n\n![[Lemma#Nowhere]]\n";
    let (view, _, cx) = open(cx, text, &[("Lemma.md", LEMMA)]);
    let proof = EmbedKey::new("Lemma", Some("Proof"));
    assert_eq!(
        card_text(&view, cx, &proof),
        "## Proof\n\nBisect the interval."
    );
    let remark = EmbedKey::new("Lemma", Some("^remark"));
    assert_eq!(
        card_text(&view, cx, &remark),
        "It fails in infinite dimensions."
    );
    let nowhere = EmbedKey::new("Lemma", Some("Nowhere"));
    assert_eq!(
        card_message(&view, cx, &nowhere),
        "“Lemma” has no heading “Nowhere”."
    );
}

#[gpui::test]
fn the_header_opens_the_note(cx: &mut TestAppContext) {
    let (view, _, cx) = open(cx, "![[Lemma#Proof]]\n\nAfter\n", &[("Lemma.md", LEMMA)]);
    let opened = opens(&view, cx);
    let header = pieces(&view, cx, 0)
        .into_iter()
        .find(|(piece, ..)| matches!(&piece.hit, Hit::Open { .. }) && piece.is_text())
        .expect("a header that opens the note");
    click(cx, header.1.center());
    assert_eq!(*opened.borrow(), ["Lemma#Proof"]);
    assert_eq!(
        view.read_with(cx, |view, _| view.cursor()),
        0,
        "the caret stays"
    );
}

#[gpui::test]
fn a_missing_note_offers_to_make_it(cx: &mut TestAppContext) {
    let (view, _, cx) = open(cx, "![[Someday]]\n\nAfter\n", &[("Lemma.md", LEMMA)]);
    let key = EmbedKey::new("Someday", None);
    assert_eq!(card_message(&view, cx, &key), "missing");
    let opened = opens(&view, cx);
    let button = pieces(&view, cx, 0)
        .into_iter()
        .find(|(piece, ..)| {
            matches!(&piece.content, PieceContent::Text(text) if text.shaped.text.as_ref() == "Create note")
        })
        .expect("a Create note button");
    assert!(matches!(&button.0.hit, Hit::Open { target } if target == "Someday"));
    click(cx, button.1.center());
    assert_eq!(*opened.borrow(), ["Someday"]);
}

#[gpui::test]
fn a_note_that_embeds_itself_stops(cx: &mut TestAppContext) {
    let a = "A's text.\n\n![[B]]\n";
    let b = "B's text.\n\n![[A]]\n";
    let (view, _, cx) = open(cx, "![[A]]\n", &[("A.md", a), ("B.md", b)]);
    let in_a = view.read_with(cx, |view, _| {
        match &view
            .embeds()
            .get(&EmbedKey::new("A", None))
            .unwrap()
            .content
        {
            EmbedContent::Note { view, .. } => view.clone(),
            _ => panic!("A shows"),
        }
    });
    cx.run_until_parked();
    let in_b = in_a.read_with(cx, |a, _| {
        match &a.embeds().get(&EmbedKey::new("B", None)).unwrap().content {
            EmbedContent::Note { view, .. } => view.clone(),
            _ => panic!("B shows inside A"),
        }
    });
    let message = in_b.read_with(cx, |b, _| {
        match &b.embeds().get(&EmbedKey::new("A", None)).unwrap().content {
            EmbedContent::Message(message) => message.clone(),
            _ => panic!("A isn't shown again inside B"),
        }
    });
    assert_eq!(message, "“A” embeds itself here, so it stops.");
}

#[gpui::test]
fn the_source_shows_at_the_cursor_with_the_card_below(cx: &mut TestAppContext) {
    let (view, _, cx) = open(cx, "Intro\n\n![[Lemma]]\n", &[("Lemma.md", LEMMA)]);
    let at = "Intro\n\n![[Le".len();
    view.update(cx, |view, cx| view.move_to(at, false, cx));
    cx.run_until_parked();
    let drawn = pieces(&view, cx, 2);
    assert!(
        drawn.iter().any(|(piece, ..)| piece.is_text()),
        "the source shows"
    );
    let below = drawn.iter().any(|(piece, _, row)| {
        matches!(piece.content, PieceContent::Embed { .. }) && *row == RowKind::Below
    });
    assert!(below, "the card sits under its source");
}

#[gpui::test]
fn a_card_follows_its_note(cx: &mut TestAppContext) {
    let (view, index, cx) = open(cx, "![[Lemma#Proof]]\n", &[("Lemma.md", LEMMA)]);
    let key = EmbedKey::new("Lemma", Some("Proof"));
    let changed = LEMMA.replace("Bisect the interval.", "Halve it again and again.");
    index.update(cx, |index, cx| {
        index.note_text_changed("Lemma.md", &changed);
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(
        card_text(&view, cx, &key),
        "## Proof\n\nHalve it again and again."
    );
}

#[gpui::test]
fn saving_the_embedded_note_updates_the_card(cx: &mut TestAppContext) {
    use gasp_desktop::workspace::{OpenIn, Workspace};
    use std::path::Path;
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("Host.md"), "![[Lemma#Proof]]\n").unwrap();
    std::fs::write(vault.path().join("Lemma.md"), LEMMA).unwrap();
    cx.update(bind_keys);
    let root = vault.path().to_path_buf();
    let (workspace, cx) = cx.add_window_view(move |window, cx| Workspace::new(&root, window, cx));
    cx.run_until_parked();
    let open_note = |cx: &mut VisualTestContext, name: &str, open_in: OpenIn| {
        cx.update(|window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace
                    .open_path(Path::new(name), open_in, window, cx)
                    .unwrap()
            })
        });
        cx.run_until_parked();
        cx.read(|cx| workspace.read(cx).active_editor(cx).unwrap())
    };
    let host = open_note(cx, "Host.md", OpenIn::ActiveTab);
    let lemma = open_note(cx, "Lemma.md", OpenIn::SplitRight);
    let key = EmbedKey::new("Lemma", Some("Proof"));
    assert_eq!(
        card_text(&host, cx, &key),
        "## Proof\n\nBisect the interval."
    );
    let at = LEMMA.find("Bisect").unwrap();
    lemma.update(cx, |lemma, cx| {
        lemma.replace(at..at + "Bisect".len(), "Halve", cx)
    });
    let path = cx.read(|cx| workspace.read(cx).vault().join("Lemma.md"));
    let doc = cx.read(|cx| workspace.read(cx).doc_for_path(&path, cx).unwrap());
    doc.update(cx, |doc, cx| doc.save(cx).unwrap());
    cx.run_until_parked();
    assert_eq!(
        card_text(&host, cx, &key),
        "## Proof\n\nHalve the interval."
    );
}
