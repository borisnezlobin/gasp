//! Drives the file tree through GPUI's test platform on a temporary vault:
//! keyboard navigation, type-to-jump, rename with link updates, create,
//! trash, the context menu, moves and reveal.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gasp_config::RuleSet;
use gasp_config::settings::TrashMode;
use gasp_desktop::file_tree::{FileTree, FileTreeEvent, FileTreeOptions, MenuItem};
use gasp_desktop::keymap::RunCommand;
use gasp_desktop::text_input;
use gpui::{
    Entity, Focusable, Modifiers, MouseButton, Pixels, TestAppContext, VisualTestContext, point, px,
};
use tempfile::TempDir;

const FILES: &[(&str, &str)] = &[
    ("Note 10.md", ""),
    ("Note 2.md", "Links to [[Plan]] and [p](Projects/Plan.md)."),
    ("chart.png", ""),
    ("Projects/Plan.md", "The plan."),
    ("Projects/Archive/Old.md", "[[Plan#Goals|goals]]"),
    ("Daily/2024-01-01.md", "Today: ![[chart.png]]"),
    (".obsidian/app.json", "{}"),
    ("notes.txt", "hidden"),
];

fn vault() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in FILES {
        let path = dir.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    dir
}

fn options(watch: bool) -> FileTreeOptions {
    FileTreeOptions {
        trash: TrashMode::Vault,
        watch,
    }
}

type Events = Rc<RefCell<Vec<FileTreeEvent>>>;

fn open<'a>(
    cx: &'a mut TestAppContext,
    root: &Path,
    watch: bool,
) -> (Entity<FileTree>, &'a mut VisualTestContext, Events) {
    cx.update(|cx| text_input::bind_keys(&RuleSet::defaults(), cx));
    let root = root.to_path_buf();
    let (tree, cx) = cx.add_window_view(move |window, cx| {
        FileTree::with_options(root.clone(), options(watch), window, cx)
    });
    let events: Events = Rc::default();
    let seen = events.clone();
    cx.update(|window, cx| {
        window.focus(&tree.focus_handle(cx));
        cx.subscribe(&tree, move |_, event: &FileTreeEvent, _| {
            seen.borrow_mut().push(event.clone());
        })
        .detach();
    });
    cx.run_until_parked();
    (tree, cx, events)
}

fn labels(tree: &Entity<FileTree>, cx: &mut VisualTestContext) -> Vec<String> {
    tree.read_with(cx, |tree, _| {
        tree.rows()
            .iter()
            .map(|row| format!("{}{}", "  ".repeat(row.depth), row.entry.label()))
            .collect()
    })
}

fn selected(tree: &Entity<FileTree>, root: &Path, cx: &mut VisualTestContext) -> Option<PathBuf> {
    tree.read_with(cx, |tree, _| tree.selected_path())
        .map(|path| path.strip_prefix(root).unwrap().to_path_buf())
}

fn select(tree: &Entity<FileTree>, root: &Path, relative: &str, cx: &mut VisualTestContext) {
    let path = root.join(relative);
    tree.update(cx, |tree, cx| assert!(tree.reveal(&path, cx), "{relative}"));
    cx.run_until_parked();
}

fn type_into_field(tree: &Entity<FileTree>, text: &str, cx: &mut VisualTestContext) {
    assert!(tree.read_with(cx, |tree, _| tree.editing_field().is_some()));
    cx.simulate_keystrokes("secondary-a");
    cx.simulate_input(text);
}

#[gpui::test]
fn lists_folders_first_in_natural_order(cx: &mut TestAppContext) {
    let dir = vault();
    let (tree, cx, _) = open(cx, dir.path(), false);
    assert_eq!(
        labels(&tree, cx),
        ["Daily", "Projects", "chart.png", "Note 2", "Note 10"]
    );
}

