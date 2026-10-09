//! Panes and tabs: dragging tabs to reorder them, to move them between
//! panes and to split panes off, the move and focus commands, the tab
//! menu's bulk closes, dividers, and the layout surviving a restart.

use std::path::Path;

use gasp_config::device::{PaneLayout, SplitAxis};
use gasp_config::settings::TrashMode;
use gasp_desktop::actions::bind_keys;
use gasp_desktop::file_tree::{FileTree, FileTreeOptions};
use gasp_desktop::workspace::pane_tree::Direction;
use gasp_desktop::workspace::{OpenIn, Pane, Workspace};
use gpui::{
    AppContext, Bounds, Entity, Modifiers, MouseButton, MouseDownEvent, Pixels, Point,
    TestAppContext, VisualTestContext, point, px,
};
use tempfile::TempDir;

fn vault_with(names: &[&str]) -> TempDir {
    let vault = tempfile::tempdir().unwrap();
    for name in names {
        std::fs::write(vault.path().join(name), format!("# {name}\n")).unwrap();
    }
    vault
}

fn open_workspace<'a>(
    cx: &'a mut TestAppContext,
    vault: &Path,
) -> (Entity<Workspace>, &'a mut VisualTestContext) {
    cx.update(bind_keys);
    let vault = vault.to_path_buf();
    let (workspace, cx) = cx.add_window_view(move |window, cx| Workspace::new(&vault, window, cx));
    cx.run_until_parked();
    (workspace, cx)
}

/// Opens each note as a tab of the active pane.
fn open_tabs(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, names: &[&str]) {
    for (index, name) in names.iter().enumerate() {
        let open_in = if index == 0 {
            OpenIn::ActiveTab
        } else {
            OpenIn::NewTab
        };
        cx.update(|window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace
                    .open_path(Path::new(name), open_in, window, cx)
                    .unwrap()
            })
        });
    }
    cx.run_until_parked();
}

fn run(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, id: &str) {
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| workspace.run_command(id, window, cx))
    });
    cx.run_until_parked();
}

fn panes(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<Entity<Pane>> {
    cx.read(|cx| workspace.read(cx).panes())
}

/// Each pane's tab titles, in reading order.
fn layout(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<Vec<String>> {
    cx.read(|cx| {
        workspace
            .read(cx)
            .panes()
            .iter()
            .map(|pane| {
                pane.read(cx)
                    .tabs()
                    .iter()
                    .map(|tab| tab.title(cx))
                    .collect()
            })
            .collect()
    })
}

fn active_title(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.read(|cx| {
        let pane = workspace.read(cx).active_pane().read(cx);
        pane.active_tab().unwrap().title(cx)
    })
}

fn saved_layout(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> PaneLayout {
    cx.read(|cx| workspace.read(cx).device_state(cx).panes.unwrap())
}

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.run_until_parked();
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} isn't drawn"))
}

/// Drags from `from` to `to` the way a hand does: press, a few moves,
/// release.
fn drag(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
    let none = Modifiers::none();
    cx.simulate_mouse_down(from, MouseButton::Left, none);
    cx.simulate_mouse_move(from + point(px(6.), px(0.)), MouseButton::Left, none);
    for step in 1..=4 {
        let t = step as f32 / 4.;
        let at = point(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
        cx.simulate_mouse_move(at, MouseButton::Left, none);
    }
    cx.simulate_mouse_move(to, MouseButton::Left, none);
    cx.simulate_mouse_up(to, MouseButton::Left, none);
    cx.run_until_parked();
}

#[gpui::test]
fn dragging_a_tab_along_the_strip_reorders_it(cx: &mut TestAppContext) {
    let vault = vault_with(&["a.md", "b.md", "c.md"]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open_tabs(&workspace, cx, &["a.md", "b.md", "c.md"]);
    let a = bounds(cx, "tab-a");
    let c = bounds(cx, "tab-c");
    // Past the middle of the last tab: a goes last.
    drag(cx, a.center(), point(c.right() - px(4.), c.center().y));
    assert_eq!(layout(&workspace, cx), vec![vec!["b", "c", "a"]]);
    assert_eq!(active_title(&workspace, cx), "a");
    // Before the middle of the first tab: a comes back first.
    let a = bounds(cx, "tab-a");
    let b = bounds(cx, "tab-b");
    drag(cx, a.center(), point(b.left() + px(4.), b.center().y));
    assert_eq!(layout(&workspace, cx), vec![vec!["a", "b", "c"]]);
}

#[gpui::test]
fn dropping_a_tab_on_an_edge_of_the_note_splits_the_pane(cx: &mut TestAppContext) {
    let vault = vault_with(&["a.md", "b.md", "c.md"]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open_tabs(&workspace, cx, &["a.md", "b.md", "c.md"]);
    let surface = bounds(cx, "pane-surface");
    let c = bounds(cx, "tab-c");
    let right_edge = point(surface.right() - px(20.), surface.center().y);
    drag(cx, c.center(), right_edge);
    assert_eq!(layout(&workspace, cx), vec![vec!["a", "b"], vec!["c"]]);
    assert_eq!(active_title(&workspace, cx), "c");
    let saved = saved_layout(&workspace, cx);
    assert_eq!(saved.split, Some(SplitAxis::Row));
}

#[gpui::test]
fn dropping_a_tab_in_the_middle_of_a_note_moves_it_there(cx: &mut TestAppContext) {
    let vault = vault_with(&["a.md", "b.md", "c.md"]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open_tabs(&workspace, cx, &["a.md", "b.md"]);
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace
                .open_path(Path::new("c.md"), OpenIn::SplitRight, window, cx)
                .unwrap()
        })
    });
    cx.run_until_parked();
    let [left, right] = panes(&workspace, cx).try_into().unwrap();
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.move_tab(&left, 0, &right, 1, window, cx)
        })
    });
    assert_eq!(layout(&workspace, cx), vec![vec!["b"], vec!["c", "a"]]);
    // Moving the last tab out closes its pane.
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.move_tab(&left, 0, &right, 0, window, cx)
        })
    });
    assert_eq!(layout(&workspace, cx), vec![vec!["b", "c", "a"]]);
}

