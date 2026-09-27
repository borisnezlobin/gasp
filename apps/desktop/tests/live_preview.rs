//! Live preview through GPUI's test platform: soft wrapping, reveal
//! modes, widgets, highlights, zoom, readable width and links. The test
//! platform shapes every character as a fixed-width glyph 0.6 em wide.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use editor_config::settings::SymbolMode;
use editor_config::{Platform, RuleSet};
use editor_desktop::actions::bind_keys;
use editor_desktop::keymap::editor_bindings;
use editor_desktop::line_layout::{Hit, Piece, PieceContent, VisualLine};
use editor_desktop::preview::math::RenderFn;
use editor_desktop::{EditorEvent, EditorView, HighlightKind};
use editor_math::{MathError, RenderedMath};
use gpui::{
    Entity, Focusable, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point,
    TestAppContext, VisualTestContext, point, px,
};

/// Draws every equation as a box 4px per source byte wide and 10px tall,
/// with its baseline 8px from the top.
fn stub_math() -> RenderFn {
    Arc::new(|tex: &str, _display: bool, _size: f64| {
        if tex.contains("\\bad") {
            return Err(MathError::Convert("unknown command".into()));
        }
        let width = 4. * tex.len() as f64;
        Ok(Arc::new(RenderedMath {
            svg: format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"10\">\
                 <rect width=\"{width}\" height=\"10\"/></svg>"
            ),
            width,
            height: 10.,
            baseline: 8.,
        }))
    })
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
) -> (Entity<EditorView>, &'a mut VisualTestContext) {
    cx.update(bind_keys);
    let text = text.to_owned();
    let (view, cx) = cx.add_window_view(move |_, cx| {
        let mut view = EditorView::new(&text, Vec::new(), cx);
        view.set_math_renderer(stub_math(), cx);
        view
    });
    cx.update(|window, cx| window.focus(&view.focus_handle(cx)));
    cx.run_until_parked();
    (view, cx)
}

fn key_for(command: &str) -> String {
    editor_bindings(&RuleSet::defaults(), Platform::current())
        .into_iter()
        .find(|(_, id)| id == command)
        .map(|(keystroke, _)| keystroke)
        .unwrap_or_else(|| panic!("{command} has no key on this platform"))
}

fn press(cx: &mut VisualTestContext, command: &str) {
    cx.simulate_keystrokes(&key_for(command));
    cx.run_until_parked();
}

fn place_cursor(view: &Entity<EditorView>, cx: &mut VisualTestContext, offset: usize) {
    view.update(cx, |view, cx| view.move_to(offset, false, cx));
    cx.run_until_parked();
}

fn cursor(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> usize {
    view.read_with(cx, |view, _| view.cursor())
}

fn text(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |view, _| view.text())
}

fn visual(view: &Entity<EditorView>, cx: &mut VisualTestContext, line: usize) -> VisualLine {
    view.read_with(cx, |view, _| {
        view.frame().unwrap().line(line).unwrap().visual.clone()
    })
}

/// Whether any drawn piece stands for the byte at `offset` as text.
fn shows_text_at(line: &VisualLine, offset: usize) -> bool {
    line.pieces()
        .any(|piece| piece.is_text() && piece.range.contains(&(offset - line.start)))
}

/// The window position of a piece's centre on `line`.
fn piece_center(
    view: &Entity<EditorView>,
    cx: &mut VisualTestContext,
    line: usize,
    find: impl Fn(&Piece) -> bool,
) -> Point<Pixels> {
    view.read_with(cx, |view, _| {
        let frame = view.frame().unwrap();
        let placed = frame.line(line).unwrap();
        for row in &placed.visual.rows {
            if let Some(piece) = row.pieces.iter().find(|piece| find(piece)) {
                return point(
                    frame.text_left + piece.x + piece.width / 2.,
                    placed.top + row.top + piece.top + piece.height / 2.,
                );
            }
        }
        panic!("no such piece on line {line}");
    })
}

fn click(cx: &mut VisualTestContext, position: Point<Pixels>, modifiers: Modifiers) {
    cx.simulate_event(MouseDownEvent {
        position,
        button: MouseButton::Left,
        modifiers,
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        position,
        button: MouseButton::Left,
        modifiers,
        click_count: 1,
    });
    cx.run_until_parked();
}