#[gpui::test]
fn arrows_move_expand_and_collapse(cx: &mut TestAppContext) {
    let dir = vault();
    let root = dir.path();
    let (tree, cx, events) = open(cx, root, false);
    cx.simulate_keystrokes("down down");
    assert_eq!(selected(&tree, root, cx), Some("Projects".into()));
    cx.simulate_keystrokes("right");
    assert_eq!(labels(&tree, cx)[2..4], ["  Archive", "  Plan"]);
    assert!(events.borrow().contains(&FileTreeEvent::ExpansionChanged));
    cx.simulate_keystrokes("right");
    assert_eq!(selected(&tree, root, cx), Some("Projects/Archive".into()));
    cx.simulate_keystrokes("right right");
    assert_eq!(
        selected(&tree, root, cx),
        Some("Projects/Archive/Old.md".into())
    );
    cx.simulate_keystrokes("left");
    assert_eq!(selected(&tree, root, cx), Some("Projects/Archive".into()));
    cx.simulate_keystrokes("left");
    assert_eq!(labels(&tree, cx)[3], "  Plan");
    cx.simulate_keystrokes("left");
    assert_eq!(selected(&tree, root, cx), Some("Projects".into()));
    cx.simulate_keystrokes("left");
    assert_eq!(labels(&tree, cx).len(), 5);
    cx.simulate_keystrokes("end");
    assert_eq!(selected(&tree, root, cx), Some("Note 10.md".into()));
    cx.simulate_keystrokes("home");
    assert_eq!(selected(&tree, root, cx), Some("Daily".into()));
    let expanded = tree.read_with(cx, |tree, _| tree.expanded_folders());
    assert!(expanded.is_empty(), "{expanded:?}");
}

#[gpui::test]
fn enter_opens_and_mod_enter_opens_in_a_new_tab(cx: &mut TestAppContext) {
    let dir = vault();
    let root = dir.path();
    let (tree, cx, events) = open(cx, root, false);
    select(&tree, root, "Note 2.md", cx);
    cx.simulate_keystrokes("enter secondary-enter");
    let opened: Vec<FileTreeEvent> = events
        .borrow()
        .iter()
        .filter(|event| matches!(event, FileTreeEvent::Open { .. }))
        .cloned()
        .collect();
    let path = root.join("Note 2.md");
    assert_eq!(
        opened,
        [
            FileTreeEvent::Open {
                path: path.clone(),
                new_tab: false
            },
            FileTreeEvent::Open {
                path,
                new_tab: true
            },
        ]
    );
    // Enter on a folder opens and closes it instead.
    select(&tree, root, "Daily", cx);
    cx.simulate_keystrokes("enter");
    assert!(labels(&tree, cx).contains(&"  2024-01-01".to_string()));
    cx.simulate_keystrokes("enter");
    assert!(!labels(&tree, cx).contains(&"  2024-01-01".to_string()));
}

#[gpui::test]
fn keymap_commands_reach_the_tree(cx: &mut TestAppContext) {
    let dir = vault();
    let root = dir.path();
    let (tree, cx, events) = open(cx, root, false);
    select(&tree, root, "chart.png", cx);
    cx.dispatch_action(RunCommand {
        id: "link.follow".into(),
    });
    assert!(events.borrow().contains(&FileTreeEvent::Open {
        path: root.join("chart.png"),
        new_tab: true,
    }));
    cx.dispatch_action(RunCommand {
        id: "note.rename".into(),
    });
    assert!(tree.read_with(cx, |tree, _| tree.editing_field().is_some()));
}

#[gpui::test]
fn typing_jumps_to_matching_names(cx: &mut TestAppContext) {
    let dir = vault();
    let root = dir.path();
    let (tree, cx, _) = open(cx, root, false);
    cx.simulate_input("n");
    assert_eq!(selected(&tree, root, cx), Some("Note 2.md".into()));
    cx.simulate_input("n");
    assert_eq!(selected(&tree, root, cx), Some("Note 10.md".into()));
    cx.simulate_input("n");
    assert_eq!(selected(&tree, root, cx), Some("Note 2.md".into()));
    cx.executor().advance_clock(Duration::from_secs(2));
    std::thread::sleep(Duration::from_millis(950));
    cx.simulate_input("pr");
    assert_eq!(selected(&tree, root, cx), Some("Projects".into()));
}

