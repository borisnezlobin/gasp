//! The right sidebar, daily notes, templates and link updates on rename,
//! in a real workspace: every panel is reached by its command and by its
//! buttons, and what it shows follows the active note and the index.

use editor_config::CONFIG_DIR;
use std::path::{Path, PathBuf};

use editor_config::{Platform, RuleSet};
use editor_desktop::actions::bind_keys;
use editor_desktop::features;
use editor_desktop::keymap::all_bindings;
use editor_desktop::knowledge::sidebar::{Row, describe};
use editor_desktop::knowledge::{KnowledgeSidebar, SidebarView, dates, is_actionable};
use editor_desktop::vault_search::VaultSearch;
use editor_desktop::workspace::{OpenIn, Workspace};
use gpui::{Entity, Focusable, Modifiers, TestAppContext, VisualTestContext};
use tempfile::TempDir;

const VAULT_SETTINGS: &str = concat!(editor_config::config_dir!(), "/settings.toml");

fn vault_with(notes: &[(&str, &str)]) -> TempDir {
    let vault = tempfile::tempdir().unwrap();
    for (name, text) in notes {
        let path = vault.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    vault
}

fn open_workspace<'a>(
    cx: &'a mut TestAppContext,
    vault: &Path,
) -> (Entity<Workspace>, &'a mut VisualTestContext) {
    cx.update(|cx| {
        bind_keys(cx);
        features::bind_view_keys(cx);
    });
    let vault = vault.to_path_buf();
    let (workspace, cx) = cx.add_window_view(move |window, cx| {
        let mut workspace = Workspace::new(&vault, window, cx);
        features::install(&mut workspace, window, cx);
        workspace
    });
    cx.run_until_parked();
    (workspace, cx)
}

fn key_for(command: &str) -> String {
    all_bindings(&RuleSet::defaults(), Platform::current())
        .into_iter()
        .find(|binding| binding.command == command)
        .map(|binding| binding.keystroke)
        .unwrap_or_else(|| panic!("{command} has no key"))
}

fn press(cx: &mut VisualTestContext, command: &str) {
    cx.simulate_keystrokes(&key_for(command));
    cx.run_until_parked();
}

fn run(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, id: &str) {
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| workspace.run_command(id, window, cx))
    });
    cx.run_until_parked();
}

fn open(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, name: &str) {
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            let path = workspace.vault().join(name);
            workspace
                .open_path(&path, OpenIn::ActiveTab, window, cx)
                .unwrap()
        })
    });
    cx.run_until_parked();
}