fn events(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> Rc<RefCell<Vec<EditorEvent>>> {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    cx.update(|_, cx| {
        cx.subscribe(view, move |_, event: &EditorEvent, _| {
            sink.borrow_mut().push(event.clone())
        })
        .detach()
    });
    seen
}

fn long_paragraph() -> String {
    (0..60)
        .map(|index| format!("word{index:02}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[gpui::test]
fn long_lines_wrap_at_the_column_width(cx: &mut TestAppContext) {
    let paragraph = long_paragraph();
    let (view, cx) = open(cx, &format!("{paragraph}\nshort"));
    let line = visual(&view, cx, 0);
    assert!(line.rows.len() > 2, "{} rows", line.rows.len());
    let column = view.read_with(cx, |view, _| view.frame().unwrap().column_width);
    for row in &line.rows {
        assert!(row.right() <= column + px(0.5), "a row overflows");
    }
    let starts: Vec<usize> = line.rows.iter().map(|row| row.range.start).collect();
    for start in &starts[1..] {
        assert!(
            paragraph[..*start].ends_with(' '),
            "rows break after spaces"
        );
    }
    let next = visual(&view, cx, 1);
    let first_bottom = view.read_with(cx, |view, _| {
        view.frame().unwrap().line(0).unwrap().bottom()
    });
    let second_top = view.read_with(cx, |view, _| view.frame().unwrap().line(1).unwrap().top);
    assert_eq!(first_bottom, second_top);
    assert_eq!(next.rows.len(), 1);
}

#[gpui::test]
fn down_and_up_move_through_wrapped_rows(cx: &mut TestAppContext) {
    let paragraph = long_paragraph();
    let (view, cx) = open(cx, &format!("{paragraph}\nshort line"));
    place_cursor(&view, cx, 3);
    let line = visual(&view, cx, 0);
    let second_row = line.rows[1].range.clone();
    press(cx, "cursor.down");
    let after_down = cursor(&view, cx);
    assert!(
        second_row.contains(&after_down),
        "{after_down} not in {second_row:?}"
    );
    assert_eq!(after_down - second_row.start, 3, "the column is kept");
    let rows = line.rows.len();
    for _ in 1..rows {
        press(cx, "cursor.down");
    }
    let line_two_start = paragraph.len() + 1;
    assert_eq!(cursor(&view, cx), line_two_start + 3);
    press(cx, "cursor.up");
    let last_row = line.rows.last().unwrap().range.clone();
    assert!(last_row.contains(&cursor(&view, cx)));
    press(cx, "cursor.line-start");
    assert_eq!(cursor(&view, cx), last_row.start);
}

#[gpui::test]
fn clicks_and_carets_follow_wrapped_rows(cx: &mut TestAppContext) {
    let paragraph = long_paragraph();
    let (view, cx) = open(cx, &paragraph);
    let line = visual(&view, cx, 0);
    let row = &line.rows[1];
    let (frame_left, line_top) = view.read_with(cx, |view, _| {
        let frame = view.frame().unwrap();
        (frame.text_left, frame.line(0).unwrap().top)
    });
    let target = row.range.start + 2;
    let x = row.x_for(target);
    click(
        cx,
        point(frame_left + x + px(1.), line_top + row.top + px(2.)),
        Modifiers::none(),
    );
    assert_eq!(cursor(&view, cx), target);
    let caret = view.read_with(cx, |view, _| {
        view.frame()
            .unwrap()
            .caret_bounds(target, view.theme())
            .unwrap()
    });
    assert_eq!(caret.origin.y, line_top + row.top + row.caret_top);
    let past_end = point(
        frame_left + row.right() + px(40.),
        line_top + row.top + px(2.),
    );
    click(cx, past_end, Modifiers::none());
    assert_eq!(cursor(&view, cx), row.soft_end);
    assert!(row.soft_end < row.range.end);
}

#[gpui::test]
fn selections_paint_one_rectangle_per_wrapped_row(cx: &mut TestAppContext) {
    let paragraph = long_paragraph();
    let (view, cx) = open(cx, &paragraph);
    let rows = visual(&view, cx, 0).rows.len();
    let rects = view.read_with(cx, |view, _| {
        view.frame()
            .unwrap()
            .range_rects(&(0..paragraph.len()), view.theme())
    });
    assert_eq!(rects.len(), rows);
}

#[gpui::test]
fn markup_hides_away_from_the_cursor_in_each_mode(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "a **bold** b\n\nelsewhere");
    let far = "a **bold** b\n\n".len() + 2;
    place_cursor(&view, cx, far);
    assert_eq!(
        view.read_with(cx, |view, _| view.symbol_mode()),
        SymbolMode::AroundCursor
    );
    assert!(!shows_text_at(&visual(&view, cx, 0), 2));
    assert!(shows_text_at(&visual(&view, cx, 0), 4));
    place_cursor(&view, cx, 5);
    assert!(
        shows_text_at(&visual(&view, cx, 0), 2),
        "the cursor reveals it"
    );

    press(cx, "markdown.cycle-symbols");
    assert_eq!(
        view.read_with(cx, |view, _| view.symbol_mode()),
        SymbolMode::AlwaysHidden
    );
    assert!(!shows_text_at(&visual(&view, cx, 0), 2));

    press(cx, "markdown.cycle-symbols");
    assert_eq!(
        view.read_with(cx, |view, _| view.symbol_mode()),
        SymbolMode::AlwaysShown
    );
    place_cursor(&view, cx, far);
    assert!(shows_text_at(&visual(&view, cx, 0), 2));

    press(cx, "markdown.cycle-symbols");
    assert_eq!(
        view.read_with(cx, |view, _| view.symbol_mode()),
        SymbolMode::AroundCursor
    );
}

#[gpui::test]
fn clicking_a_checkbox_toggles_the_task(cx: &mut TestAppContext) {
    let note = "- [ ] buy milk\n\nend";
    let (view, cx) = open(cx, note);
    place_cursor(&view, cx, note.len());
    let checkbox = piece_center(&view, cx, 0, |piece| {
        matches!(piece.hit, Hit::Checkbox { .. })
    });
    click(cx, checkbox, Modifiers::none());
    assert_eq!(text(&view, cx), "- [x] buy milk\n\nend");
    assert_eq!(cursor(&view, cx), note.len(), "the cursor stays put");
    let checkbox = piece_center(&view, cx, 0, |piece| {
        matches!(piece.hit, Hit::Checkbox { .. })
    });
    click(cx, checkbox, Modifiers::none());
    assert_eq!(text(&view, cx), note);
    press(cx, "edit.undo");
    assert_eq!(text(&view, cx), "- [x] buy milk\n\nend");
}

#[gpui::test]
fn clicking_a_callout_header_folds_it(cx: &mut TestAppContext) {
    let note = "> [!note]+ Title\n> body\n\nend";
    let (view, cx) = open(cx, note);
    place_cursor(&view, cx, note.len());
    assert!(!visual(&view, cx, 1).is_collapsed());
    let header = piece_center(&view, cx, 0, |piece| matches!(piece.hit, Hit::Fold { .. }));
    click(cx, header, Modifiers::none());
    assert!(visual(&view, cx, 1).is_collapsed());
    assert_eq!(text(&view, cx), note, "folding doesn't edit the note");
    assert_eq!(cursor(&view, cx), note.len());
    let header = piece_center(&view, cx, 0, |piece| matches!(piece.hit, Hit::Fold { .. }));
    click(cx, header, Modifiers::none());
    assert!(!visual(&view, cx, 1).is_collapsed());
}

#[gpui::test]
fn highlights_become_background_rectangles(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "find the word and the other word");
    view.update(cx, |view, cx| {
        view.set_highlights(HighlightKind::SearchMatch, vec![9..13, 28..32], cx);
        let active = 9..13;
        view.set_highlights(HighlightKind::ActiveSearchMatch, Vec::from([active]), cx);
    });
    cx.run_until_parked();
    let (highlights, left, x9, x13) = view.read_with(cx, |view, _| {
        let frame = view.frame().unwrap();
        let line = &frame.line(0).unwrap().visual;
        (
            frame.highlights.clone(),
            frame.text_left,
            line.x_for_offset(9),
            line.x_for_offset(13),
        )
    });
    let kinds: Vec<HighlightKind> = highlights.iter().map(|(kind, _)| *kind).collect();
    assert_eq!(
        kinds,
        vec![
            HighlightKind::SearchMatch,
            HighlightKind::SearchMatch,
            HighlightKind::ActiveSearchMatch
        ]
    );
    let active = highlights[2].1;
    assert!((active.origin.x - (left + x9)).abs() < px(0.01));
    assert!((active.size.width - (x13 - x9)).abs() < px(0.01));
    view.update(cx, |view, cx| {
        view.set_highlights(HighlightKind::SearchMatch, Vec::new(), cx);
        view.set_highlights(HighlightKind::ActiveSearchMatch, Vec::new(), cx);
    });
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.frame().unwrap().highlights.is_empty()));
}