#[gpui::test]
fn f2_renames_and_leaves_links_to_the_workspace(cx: &mut TestAppContext) {
    let dir = vault();
    let root = dir.path();
    let (tree, cx, events) = open(cx, root, false);
    select(&tree, root, "Projects/Plan.md", cx);
    cx.simulate_keystrokes("f2");
    let field = tree.read_with(cx, |tree, _| tree.editing_field()).unwrap();
    assert_eq!(
        field.read_with(cx, |field, _| field.text().to_string()),
        "Plan"
    );
    type_into_field(&tree, "Master plan", cx);
    cx.simulate_keystrokes("enter");
    assert!(root.join("Projects/Master plan.md").is_file());
    assert!(!root.join("Projects/Plan.md").exists());
    let events = events.borrow();
    assert!(events.contains(&FileTreeEvent::Renamed {
        from: root.join("Projects/Plan.md"),
        to: root.join("Projects/Master plan.md"),
    }));
    assert!(
        fs::read_to_string(root.join("Note 2.md"))
            .unwrap()
            .contains("[[Plan]]"),
        "the tree itself never rewrites notes"
    );
    assert_eq!(
        selected(&tree, root, cx),
        Some("Projects/Master plan.md".into())
    );
    assert!(tree.read_with(cx, |tree, _| tree.editing_field().is_none()));
    let focused = cx.update(|window, cx| tree.focus_handle(cx).is_focused(window));
    assert!(focused, "focus returns to the tree");
}

#[gpui::test]
fn a_taken_name_keeps_the_field_open_and_escape_cancels(cx: &mut TestAppContext) {
    let dir = vault();
    let root = dir.path();
    let (tree, cx, _) = open(cx, root, false);
    select(&tree, root, "Note 2.md", cx);
    cx.simulate_keystrokes("f2");
    type_into_field(&tree, "Note 10", cx);
    cx.simulate_keystrokes("enter");
    assert!(tree.read_with(cx, |tree, _| tree.edit_error().is_some()));
    assert!(root.join("Note 2.md").exists());
    cx.simulate_keystrokes("escape");
    assert!(tree.read_with(cx, |tree, _| tree.editing_field().is_none()));
    assert!(root.join("Note 2.md").exists());
}

#[gpui::test]
fn renaming_an_image_keeps_its_extension_selected_out(cx: &mut TestAppContext) {
    let dir = vault();
    let root = dir.path();
    let (tree, cx, _) = open(cx, root, false);
    select(&tree, root, "chart.png", cx);
    cx.simulate_keystrokes("f2");
    let field = tree.read_with(cx, |tree, _| tree.editing_field()).unwrap();
    assert_eq!(field.read_with(cx, |field, _| field.selected_range()), 0..5);
    cx.simulate_input("sales");
    cx.simulate_keystrokes("enter");
    assert!(root.join("sales.png").is_file());
}

#[gpui::test]
fn new_notes_and_folders_go_in_the_selected_folder(cx: &mut TestAppContext) {
    let dir = vault();
    let root = dir.path();
    let (tree, cx, events) = open(cx, root, false);
    select(&tree, root, "Projects/Plan.md", cx);
    cx.simulate_keystrokes("secondary-n");
    let field = tree.read_with(cx, |tree, _| tree.editing_field()).unwrap();
    assert_eq!(
        field.read_with(cx, |field, _| field.text().to_string()),
        "Untitled"
    );
    cx.simulate_input("Idea");
    cx.simulate_keystrokes("enter");
    let idea = root.join("Projects/Idea.md");
    assert!(idea.is_file());
    assert!(
        events
            .borrow()
            .contains(&FileTreeEvent::Created { path: idea.clone() })
    );
    assert!(events.borrow().contains(&FileTreeEvent::Open {
        path: idea,
        new_tab: false
    }));
    assert_eq!(selected(&tree, root, cx), Some("Projects/Idea.md".into()));

    cx.simulate_keystrokes("secondary-alt-n");
    cx.simulate_input("Drafts");
    cx.simulate_keystrokes("enter");
    assert!(root.join("Projects/Drafts").is_dir());
}

