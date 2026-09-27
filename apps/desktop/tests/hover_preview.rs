//! Hover previews: a link shows its note after a pause, or at once with
//! Mod held; missing notes offer to be created; footnote references show
//! their text; and the popover goes once the pointer leaves.

use std::time::Duration;

use editor_desktop::actions::bind_keys;
use editor_desktop::hover::PreviewContent;
use editor_desktop::vault_index::VaultIndex;
use editor_desktop::{EditorEvent, EditorView};
use gpui::{
    AppContext, Entity, Focusable, Modifiers, Pixels, Point, TestAppContext, VisualTestContext,
};
use tempfile::TempDir;

/// Long enough that its second heading is below the preview's fold.
const TARGET: &str = "# Target\n\nFirst part.\n\n\
    One.\n\nTwo.\n\nThree.\n\nFour.\n\nFive.\n\nSix.\n\nSeven.\n\nEight.\n\n\
    Nine.\n\nTen.\n\nEleven.\n\nTwelve.\n\n## Second\n\nThe second part.\n";

fn open<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
) -> (Entity<EditorView>, TempDir, &'a mut VisualTestContext) {
    cx.update(bind_keys);
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("Target.md"), TARGET).unwrap();
    let text = text.to_owned();
    let (view, cx) = cx.add_window_view(move |_, cx| EditorView::new(&text, Vec::new(), cx));
    let root = vault.path().to_path_buf();
    let index = cx.new(|_| VaultIndex::with_notes(&root, &[("Target.md", TARGET)]));
    view.update(cx, |view, cx| view.set_vault_index(index, cx));
    cx.update(|window, cx| window.focus(&view.focus_handle(cx)));
    cx.run_until_parked();
    (view, vault, cx)
}

/// The middle of the first occurrence of `needle`, as drawn.
fn point_of(view: &Entity<EditorView>, cx: &mut VisualTestContext, needle: &str) -> Point<Pixels> {
    view.read_with(cx, |view, _| {
        let at = view.text().find(needle).expect("the text has the needle");
        let frame = view.frame().expect("the view has been drawn");
        frame
            .range_bounds(&(at..at + needle.len()))
            .expect("the needle is on screen")
            .center()
    })
}

fn hover(cx: &mut VisualTestContext, at: Point<Pixels>, modifiers: Modifiers) {
    cx.simulate_mouse_move(at, None, modifiers);
    cx.run_until_parked();
}

fn hover_on(
    view: &Entity<EditorView>,
    cx: &mut VisualTestContext,
    needle: &str,
    modifiers: Modifiers,
) {
    let at = point_of(view, cx, needle);
    hover(cx, at, modifiers);
}

fn wait(cx: &mut VisualTestContext, delay: Duration) {
    cx.executor().advance_clock(delay);
    cx.run_until_parked();
}

/// What the open preview shows, in a word, and the title of a note.
fn shown(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |view, cx| {
        let open = view.hover_preview()?;
        Some(match &open.content {
            PreviewContent::Loading => "loading".into(),
            PreviewContent::Note { title, .. } => format!("note {title}"),
            PreviewContent::Missing { link } => format!("missing {}", link.name()),
            PreviewContent::Footnote { view } => format!("footnote {}", view.read(cx).text()),
            PreviewContent::Message(message) => message.clone(),
            PreviewContent::Flag(flag) => format!("flag {}", flag.message),
        })
    })
}