#[gpui::test]
fn a_note_already_in_the_target_pane_is_shown_not_doubled(cx: &mut TestAppContext) {
    let vault = vault_with(&["a.md", "b.md"]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open_tabs(&workspace, cx, &["a.md", "b.md"]);
    run(&workspace, cx, "pane.split-right");
    let [left, right] = panes(&workspace, cx).try_into().unwrap();
    assert_eq!(layout(&workspace, cx), vec![vec!["a", "b"], vec!["b"]]);
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.move_tab(&left, 1, &right, 0, window, cx)
        })
    });
    assert_eq!(layout(&workspace, cx), vec![vec!["a"], vec!["b"]]);
}

#[gpui::test]
fn quarters_come_from_splitting_each_half_down(cx: &mut TestAppContext) {
    let vault = vault_with(&["a.md", "b.md", "c.md", "d.md"]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open_tabs(&workspace, cx, &["a.md", "b.md", "c.md", "d.md"]);
    let split = |cx: &mut VisualTestContext, index: usize, target: usize, side: Direction| {
        let all = panes(&workspace, cx);
        let (from, target) = (all[0].clone(), all[target].clone());
        cx.update(|window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.split_with_tab(&from, index, &target, side, window, cx)
            })
        });
        cx.run_until_parked();
    };
    split(cx, 1, 0, Direction::Right); // a c d | b
    split(cx, 1, 1, Direction::Down); // a d | (b over c)
    split(cx, 1, 0, Direction::Down); // (a over d) | (b over c)
    assert_eq!(
        layout(&workspace, cx),
        vec![vec!["a"], vec!["d"], vec!["b"], vec!["c"]]
    );
    let saved = saved_layout(&workspace, cx);
    assert_eq!(saved.split, Some(SplitAxis::Row));
    for side in &saved.sides {
        assert_eq!(side.split, Some(SplitAxis::Column));
        assert_eq!(side.ratio, Some(0.5));
    }
    // Moving through the quarters by direction.
    run(&workspace, cx, "pane.focus-up");
    assert_eq!(active_title(&workspace, cx), "a");
    run(&workspace, cx, "pane.focus-right");
    assert_eq!(active_title(&workspace, cx), "b");
    run(&workspace, cx, "pane.focus-down");
    assert_eq!(active_title(&workspace, cx), "c");
    // A pane's only tab can't split away from it.
    split(cx, 0, 0, Direction::Left);
    assert_eq!(panes(&workspace, cx).len(), 4);
}