#[gpui::test]
fn delete_asks_first_then_trashes(cx: &mut TestAppContext) {
    let dir = vault();
    let root = dir.path();
    let (tree, cx, events) = open(cx, root, false);
    select(&tree, root, "Note 10.md", cx);
    cx.simulate_keystrokes("delete");
    let pending = tree.read_with(cx, |tree, _| tree.pending_trash());
    assert_eq!(pending, [root.join("Note 10.md")]);
    cx.simulate_keystrokes("escape");
    assert!(tree.read_with(cx, |tree, _| tree.pending_trash().is_empty()));
    assert!(root.join("Note 10.md").exists());
    cx.simulate_keystrokes("backspace enter");
    assert!(!root.join("Note 10.md").exists());
    assert!(root.join(".trash/Note 10.md").exists());
    assert!(events.borrow().contains(&FileTreeEvent::Trashed {
        path: root.join("Note 10.md"),
        text: Some(String::new()),
        trashed_to: Some(root.join(".trash/Note 10.md")),
    }));
    // The selection moves to a neighbour.
    assert_eq!(selected(&tree, root, cx), Some("Note 2.md".into()));
    assert!(!labels(&tree, cx).contains(&"Note 10".to_string()));
}

#[gpui::test]
fn the_context_menu_works_from_the_keyboard(cx: &mut TestAppContext) {
    let dir = vault();
    let root = dir.path();
    let (tree, cx, _) = open(cx, root, false);
    select(&tree, root, "Note 2.md", cx);
    cx.simulate_keystrokes("shift-f10");
    let items = tree
        .read_with(cx, |tree, _| tree.context_menu_items())
        .unwrap();
    assert_eq!(items[2], MenuItem::Rename);
    cx.simulate_keystrokes("down down enter");
    assert!(tree.read_with(cx, |tree, _| tree.context_menu_items().is_none()));
    assert!(tree.read_with(cx, |tree, _| tree.editing_field().is_some()));
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("shift-f10 up enter");
    assert_eq!(
        tree.read_with(cx, |tree, _| tree.pending_trash()),
        [root.join("Note 2.md")]
    );
}