#[gpui::test]
fn zoom_scales_text_and_remeasures_lines(cx: &mut TestAppContext) {
    let paragraph = long_paragraph();
    let (view, cx) = open(cx, &format!("{paragraph}\nshort"));
    let before = visual(&view, cx, 1);
    let rows_before = visual(&view, cx, 0).rows.len();
    press(cx, "view.zoom-in");
    assert!((view.read_with(cx, |view, _| view.zoom()) - 1.1).abs() < 1e-4);
    let after = visual(&view, cx, 1);
    let ratio = after.height / before.height;
    assert!((ratio - 1.1).abs() < 0.01, "height grew by {ratio}");
    assert!(after.width() > before.width());
    assert!(visual(&view, cx, 0).rows.len() >= rows_before);
    press(cx, "view.zoom-out");
    press(cx, "view.zoom-out");
    assert!(visual(&view, cx, 1).height < before.height);
    press(cx, "view.zoom-reset");
    assert_eq!(view.read_with(cx, |view, _| view.zoom()), 1.);
    assert_eq!(visual(&view, cx, 1).height, before.height);
}

#[gpui::test]
fn readable_width_centres_the_column(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, &long_paragraph());
    let (bounds, left, width, max) = view.read_with(cx, |view, _| {
        let frame = view.frame().unwrap();
        (
            frame.bounds,
            frame.text_left,
            frame.column_width,
            view.theme().editor_max_width,
        )
    });
    assert!(bounds.size.width > max * 1.5, "the test window is wide");
    assert_eq!(width, max);
    assert_eq!(left - bounds.left(), bounds.right() - (left + width));
    let narrow_rows = visual(&view, cx, 0).rows.len();
    view.update(cx, |view, cx| view.toggle_readable_width(cx));
    cx.run_until_parked();
    let (left, width, padding) = view.read_with(cx, |view, _| {
        let frame = view.frame().unwrap();
        (
            frame.text_left,
            frame.column_width,
            view.theme().text_padding,
        )
    });
    assert_eq!(left, bounds.left() + padding);
    assert_eq!(width, bounds.size.width - padding * 2.);
    assert!(visual(&view, cx, 0).rows.len() < narrow_rows);
}