#[gpui::test]
fn move_tab_commands_go_to_a_neighbour_or_split_one_off(cx: &mut TestAppContext) {
    let vault = vault_with(&["a.md", "b.md", "c.md"]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open_tabs(&workspace, cx, &["a.md", "b.md", "c.md"]);
    run(&workspace, cx, "pane.move-tab-down");
    assert_eq!(layout(&workspace, cx), vec![vec!["a", "b"], vec!["c"]]);
    assert_eq!(saved_layout(&workspace, cx).split, Some(SplitAxis::Column));
    run(&workspace, cx, "pane.move-tab-up");
    assert_eq!(layout(&workspace, cx), vec![vec!["a", "b", "c"]]);
    assert_eq!(active_title(&workspace, cx), "c");
    // A lone tab with nowhere to go stays.
    run(&workspace, cx, "tab.close-others");
    assert_eq!(layout(&workspace, cx), vec![vec!["c"]]);
    run(&workspace, cx, "pane.move-tab-left");
    assert_eq!(layout(&workspace, cx), vec![vec!["c"]]);
}

#[gpui::test]
fn an_image_tab_moves_and_splits_like_a_note(cx: &mut TestAppContext) {
    let vault = vault_with(&["a.md"]);
    image::RgbaImage::new(6, 4)
        .save(vault.path().join("pic.png"))
        .unwrap();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open_tabs(&workspace, cx, &["a.md", "pic.png"]);
    run(&workspace, cx, "pane.move-tab-right");
    assert_eq!(layout(&workspace, cx), vec![vec!["a"], vec!["pic"]]);
    assert_eq!(saved_layout(&workspace, cx).sides[1].tabs, vec!["pic.png"]);
    run(&workspace, cx, "pane.move-tab-left");
    assert_eq!(layout(&workspace, cx), vec![vec!["a", "pic"]]);
    assert_eq!(active_title(&workspace, cx), "pic");
}

#[gpui::test]
fn closing_to_the_right_keeps_the_tabs_before(cx: &mut TestAppContext) {
    let vault = vault_with(&["a.md", "b.md", "c.md", "d.md"]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open_tabs(&workspace, cx, &["a.md", "b.md", "c.md", "d.md"]);
    run(&workspace, cx, "tab.go-2");
    run(&workspace, cx, "tab.close-right");
    assert_eq!(layout(&workspace, cx), vec![vec!["a", "b"]]);
}

#[gpui::test]
fn the_tab_menu_offers_closing_splitting_and_moving(cx: &mut TestAppContext) {
    let vault = vault_with(&["a.md", "b.md"]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open_tabs(&workspace, cx, &["a.md", "b.md"]);
    let a = bounds(cx, "tab-a");
    cx.simulate_event(MouseDownEvent {
        position: a.center(),
        button: MouseButton::Right,
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
    assert_eq!(active_title(&workspace, cx), "a");
    let menu = cx
        .read(|cx| workspace.read(cx).open_menu(cx))
        .expect("the tab menu is open");
    let labels = cx.read(|cx| menu.read(cx).labels());
    for label in [
        "Close",
        "Close others",
        "Close to the right",
        "Split right",
        "Split down",
        "Move to",
        "Copy path",
    ] {
        assert!(
            labels.iter().any(|item| item == label),
            "{label} in {labels:?}"
        );
    }
    cx.update(|window, cx| {
        menu.update(cx, |menu, cx| menu.choose("Close others", window, cx));
    });
    cx.run_until_parked();
    assert_eq!(layout(&workspace, cx), vec![vec!["a"]]);
}

/// Presses, moves partway with the button held, and leaves it held.
fn drag_without_letting_go(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
    let none = Modifiers::none();
    cx.simulate_mouse_down(from, MouseButton::Left, none);
    cx.simulate_mouse_move(from + point(px(6.), px(0.)), MouseButton::Left, none);
    cx.simulate_mouse_move(to, MouseButton::Left, none);
    cx.run_until_parked();
}

#[gpui::test]
fn escape_puts_a_dragged_tab_back(cx: &mut TestAppContext) {
    let vault = vault_with(&["a.md", "b.md", "c.md"]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open_tabs(&workspace, cx, &["a.md", "b.md", "c.md"]);
    let surface = bounds(cx, "pane-surface");
    let c = bounds(cx, "tab-c");
    let right_edge = point(surface.right() - px(20.), surface.center().y);
    drag_without_letting_go(cx, c.center(), right_edge);
    cx.simulate_keystrokes("escape");
    let none = Modifiers::none();
    cx.simulate_mouse_move(right_edge - point(px(4.), px(0.)), MouseButton::Left, none);
    cx.simulate_mouse_up(right_edge, MouseButton::Left, none);
    cx.run_until_parked();
    assert_eq!(layout(&workspace, cx), vec![vec!["a", "b", "c"]]);
    assert!(cx.read(|cx| !cx.has_active_drag()));
}

#[gpui::test]
fn escape_puts_a_dragged_divider_back(cx: &mut TestAppContext) {
    let vault = vault_with(&["a.md", "b.md"]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open_tabs(&workspace, cx, &["a.md", "b.md"]);
    run(&workspace, cx, "pane.move-tab-right");
    let before = saved_layout(&workspace, cx).ratio;
    let divider = bounds(cx, "divider-0");
    drag_without_letting_go(cx, divider.center(), point(px(300.), divider.center().y));
    assert_ne!(
        saved_layout(&workspace, cx).ratio,
        before,
        "the drag moves it"
    );
    cx.simulate_keystrokes("escape");
    let none = Modifiers::none();
    cx.simulate_mouse_move(point(px(250.), divider.center().y), MouseButton::Left, none);
    cx.simulate_mouse_up(point(px(250.), divider.center().y), MouseButton::Left, none);
    cx.run_until_parked();
    assert_eq!(saved_layout(&workspace, cx).ratio, before);
}

#[gpui::test]
fn a_double_click_on_a_divider_evens_it_out(cx: &mut TestAppContext) {
    let vault = vault_with(&["a.md", "b.md"]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open_tabs(&workspace, cx, &["a.md", "b.md"]);
    run(&workspace, cx, "pane.move-tab-right");
    let divider = bounds(cx, "divider-0");
    let none = Modifiers::none();
    // Dragging stops short of the minimum pane width.
    let far_left = point(px(10.), divider.center().y);
    cx.simulate_mouse_down(divider.center(), MouseButton::Left, none);
    cx.simulate_mouse_move(far_left, MouseButton::Left, none);
    cx.simulate_mouse_up(far_left, MouseButton::Left, none);
    let ratio = saved_layout(&workspace, cx).ratio.unwrap();
    assert!(ratio > 0.1 && ratio < 0.5, "{ratio}");
    let divider = bounds(cx, "divider-0");
    cx.simulate_event(MouseDownEvent {
        position: divider.center(),
        button: MouseButton::Left,
        modifiers: none,
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_mouse_up(divider.center(), MouseButton::Left, none);
    assert_eq!(saved_layout(&workspace, cx).ratio, Some(0.5));
}

#[gpui::test]
fn splits_and_their_sizes_survive_a_restart(cx: &mut TestAppContext) {
    let vault = vault_with(&["a.md", "b.md", "c.md", "gone.md"]);
    {
        let (workspace, cx) = open_workspace(cx, vault.path());
        open_tabs(&workspace, cx, &["a.md", "b.md", "c.md", "gone.md"]);
        run(&workspace, cx, "pane.move-tab-right"); // a b c | gone
        run(&workspace, cx, "pane.focus-left");
        run(&workspace, cx, "tab.go-2");
        run(&workspace, cx, "pane.move-tab-down"); // (a c over b) | gone
        cx.update(|_, cx| workspace.update(cx, |workspace, cx| workspace.prepare_to_close(cx)));
    }
    std::fs::remove_file(vault.path().join("gone.md")).unwrap();
    let vault_path = vault.path().to_path_buf();
    let (restored, cx) = cx.add_window_view(move |window, cx| {
        let mut workspace = Workspace::new(&vault_path, window, cx);
        workspace.restore_session(window, cx);
        workspace
    });
    cx.run_until_parked();
    // The pane whose only note is gone closes; the stack stays.
    assert_eq!(layout(&restored, cx), vec![vec!["a", "c"], vec!["b"]]);
    assert_eq!(active_title(&restored, cx), "b");
    assert_eq!(saved_layout(&restored, cx).split, Some(SplitAxis::Column));
}

#[gpui::test]
fn a_split_starts_below_the_frontmatter(cx: &mut TestAppContext) {
    let vault = tempfile::tempdir().unwrap();
    let note = "---\ntags: [physics]\n---\n# Waves\n";
    std::fs::write(vault.path().join("waves.md"), note).unwrap();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open_tabs(&workspace, cx, &["waves.md"]);
    run(&workspace, cx, "pane.split-right");
    let [left, right] = panes(&workspace, cx).try_into().unwrap();
    let body = note.find("# Waves").unwrap();
    for pane in [left, right] {
        let cursor = cx.read(|cx| pane.read(cx).active_editor().unwrap().read(cx).cursor());
        assert_eq!(cursor, body, "the frontmatter shows as properties");
    }
}

/// A workspace with the file tree showing in the left panel.
fn open_workspace_with_tree<'a>(
    cx: &'a mut TestAppContext,
    vault: &Path,
) -> (Entity<Workspace>, &'a mut VisualTestContext) {
    let root = vault.to_path_buf();
    let (workspace, cx) = open_workspace(cx, vault);
    cx.update(|window, cx| {
        let options = FileTreeOptions {
            trash: TrashMode::Vault,
            watch: false,
        };
        let tree = cx.new(|cx| FileTree::with_options(root, options, window, cx));
        workspace.update(cx, |workspace, cx| workspace.set_file_tree(tree, cx));
    });
    run(&workspace, cx, "sidebar.files.show");
    (workspace, cx)
}

#[gpui::test]
fn tree_drop_a_note_on_the_tab_strip_opens_it_at_that_slot(cx: &mut TestAppContext) {
    let vault = vault_with(&["a.md", "b.md", "c.md"]);
    // Beside the notes, so the panel covers none of the tabs: without
    // the Mac's window buttons the strip starts under an open overlay.
    let config = vault.path().join(gasp_config::CONFIG_DIR);
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("settings.toml"),
        "[sidebar.files]\nmode = \"push\"\n",
    )
    .unwrap();
    let (workspace, cx) = open_workspace_with_tree(cx, vault.path());
    open_tabs(&workspace, cx, &["a.md", "c.md"]);
    let row = bounds(cx, "tree-row-b");
    let c = bounds(cx, "tab-c");
    drag(cx, row.center(), point(c.left() + px(4.), c.center().y));
    assert_eq!(layout(&workspace, cx), vec![vec!["a", "b", "c"]]);
    assert_eq!(active_title(&workspace, cx), "b");
    // A note the pane already shows gets its tab shown, not a second one.
    let row = bounds(cx, "tree-row-a");
    let strip = bounds(cx, "tab-c");
    drag(
        cx,
        row.center(),
        point(strip.right() - px(4.), strip.center().y),
    );
    assert_eq!(layout(&workspace, cx), vec![vec!["a", "b", "c"]]);
    assert_eq!(active_title(&workspace, cx), "a");
}

#[gpui::test]
fn tree_drop_a_note_on_an_edge_of_the_note_splits_the_pane(cx: &mut TestAppContext) {
    let vault = vault_with(&["a.md", "b.md"]);
    let (workspace, cx) = open_workspace_with_tree(cx, vault.path());
    open_tabs(&workspace, cx, &["a.md"]);
    let surface = bounds(cx, "pane-surface");
    let row = bounds(cx, "tree-row-b");
    drag(
        cx,
        row.center(),
        point(surface.right() - px(20.), surface.center().y),
    );
    assert_eq!(layout(&workspace, cx), vec![vec!["a"], vec!["b"]]);
    assert_eq!(active_title(&workspace, cx), "b");
}

#[gpui::test]
fn tree_drop_a_selection_opens_its_notes_and_skips_folders(cx: &mut TestAppContext) {
    let vault = vault_with(&["a.md", "b.md", "c.md"]);
    std::fs::create_dir(vault.path().join("Folder")).unwrap();
    let (workspace, cx) = open_workspace_with_tree(cx, vault.path());
    open_tabs(&workspace, cx, &["a.md"]);
    let surface = bounds(cx, "pane-surface");
    let middle = surface.center();
    // A folder on its own lands nowhere.
    let folder = bounds(cx, "tree-row-Folder");
    drag(cx, folder.center(), middle);
    assert_eq!(layout(&workspace, cx), vec![vec!["a"]]);
    assert!(vault.path().join("Folder").is_dir());
    for row in ["tree-row-Folder", "tree-row-b", "tree-row-c"] {
        let row = bounds(cx, row);
        cx.simulate_click(row.center(), Modifiers::secondary_key());
    }
    let row = bounds(cx, "tree-row-c");
    drag(cx, row.center(), middle);
    assert_eq!(layout(&workspace, cx), vec![vec!["a", "b", "c"]]);
    assert!(vault.path().join("b.md").is_file(), "the notes stay put");
}