#[gpui::test]
fn right_click_opens_the_menu_and_copy_path_copies(cx: &mut TestAppContext) {
    let dir = vault();
    let root = dir.path();
    let (tree, cx, _) = open(cx, root, false);
    let row = cx.debug_bounds("tree-row-chart.png").expect("row is drawn");
    cx.simulate_event(gpui::MouseDownEvent {
        position: row.center(),
        button: gpui::MouseButton::Right,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
    assert_eq!(selected(&tree, root, cx), Some("chart.png".into()));
    assert!(tree.read_with(cx, |tree, _| tree.context_menu_items().is_some()));
    let copy = cx
        .debug_bounds("tree-menu-Copy path")
        .expect("menu is drawn");
    cx.simulate_click(copy.center(), Modifiers::default());
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(
        copied,
        Some(root.join("chart.png").to_string_lossy().into_owned())
    );
}

#[gpui::test]
fn clicking_a_row_selects_and_opens(cx: &mut TestAppContext) {
    let dir = vault();
    let root = dir.path();
    let (tree, cx, events) = open(cx, root, false);
    let row = cx.debug_bounds("tree-row-Note 2").expect("row is drawn");
    cx.simulate_click(row.center(), Modifiers::default());
    assert_eq!(selected(&tree, root, cx), Some("Note 2.md".into()));
    assert!(events.borrow().contains(&FileTreeEvent::Open {
        path: root.join("Note 2.md"),
        new_tab: false,
    }));
    let folder = cx.debug_bounds("tree-row-Daily").expect("row is drawn");
    cx.simulate_click(folder.center(), Modifiers::default());
    assert!(labels(&tree, cx).contains(&"  2024-01-01".to_string()));
}

#[gpui::test]
fn cut_and_paste_move_between_folders(cx: &mut TestAppContext) {
    let dir = vault();
    let root = dir.path();
    let (tree, cx, events) = open(cx, root, false);
    select(&tree, root, "Note 2.md", cx);
    cx.simulate_keystrokes("secondary-x");
    select(&tree, root, "Daily", cx);
    cx.simulate_keystrokes("secondary-v");
    assert!(root.join("Daily/Note 2.md").is_file());
    assert!(events.borrow().contains(&FileTreeEvent::Renamed {
        from: root.join("Note 2.md"),
        to: root.join("Daily/Note 2.md"),
    }));
    assert_eq!(selected(&tree, root, cx), Some("Daily/Note 2.md".into()));
}

#[gpui::test]
fn dropping_moves_a_folder_and_keeps_it_open(cx: &mut TestAppContext) {
    let dir = vault();
    let root = dir.path();
    let (tree, cx, events) = open(cx, root, false);
    select(&tree, root, "Projects/Archive/Old.md", cx);
    let (from, into) = (root.join("Projects/Archive"), root.join("Daily"));
    tree.update(cx, |tree, cx| tree.move_into(&from, &into, cx));
    cx.run_until_parked();
    assert!(root.join("Daily/Archive/Old.md").is_file());
    assert!(events.borrow().contains(&FileTreeEvent::Renamed {
        from,
        to: root.join("Daily/Archive"),
    }));
    assert!(labels(&tree, cx).contains(&"    Old".to_string()));
    // A folder can't go inside itself.
    let (projects, inner) = (root.join("Projects"), root.join("Projects"));
    tree.update(cx, |tree, cx| tree.move_into(&projects, &inner, cx));
    assert!(root.join("Projects").is_dir());
}

#[gpui::test]
fn reveal_expands_parents_and_active_note_is_tracked(cx: &mut TestAppContext) {
    let dir = vault();
    let root = dir.path();
    let (tree, cx, _) = open(cx, root, false);
    let old = root.join("Projects/Archive/Old.md");
    tree.update(cx, |tree, cx| {
        tree.set_active_path(Some(&old), cx);
        assert!(tree.reveal(&old, cx));
        assert!(!tree.reveal(Path::new("/elsewhere/x.md"), cx));
    });
    assert_eq!(
        selected(&tree, root, cx),
        Some("Projects/Archive/Old.md".into())
    );
    let mut expanded = tree.read_with(cx, |tree, _| tree.expanded_folders());
    expanded.sort();
    assert_eq!(
        expanded,
        [PathBuf::from("Projects"), PathBuf::from("Projects/Archive")]
    );
}

#[gpui::test]
fn saved_expansion_restores(cx: &mut TestAppContext) {
    let dir = vault();
    let root = dir.path();
    let (tree, cx, _) = open(cx, root, false);
    tree.update(cx, |tree, cx| {
        tree.set_expanded_folders(vec!["Daily".into(), "Gone".into()], cx)
    });
    assert!(labels(&tree, cx).contains(&"  2024-01-01".to_string()));
    assert_eq!(
        tree.read_with(cx, |tree, _| tree.expanded_folders()),
        [PathBuf::from("Daily")]
    );
}

#[gpui::test]
fn escape_hands_focus_back(cx: &mut TestAppContext) {
    let dir = vault();
    let (_, cx, events) = open(cx, dir.path(), false);
    cx.simulate_keystrokes("escape");
    assert!(events.borrow().contains(&FileTreeEvent::Dismissed));
}

#[gpui::test]
fn outside_changes_refresh_the_tree(cx: &mut TestAppContext) {
    let dir = vault();
    let root = dir.path();
    let (tree, cx, _) = open(cx, root, true);
    fs::write(root.join("Fresh.md"), "").unwrap();
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(20));
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
        if labels(&tree, cx).contains(&"Fresh".to_string()) {
            return;
        }
    }
    panic!(
        "the tree never showed the new note: {:?}",
        labels(&tree, cx)
    );
}