fn root(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> PathBuf {
    cx.read(|cx| workspace.read(cx).vault().to_path_buf())
}

fn sidebar(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<KnowledgeSidebar> {
    cx.read(|cx| {
        let view = workspace.read(cx).right_panel().view().unwrap().clone();
        view.downcast::<KnowledgeSidebar>().ok().unwrap()
    })
}

fn rows(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    let sidebar = sidebar(workspace, cx);
    cx.read(|cx| describe(sidebar.read(cx).rows()))
}

fn is_open(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    cx.read(|cx| workspace.read(cx).right_panel().is_visible())
}

fn active_text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.read(|cx| {
        workspace
            .read(cx)
            .active_editor(cx)
            .unwrap()
            .read(cx)
            .text()
    })
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("nothing drawn as {selector}"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

/// The index of the first row `describe` gives as `wanted`.
fn row_index(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, wanted: &str) -> usize {
    rows(workspace, cx)
        .iter()
        .position(|row| row == wanted)
        .unwrap_or_else(|| panic!("no row {wanted} in {:?}", rows(workspace, cx)))
}

fn click_row(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, wanted: &str) {
    let index = row_index(workspace, cx, wanted);
    let selector: &'static str = Box::leak(format!("knowledge-row-{index}").into_boxed_str());
    click(cx, selector);
}

const TARGET: &str = "# Target\n\nBody.\n\n## Part\n\nMore.\n";

fn linked_vault() -> TempDir {
    vault_with(&[
        ("Target.md", TARGET),
        ("A.md", "- See [[Target]] here.\n"),
        ("folder/B.md", "Also [[Target#Part|the part]].\n"),
        ("C.md", "Mentions target without a link. #physics/waves\n"),
        ("D.md", "Nothing here. #physics\n"),
    ])
}

#[gpui::test]
fn backlinks_show_linking_notes_and_open_at_the_link(cx: &mut TestAppContext) {
    let vault = linked_vault();
    let (workspace, cx) = open_workspace(cx, vault.path());
    assert!(!is_open(&workspace, cx));
    open(&workspace, cx, "Target.md");
    press(cx, "sidebar.backlinks");
    assert!(is_open(&workspace, cx));
    assert_eq!(
        rows(&workspace, cx),
        [
            "summary: 2 notes link here",
            "note: A",
            "link: See Target here.",
            "note: B",
            "link: Also the part.",
            "unlinked: false None",
        ]
    );
    click_row(&workspace, cx, "link: Also the part.");
    assert_eq!(
        active_text(&workspace, cx),
        "Also [[Target#Part|the part]].\n"
    );
    let cursor = cx.read(|cx| {
        workspace
            .read(cx)
            .active_editor(cx)
            .unwrap()
            .read(cx)
            .cursor()
    });
    assert_eq!(cursor, 5);
    // B links nowhere else, so its own backlinks are empty.
    assert_eq!(rows(&workspace, cx)[0], "summary: No notes link here yet.");
}

#[gpui::test]
fn unlinked_mentions_turn_into_links(cx: &mut TestAppContext) {
    let vault = linked_vault();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "Target.md");
    press(cx, "sidebar.backlinks");
    click_row(&workspace, cx, "unlinked: false None");
    let found = rows(&workspace, cx);
    assert_eq!(
        found[5..],
        [
            "unlinked: true Some(1)",
            "note: C",
            "mention: Mentions target without a link. #physics/waves",
        ]
    );
    click(cx, "button-Link");
    let text = std::fs::read_to_string(root(&workspace, cx).join("C.md")).unwrap();
    assert_eq!(
        text,
        "Mentions [[Target|target]] without a link. #physics/waves\n"
    );
    // The index heard about it, so C is a backlink now and not a mention.
    let after = rows(&workspace, cx);
    assert_eq!(after[0], "summary: 3 notes link here");
    assert!(
        after.contains(&"unlinked: true None".to_string()),
        "{after:?}"
    );
}

#[gpui::test]
fn outgoing_links_follow_or_create_notes(cx: &mut TestAppContext) {
    let vault = vault_with(&[
        ("Source.md", "[[Target]] and [[Nowhere yet]].\n"),
        ("Target.md", "target"),
    ]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "Source.md");
    press(cx, "sidebar.outgoing-links");
    assert_eq!(
        rows(&workspace, cx),
        [
            "summary: 2 links go out, 1 to a note that doesn’t exist yet",
            "out: Target true",
            "out: Nowhere yet false",
        ]
    );
    click_row(&workspace, cx, "out: Nowhere yet false");
    assert!(root(&workspace, cx).join("Nowhere yet.md").is_file());
    assert_eq!(active_text(&workspace, cx), "");
}

#[gpui::test]
fn the_outline_marks_the_current_heading_and_jumps(cx: &mut TestAppContext) {
    let vault = vault_with(&[("Target.md", TARGET)]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "Target.md");
    press(cx, "sidebar.outline");
    assert_eq!(
        rows(&workspace, cx),
        ["heading: 0 Target true", "heading: 1 Part false"]
    );
    click_row(&workspace, cx, "heading: 1 Part false");
    let cursor = cx.read(|cx| {
        workspace
            .read(cx)
            .active_editor(cx)
            .unwrap()
            .read(cx)
            .cursor()
    });
    assert_eq!(cursor, TARGET.find("## Part").unwrap());
    assert_eq!(
        rows(&workspace, cx),
        ["heading: 0 Target false", "heading: 1 Part true"]
    );
    // Typing a heading shows once typing pauses.
    let editor = cx.read(|cx| workspace.read(cx).active_editor(cx).unwrap());
    let end = TARGET.len();
    editor.update(cx, |editor, cx| {
        editor.replace(end..end, "\n## Added\n", cx)
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    cx.run_until_parked();
    assert_eq!(rows(&workspace, cx).len(), 3);
}

#[gpui::test]
fn tags_nest_and_search_the_vault(cx: &mut TestAppContext) {
    let vault = linked_vault();
    let (workspace, cx) = open_workspace(cx, vault.path());
    press(cx, "sidebar.tags");
    assert_eq!(
        rows(&workspace, cx),
        [
            "summary: 1 tag",
            "tag: physics 2 false",
            "tag: physics/waves 1 false",
        ]
    );
    click(cx, "knowledge-tag-toggle-physics");
    assert_eq!(rows(&workspace, cx).len(), 2);
    click_row(&workspace, cx, "tag: physics 2 true");
    let search = cx.read(|cx| workspace.read(cx).active_modal::<VaultSearch>());
    let search = search.expect("the tag opens vault search");
    let results = cx.read(|cx| search.read(cx).results().len());
    assert_eq!(results, 2);
}

#[gpui::test]
fn a_tag_finds_notes_that_have_it_only_in_their_frontmatter(cx: &mut TestAppContext) {
    let vault = vault_with(&[
        ("Front.md", "---\ntags: [optics]\n---\nLenses.\n"),
        ("Inline.md", "Mirrors #optics/mirrors\n"),
        ("Plain.md", "The word optics, untagged.\n"),
    ]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    press(cx, "sidebar.tags");
    click_row(&workspace, cx, "tag: optics 2 false");
    let search = cx.read(|cx| workspace.read(cx).active_modal::<VaultSearch>());
    let search = search.expect("the tag opens vault search");
    let found: Vec<String> = cx.read(|cx| {
        let search = search.read(cx);
        search
            .results()
            .iter()
            .map(|result| result.path.to_string_lossy().into_owned())
            .collect()
    });
    assert_eq!(found, ["Front.md", "Inline.md"]);
}

#[gpui::test]
fn buttons_and_commands_show_hide_and_remember_the_sidebar(cx: &mut TestAppContext) {
    let vault = linked_vault();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "Target.md");
    // Hidden, the tab bar has the button that shows it.
    click(cx, "pane-right-sidebar-toggle");
    assert!(is_open(&workspace, cx));
    click(cx, "knowledge-outline");
    let sidebar = sidebar(&workspace, cx);
    let view = cx.read(|cx| sidebar.read(cx).view());
    assert_eq!(view, SidebarView::Outline);
    // The same view's command again hides it; the toggle brings it back.
    press(cx, "sidebar.outline");
    assert!(!is_open(&workspace, cx));
    press(cx, "sidebar.right.toggle");
    assert!(is_open(&workspace, cx));
    let state = cx.read(|cx| workspace.read(cx).device_state(cx).right_sidebar);
    assert!(state.open);
    assert_eq!(state.view, "outline");
    click(cx, "knowledge-hide");
    assert!(!is_open(&workspace, cx));
    let state = cx.read(|cx| workspace.read(cx).device_state(cx).right_sidebar);
    assert!(!state.open);
}

#[gpui::test]
fn the_sidebar_opens_where_it_was_left(cx: &mut TestAppContext) {
    let vault = linked_vault();
    std::fs::create_dir_all(vault.path().join(CONFIG_DIR)).unwrap();
    std::fs::write(
        vault.path().join(CONFIG_DIR).join("device.toml"),
        "[right-sidebar]\nopen = true\nview = \"tags\"\nwidth = 300\n",
    )
    .unwrap();
    let (workspace, cx) = open_workspace(cx, vault.path());
    assert!(is_open(&workspace, cx));
    assert_eq!(rows(&workspace, cx)[0], "summary: 1 tag");
    let width = cx.read(|cx| workspace.read(cx).right_panel().width);
    assert_eq!(width, gpui::px(300.));
}

#[gpui::test]
fn renaming_from_the_title_updates_links(cx: &mut TestAppContext) {
    let vault = vault_with(&[
        ("Old name.md", "old"),
        ("A.md", "See [[Old name]] and [[Old name#Part|part]].\n"),
        ("sub/B.md", "[b](../Old%20name.md)\n"),
        ("C.md", "Unrelated [[Other]].\n"),
    ]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "A.md");
    open_in_new_tab(&workspace, cx, "Old name.md");
    run(&workspace, cx, "note.rename");
    cx.simulate_input("New name");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let root = root(&workspace, cx);
    assert!(root.join("New name.md").is_file());
    // A is open, so its editor has the change, as one undoable edit.
    let editor = cx.read(|cx| {
        let pane = workspace.read(cx).active_pane().read(cx);
        pane.tabs()[0].note().unwrap().editor.clone()
    });
    let text = cx.read(|cx| editor.read(cx).text());
    assert_eq!(text, "See [[New name]] and [[New name#Part|part]].\n");
    editor.update(cx, |editor, cx| editor.undo(cx));
    let undone = cx.read(|cx| editor.read(cx).text());
    assert_eq!(undone, "See [[Old name]] and [[Old name#Part|part]].\n");
    // B isn't open, so its file changed.
    let b = std::fs::read_to_string(root.join("sub/B.md")).unwrap();
    assert_eq!(b, "[b](../New%20name.md)\n");
    let c = std::fs::read_to_string(root.join("C.md")).unwrap();
    assert_eq!(c, "Unrelated [[Other]].\n");
}

#[gpui::test]
fn renaming_leaves_links_alone_when_the_setting_is_off(cx: &mut TestAppContext) {
    let vault = vault_with(&[
        ("Old.md", ""),
        ("A.md", "[[Old]]"),
        (VAULT_SETTINGS, "[files]\nupdate-links-on-rename = false\n"),
    ]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "Old.md");
    run(&workspace, cx, "note.rename");
    cx.simulate_input("New");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let root = root(&workspace, cx);
    assert!(root.join("New.md").is_file());
    assert_eq!(
        std::fs::read_to_string(root.join("A.md")).unwrap(),
        "[[Old]]"
    );
}

#[gpui::test]
fn moving_in_the_file_tree_updates_links_like_a_rename(cx: &mut TestAppContext) {
    let vault = vault_with(&[
        ("Plan.md", "Back to [a](A.md).\n"),
        ("A.md", "See [[Plan]] and [p](Plan.md).\n"),
        ("sub/B.md", "[[Plan#Goals|goals]]\n"),
        ("Archive/.keep", ""),
    ]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "A.md");
    let root = root(&workspace, cx);
    let tree = cx.read(|cx| workspace.read(cx).file_tree().unwrap().clone());
    tree.update(cx, |tree, cx| {
        tree.move_into(&root.join("Plan.md"), &root.join("Archive"), cx)
    });
    cx.run_until_parked();
    assert!(root.join("Archive/Plan.md").is_file());
    // A is open: its editor has the change, as one undoable edit, and
    // its file is untouched until it saves.
    assert_eq!(
        active_text(&workspace, cx),
        "See [[Plan]] and [p](Archive/Plan.md).\n"
    );
    let editor = cx.read(|cx| workspace.read(cx).active_editor(cx).unwrap());
    editor.update(cx, |editor, cx| editor.undo(cx));
    assert_eq!(
        cx.read(|cx| editor.read(cx).text()),
        "See [[Plan]] and [p](Plan.md).\n"
    );
    // The note that moved keeps its own relative link working.
    assert_eq!(
        std::fs::read_to_string(root.join("Archive/Plan.md")).unwrap(),
        "Back to [a](../A.md).\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("sub/B.md")).unwrap(),
        "[[Plan#Goals|goals]]\n",
        "a name that still finds the note stays as written"
    );
}

fn open_in_new_tab(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, name: &str) {
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            let path = workspace.vault().join(name);
            workspace
                .open_path(&path, OpenIn::NewTab, window, cx)
                .unwrap()
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn todays_note_starts_from_its_template(cx: &mut TestAppContext) {
    let vault = vault_with(&[
        ("Templates/Day.md", "# {{date:YYYY}}\n\n{{title}}\n"),
        (
            VAULT_SETTINGS,
            "[daily-notes]\nfolder = \"Journal\"\ntemplate = \"Templates/Day\"\n",
        ),
    ]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    click(cx, "pane-right-sidebar-toggle");
    run(&workspace, cx, "daily.open");
    let today = dates::format(dates::now(), "YYYY-MM-DD");
    let path = root(&workspace, cx).join(format!("Journal/{today}.md"));
    assert!(path.is_file());
    let year = dates::format(dates::now(), "YYYY");
    assert_eq!(
        active_text(&workspace, cx),
        format!("# {year}\n\n{today}\n")
    );
    // Opening it again keeps what's there.
    std::fs::write(&path, "kept").unwrap();
    press(cx, "daily.open");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "kept");
}

#[gpui::test]
fn templates_go_in_at_the_cursor(cx: &mut TestAppContext) {
    let vault = vault_with(&[
        ("Templates/Greeting.md", "Hi {{title}}, {{date:YYYY}}."),
        ("Templates/Other.md", "other"),
        ("Note.md", "ab"),
    ]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "Note.md");
    let editor = cx.read(|cx| workspace.read(cx).active_editor(cx).unwrap());
    editor.update(cx, |editor, cx| editor.select(1, 1, cx));
    press(cx, "template.insert");
    cx.simulate_input("greet");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let year = dates::format(dates::now(), "YYYY");
    assert_eq!(active_text(&workspace, cx), format!("aHi Note, {year}.b"));
}

#[gpui::test]
fn the_sidebar_says_what_to_do_without_a_note(cx: &mut TestAppContext) {
    let vault = vault_with(&[]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    press(cx, "sidebar.backlinks");
    assert_eq!(
        rows(&workspace, cx),
        ["message: Open a note to see the notes that link to it."]
    );
    press(cx, "sidebar.tags");
    assert_eq!(
        rows(&workspace, cx),
        ["message: No notes have tags yet. Write #tag in a note to add one."]
    );
    let sidebar = sidebar(&workspace, cx);
    let heading = cx.read(|cx| {
        sidebar
            .read(cx)
            .rows()
            .iter()
            .any(|row| matches!(row, Row::Heading { .. }))
    });
    assert!(!heading);
}

fn cursor(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> usize {
    cx.read(|cx| {
        workspace
            .read(cx)
            .active_editor(cx)
            .unwrap()
            .read(cx)
            .cursor()
    })
}

fn editor_focused(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| {
        let editor = workspace.read(cx).active_editor(cx).unwrap();
        editor.focus_handle(cx).is_focused(window)
    })
}

#[gpui::test]
fn the_sidebar_is_driven_from_the_keyboard(cx: &mut TestAppContext) {
    let vault = vault_with(&[("Target.md", TARGET)]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "Target.md");
    press(cx, "sidebar.outline");
    press(cx, "sidebar.right.focus");
    let sidebar = sidebar(&workspace, cx);
    // It starts on the heading the cursor is under.
    assert_eq!(cx.read(|cx| sidebar.read(cx).selected()), Some(0));
    assert!(!editor_focused(&workspace, cx));
    cx.simulate_keystrokes("down");
    assert_eq!(cx.read(|cx| sidebar.read(cx).selected()), Some(1));
    // Past the end the selection stays put.
    cx.simulate_keystrokes("down");
    assert_eq!(cx.read(|cx| sidebar.read(cx).selected()), Some(1));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(cursor(&workspace, cx), TARGET.find("## Part").unwrap());
    assert!(editor_focused(&workspace, cx));
    // Escape hands the keyboard back without doing anything.
    press(cx, "sidebar.right.focus");
    assert!(!editor_focused(&workspace, cx));
    cx.simulate_keystrokes("up escape");
    cx.run_until_parked();
    assert!(editor_focused(&workspace, cx));
    assert_eq!(cursor(&workspace, cx), TARGET.find("## Part").unwrap());
}

#[gpui::test]
fn focusing_the_sidebar_opens_it_and_skips_rows_that_do_nothing(cx: &mut TestAppContext) {
    let vault = linked_vault();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "Target.md");
    assert!(!is_open(&workspace, cx));
    press(cx, "sidebar.right.focus");
    assert!(is_open(&workspace, cx));
    let sidebar = sidebar(&workspace, cx);
    let (selected, rows) = cx.read(|cx| {
        let sidebar = sidebar.read(cx);
        (sidebar.selected(), sidebar.rows().to_vec())
    });
    let selected = selected.expect("a row is selected");
    assert!(is_actionable(&rows[selected]));
    assert!(rows[..selected].iter().all(|row| !is_actionable(row)));
}