#[gpui::test]
fn links_open_with_mod_click_and_mod_enter(cx: &mut TestAppContext) {
    let note = "see [[Other note#Part|there]] and [site](https://example.com)";
    let (view, cx) = open(cx, note);
    let seen = events(&view, cx);
    place_cursor(&view, cx, note.find("site").unwrap() + 1);
    press(cx, "link.follow");
    assert!(
        seen.borrow()
            .contains(&EditorEvent::OpenLink("https://example.com".into()))
    );
    place_cursor(&view, cx, 0);
    let wiki = view.read_with(cx, |view, _| {
        let frame = view.frame().unwrap();
        let placed = frame.line(0).unwrap();
        let x = placed.visual.x_for_offset(note.find("there").unwrap() + 2);
        point(frame.text_left + x, placed.text_top() + px(2.))
    });
    click(cx, wiki, Modifiers::secondary_key());
    assert_eq!(
        seen.borrow().last(),
        Some(&EditorEvent::OpenLink("Other note#Part".into()))
    );
    assert_eq!(
        cursor(&view, cx),
        0,
        "following a link doesn't move the cursor"
    );
}

#[gpui::test]
fn inline_math_sits_on_the_text_baseline(cx: &mut TestAppContext) {
    let note = "area $x^2$ grows\n\nend";
    let (view, cx) = open(cx, note);
    place_cursor(&view, cx, note.len());
    cx.run_until_parked();
    let line = visual(&view, cx, 0);
    let row = &line.rows[0];
    let math = row
        .pieces
        .iter()
        .find(|piece| matches!(piece.content, PieceContent::Image { .. }))
        .expect("the equation is drawn");
    assert_eq!(math.width, px(4. * 3.));
    let text = row.pieces.iter().find(|piece| piece.is_text()).unwrap();
    let PieceContent::Text(shaped) = &text.content else {
        unreachable!()
    };
    let (ascent, descent) = (shaped.shaped.ascent, shaped.shaped.descent.abs());
    let text_baseline = text.top + (shaped.line_height - ascent - descent) / 2. + ascent;
    assert_eq!(math.top + px(8.), text_baseline);
    assert!(!shows_text_at(&line, 6), "the source is hidden");

    place_cursor(&view, cx, 7);
    let line = visual(&view, cx, 0);
    assert!(shows_text_at(&line, 6), "the cursor reveals the source");
    assert_eq!(line.overlays.len(), 1, "a preview shows above the line");
    assert_eq!(line.overlays[0].width, px(12.));
}