/// Where a row was last drawn. The list draws only rows in view, and a
/// row keeps the place it was last drawn at after it scrolls away.
fn row_top(cx: &mut VisualTestContext, row: &'static str) -> Option<Pixels> {
    cx.run_until_parked();
    cx.debug_bounds(row).map(|bounds| bounds.top())
}

/// Lets time pass with the pointer held still.
fn hold(cx: &mut VisualTestContext) {
    for _ in 0..20 {
        cx.executor().advance_clock(Duration::from_millis(250));
        cx.run_until_parked();
    }
}

#[gpui::test]
fn dragging_near_an_edge_scrolls_the_tree_until_the_drop(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    for n in 1..=120 {
        fs::write(dir.path().join(format!("Note {n}.md")), "").unwrap();
    }
    let (_, cx, _) = open(cx, dir.path(), false);
    let (first, last) = ("tree-row-Note 1", "tree-row-Note 120");
    assert_eq!(row_top(cx, last), None, "the last note starts out of view");
    let top = row_top(cx, first).expect("row is drawn");
    let from = cx.debug_bounds("tree-row-Note 5").unwrap().center();
    let height = cx.update(|window, _| window.viewport_size().height);
    let (none, left) = (Modifiers::none(), MouseButton::Left);
    let bottom = point(from.x, height - px(2.));
    cx.simulate_mouse_down(from, left, none);
    cx.simulate_mouse_move(from + point(px(0.), px(10.)), left, none);
    // Held at the bottom edge, the tree keeps scrolling to its end.
    cx.simulate_mouse_move(bottom, left, none);
    hold(cx);
    let end = row_top(cx, last).expect("the last note came into view");
    assert!(end > height - px(60.), "scrolled all the way: {end:?}");
    // Away from the edges it stays put.
    cx.simulate_mouse_move(point(from.x, height / 2.), left, none);
    hold(cx);
    assert_eq!(row_top(cx, last), Some(end));
    // At the top edge it goes back up.
    cx.simulate_mouse_move(point(from.x, top + px(2.)), left, none);
    hold(cx);
    assert_eq!(row_top(cx, first), Some(top));
    // Once dropped, the edge no longer scrolls it.
    cx.simulate_mouse_move(bottom, left, none);
    cx.simulate_mouse_up(bottom, left, none);
    let settled = row_top(cx, first);
    hold(cx);
    cx.simulate_mouse_move(bottom - point(px(0.), px(1.)), None, none);
    hold(cx);
    assert_eq!(row_top(cx, first), settled);
}

fn click_row(cx: &mut VisualTestContext, row: &'static str, modifiers: Modifiers) {
    let bounds = cx.debug_bounds(row).expect("row is drawn");
    cx.simulate_click(bounds.center(), modifiers);
}

fn picked(tree: &Entity<FileTree>, root: &Path, cx: &mut VisualTestContext) -> Vec<PathBuf> {
    tree.read_with(cx, |tree, _| tree.multi_selected_paths())
        .into_iter()
        .map(|path| path.strip_prefix(root).unwrap().to_path_buf())
        .collect()
}