fn preview_view(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> Entity<EditorView> {
    view.read_with(cx, |view, _| match &view.hover_preview().unwrap().content {
        PreviewContent::Note { view, .. } => view.clone(),
        _ => panic!("not a note preview"),
    })
}

const DELAY: Duration = Duration::from_millis(400);

#[gpui::test]
fn resting_on_a_link_previews_the_note_at_its_heading(cx: &mut TestAppContext) {
    let (view, _vault, cx) = open(cx, "Intro\n\nSee [[Target#Second]] for more.\n");
    let link = point_of(&view, cx, "Target#Second");
    hover(cx, link, Modifiers::none());
    assert_eq!(shown(&view, cx), None, "not before the pause");
    wait(cx, DELAY);
    assert_eq!(shown(&view, cx).as_deref(), Some("note Target"));
    let preview = preview_view(&view, cx);
    let scrolled = preview.read_with(cx, |preview, _| preview.scroll_offset());
    let height = preview.read_with(cx, |preview, _| preview.content_height());
    assert!(
        scrolled > Pixels::ZERO,
        "scrolled to the heading: {scrolled:?} of {height:?}"
    );
    assert!(cx.debug_bounds("hover-preview").is_some(), "drawn");

    // The same link again reuses the parsed note.
    hover_on(&view, cx, "Intro", Modifiers::none());
    wait(cx, DELAY);
    assert_eq!(shown(&view, cx), None, "closed once the pointer left");
    hover(cx, link, Modifiers::none());
    wait(cx, DELAY);
    assert_eq!(preview_view(&view, cx), preview);
}

#[gpui::test]
fn mod_opens_the_preview_at_once(cx: &mut TestAppContext) {
    let (view, _vault, cx) = open(cx, "Intro\n\nSee [[Target]].\n");
    hover_on(&view, cx, "Target", Modifiers::secondary_key());
    assert_eq!(shown(&view, cx).as_deref(), Some("note Target"));
    cx.simulate_keystrokes("escape");
    assert_eq!(shown(&view, cx), None, "Escape closes it");
}

#[gpui::test]
fn the_pointer_can_cross_into_the_popover(cx: &mut TestAppContext) {
    let (view, _vault, cx) = open(cx, "Intro\n\nSee [[Target]].\n");
    hover_on(&view, cx, "Target", Modifiers::secondary_key());
    let popover = cx.debug_bounds("hover-preview").expect("drawn");
    hover(cx, popover.center(), Modifiers::none());
    wait(cx, DELAY);
    assert_eq!(
        shown(&view, cx).as_deref(),
        Some("note Target"),
        "still open"
    );
    hover_on(&view, cx, "Intro", Modifiers::none());
    wait(cx, DELAY);
    assert_eq!(shown(&view, cx), None);
}

#[gpui::test]
fn a_missing_note_offers_to_create_it(cx: &mut TestAppContext) {
    let (view, _vault, cx) = open(cx, "Intro\n\nSee [[Quantum Tunneling]].\n");
    let opened = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = opened.clone();
    cx.update(|_, cx| {
        cx.subscribe(&view, move |_, event: &EditorEvent, _| {
            if let EditorEvent::OpenLink(target) = event {
                seen.borrow_mut().push(target.clone());
            }
        })
        .detach()
    });
    hover_on(&view, cx, "Quantum", Modifiers::secondary_key());
    assert_eq!(
        shown(&view, cx).as_deref(),
        Some("missing Quantum Tunneling")
    );
    let create = cx.debug_bounds("button-Create").expect("a Create button");
    cx.simulate_click(create.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(*opened.borrow(), ["Quantum Tunneling"]);
    assert_eq!(shown(&view, cx), None);
}

#[gpui::test]
fn footnote_references_show_their_text(cx: &mut TestAppContext) {
    let text = "Intro\n\nA claim[^1] and another[^2].\n\n[^1]: The *source*.\n";
    let (view, _vault, cx) = open(cx, text);
    hover_on(&view, cx, "[^1]", Modifiers::secondary_key());
    assert_eq!(shown(&view, cx).as_deref(), Some("footnote The *source*."));
    hover_on(&view, cx, "Intro", Modifiers::none());
    wait(cx, DELAY);
    hover_on(&view, cx, "[^2]", Modifiers::secondary_key());
    assert_eq!(
        shown(&view, cx).as_deref(),
        Some("Footnote [^2] has no definition.")
    );
}

#[gpui::test]
fn web_links_and_plain_text_preview_nothing(cx: &mut TestAppContext) {
    let (view, _vault, cx) = open(cx, "Intro\n\nSee [site](https://example.com) now.\n");
    hover_on(&view, cx, "site", Modifiers::secondary_key());
    wait(cx, DELAY);
    assert_eq!(shown(&view, cx), None);
    hover_on(&view, cx, "now", Modifiers::secondary_key());
    wait(cx, DELAY);
    assert_eq!(shown(&view, cx), None);
}