#[gpui::test]
fn math_errors_show_the_source(cx: &mut TestAppContext) {
    let note = "bad $\\bad$ math\n\nend";
    let (view, cx) = open(cx, note);
    place_cursor(&view, cx, note.len());
    cx.run_until_parked();
    let line = visual(&view, cx, 0);
    assert!(
        !line
            .pieces()
            .any(|piece| matches!(piece.content, PieceContent::Image { .. }))
    );
    assert!(shows_text_at(&line, 6));
}

#[gpui::test]
fn tables_render_as_a_grid_until_the_cursor_enters(cx: &mut TestAppContext) {
    let note = "| a | b |\n| --- | ---: |\n| one | 2 |\n\nend";
    let (view, cx) = open(cx, note);
    place_cursor(&view, cx, note.len());
    let first = visual(&view, cx, 0);
    assert_eq!(first.rows.len(), 1);
    let cells = first.pieces().filter(|piece| piece.is_text()).count();
    assert_eq!(cells, 4);
    assert!(visual(&view, cx, 1).is_collapsed());
    assert!(visual(&view, cx, 2).is_collapsed());
    place_cursor(&view, cx, 3);
    assert!(!visual(&view, cx, 2).is_collapsed());
    assert!(shows_text_at(&visual(&view, cx, 0), 0));
}

#[gpui::test]
fn table_cells_show_rendered_math_on_the_text_baseline(cx: &mut TestAppContext) {
    let wide = "a+b+c+d+e+f+g+h+i+j";
    let note = format!("| h | v |\n| --- | --- |\n| x ${wide}$ | 2 |\n\nend");
    let (view, cx) = open(cx, &note);
    place_cursor(&view, cx, note.len());
    cx.run_until_parked();
    let line = visual(&view, cx, 0);
    let math = line
        .pieces()
        .find(|piece| matches!(piece.content, PieceContent::Image { .. }))
        .expect("the equation in the cell is drawn");
    assert_eq!(math.width, px(4. * wide.len() as f32));
    let texts: Vec<&Piece> = line
        .pieces()
        .filter(|piece| piece.is_text() && piece.top + piece.height > math.top)
        .collect();
    let x_label = texts
        .iter()
        .find(|piece| piece.right() <= math.x)
        .expect("the text before the equation shares its cell");
    let PieceContent::Text(shaped) = &x_label.content else {
        unreachable!()
    };
    let ascent = shaped.shaped.ascent;
    let descent = shaped.shaped.descent.abs();
    let baseline = x_label.top + (shaped.line_height - ascent - descent) / 2. + ascent;
    let gap = (math.top + px(8.) - baseline).abs();
    assert!(
        gap < px(0.01),
        "math sits on the text baseline, {gap:?} off"
    );
    let two = texts
        .iter()
        .find(|piece| piece.x > math.x)
        .expect("the second column");
    assert!(
        two.x >= math.right(),
        "the column after the equation starts past it"
    );
}

#[gpui::test]
fn table_cells_set_each_style_in_its_own_font(cx: &mut TestAppContext) {
    let note = "| h |\n| --- |\n| *it* and `code` |\n\nend";
    let (view, cx) = open(cx, note);
    place_cursor(&view, cx, note.len());
    let line = visual(&view, cx, 0);
    let header = line.pieces().find(|piece| piece.is_text()).unwrap();
    let body_cells = line
        .pieces()
        .filter(|piece| piece.is_text() && piece.top > header.top + header.height)
        .count();
    assert_eq!(body_cells, 3, "italic, plain and code are shaped apart");
}

#[gpui::test]
fn quotes_and_callouts_indent_their_text(cx: &mut TestAppContext) {
    let note = "plain\n> quoted\n> [!warning] Careful\n\nend";
    let (view, cx) = open(cx, note);
    place_cursor(&view, cx, note.len());
    let plain = visual(&view, cx, 0);
    let quoted = visual(&view, cx, 1);
    let first_x = |line: &VisualLine| line.pieces().next().unwrap().x;
    assert!(first_x(&quoted) > first_x(&plain));
    assert_eq!(quoted.decor.bars.len(), 1);
}