#[gpui::test]
fn multi_select_cmd_click_picks_notes_that_drag_together(cx: &mut TestAppContext) {
    let dir = vault();
    let root = dir.path();
    let (tree, cx, events) = open(cx, root, false);
    click_row(cx, "tree-row-Note 2", Modifiers::none());
    events.borrow_mut().clear();
    click_row(cx, "tree-row-Note 10", Modifiers::secondary_key());
    assert!(events.borrow().is_empty(), "Cmd-click opens nothing");
    assert_eq!(
        picked(&tree, root, cx),
        [PathBuf::from("Note 2.md"), PathBuf::from("Note 10.md")]
    );
    let from = cx.debug_bounds("tree-row-Note 10").unwrap().center();
    let to = cx.debug_bounds("tree-row-Daily").unwrap().center();
    let (none, left) = (Modifiers::none(), MouseButton::Left);
    cx.simulate_mouse_down(from, left, none);
    cx.simulate_mouse_move(from + point(px(0.), px(10.)), left, none);
    cx.simulate_mouse_move(to, left, none);
    cx.simulate_mouse_up(to, left, none);
    cx.run_until_parked();
    assert!(root.join("Daily/Note 2.md").is_file());
    assert!(root.join("Daily/Note 10.md").is_file());
    assert_eq!(
        picked(&tree, root, cx),
        [
            PathBuf::from("Daily/Note 2.md"),
            PathBuf::from("Daily/Note 10.md")
        ]
    );
    click_row(cx, "tree-row-chart.png", Modifiers::none());
    assert!(
        picked(&tree, root, cx).is_empty(),
        "a plain click clears it"
    );
}

#[gpui::test]
fn multi_select_shift_click_cuts_trashes_and_escape_clears(cx: &mut TestAppContext) {
    let dir = vault();
    let root = dir.path();
    let (tree, cx, events) = open(cx, root, false);
    click_row(cx, "tree-row-chart.png", Modifiers::none());
    click_row(cx, "tree-row-Note 10", Modifiers::shift());
    assert_eq!(
        picked(&tree, root, cx),
        ["chart.png", "Note 2.md", "Note 10.md"].map(PathBuf::from)
    );
    cx.simulate_keystrokes("secondary-x");
    select(&tree, root, "Projects", cx);
    cx.simulate_keystrokes("secondary-v");
    for moved in [
        "Projects/chart.png",
        "Projects/Note 2.md",
        "Projects/Note 10.md",
    ] {
        assert!(root.join(moved).is_file(), "{moved}");
    }
    cx.simulate_keystrokes("delete");
    assert_eq!(tree.read_with(cx, |tree, _| tree.pending_trash()).len(), 3);
    cx.simulate_keystrokes("enter");
    assert!(!root.join("Projects/Note 2.md").exists());
    assert!(root.join(".trash/Note 2.md").exists());
    assert!(picked(&tree, root, cx).is_empty());

    click_row(cx, "tree-row-Daily", Modifiers::none());
    click_row(cx, "tree-row-Projects", Modifiers::secondary_key());
    assert_eq!(picked(&tree, root, cx).len(), 2);
    events.borrow_mut().clear();
    cx.simulate_keystrokes("escape");
    assert!(picked(&tree, root, cx).is_empty());
    assert!(!events.borrow().contains(&FileTreeEvent::Dismissed));
    cx.simulate_keystrokes("escape");
    assert!(events.borrow().contains(&FileTreeEvent::Dismissed));
}

#[gpui::test]
fn multi_select_survives_a_right_click_inside_it(cx: &mut TestAppContext) {
    let dir = vault();
    let root = dir.path();
    let (tree, cx, _) = open(cx, root, false);
    click_row(cx, "tree-row-Note 2", Modifiers::secondary_key());
    click_row(cx, "tree-row-Note 10", Modifiers::secondary_key());
    let right_click = |cx: &mut VisualTestContext, row: &'static str| {
        let position = cx.debug_bounds(row).unwrap().center();
        cx.simulate_event(gpui::MouseDownEvent {
            position,
            button: MouseButton::Right,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        });
        cx.run_until_parked();
    };
    right_click(cx, "tree-row-Note 2");
    assert_eq!(picked(&tree, root, cx).len(), 2);
    cx.simulate_keystrokes("escape");
    right_click(cx, "tree-row-chart.png");
    assert!(picked(&tree, root, cx).is_empty());
    assert_eq!(selected(&tree, root, cx), Some("chart.png".into()));
}
