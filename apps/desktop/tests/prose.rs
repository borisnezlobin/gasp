//! Prose in a real workspace: sentence-length tints and their toggle, the
//! grammar checker's flags with Accept and Ignore, and recovering an
//! earlier version of a note from its snapshots.

use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use editor_config::Platform;
use editor_config::RuleSet;
use editor_desktop::EditorView;
use editor_desktop::actions::bind_keys;
use editor_desktop::features;
use editor_desktop::hover::PreviewContent;
use editor_desktop::keymap::all_bindings;
use editor_desktop::prose::CHECK_DELAY;
use editor_desktop::recovery::dialog::RecoveryDialog;
use editor_desktop::recovery::store::use_data_dir;
use editor_desktop::workspace::{OpenIn, Workspace};
use editor_prose::{FlagKind, Length};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use tempfile::TempDir;

/// Snapshots from these tests go to a folder of their own, never the
/// real data folder.
fn data_dir() -> &'static Path {
    static DIR: OnceLock<TempDir> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        use_data_dir(dir.path().to_path_buf());
        dir
    })
    .path()
}

fn vault_with(text: &str) -> TempDir {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("Note.md"), text).unwrap();
    vault
}

fn open_workspace<'a>(
    cx: &'a mut TestAppContext,
    vault: &Path,
) -> (Entity<Workspace>, &'a mut VisualTestContext) {
    data_dir();
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
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            let path = workspace.vault().join("Note.md");
            workspace
                .open_path(&path, OpenIn::ActiveTab, window, cx)
                .unwrap();
            workspace.focus_active(window, cx);
        })
    });
    cx.run_until_parked();
    (workspace, cx)
}

fn editor(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<EditorView> {
    cx.read(|cx| workspace.read(cx).active_editor(cx).unwrap())
}

fn press(cx: &mut VisualTestContext, command: &str) {
    let key = all_bindings(&RuleSet::defaults(), Platform::current())
        .into_iter()
        .find(|binding| binding.command == command)
        .map(|binding| binding.keystroke)
        .unwrap_or_else(|| panic!("{command} has no key"));
    cx.simulate_keystrokes(&key);
    cx.run_until_parked();
}

fn run(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, id: &str) {
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| workspace.run_command(id, window, cx))
    });
    cx.run_until_parked();
}

fn tinted(editor: &Entity<EditorView>, cx: &mut VisualTestContext) -> Vec<(String, Length)> {
    editor.read_with(cx, |view, _| {
        let text = view.text();
        view.tinted_sentences()
            .iter()
            .map(|(range, length)| (text[range.clone()].to_owned(), *length))
            .collect()
    })
}

#[gpui::test]
fn sentences_are_tinted_by_length_and_the_key_toggles_them(cx: &mut TestAppContext) {
    let long = "This sentence keeps going on and on for quite a while, well past the point \
                where a reader would like a full stop.";
    let text = format!(
        "# A heading that is not tinted\n\nShort one. {long}\n\n```\ncode. More code.\n```\n"
    );
    let vault = vault_with(&text);
    let (workspace, cx) = open_workspace(cx, vault.path());
    let editor = editor(&workspace, cx);
    assert_eq!(
        tinted(&editor, cx),
        [
            ("Short one.".to_owned(), Length::Short),
            (long.to_owned(), Length::Long)
        ]
    );
    press(cx, "prose.toggle-sentence-highlighting");
    assert!(!editor.read_with(cx, |view, _| view.is_highlighting_sentences()));
    cx.run_until_parked();
    assert!(tinted(&editor, cx).is_empty());
    let settings = std::fs::read_to_string(vault.path().join(".editor/settings.toml")).unwrap();
    assert!(settings.contains("enabled = false"), "{settings}");
    press(cx, "prose.toggle-sentence-highlighting");
    assert!(editor.read_with(cx, |view, _| view.is_highlighting_sentences()));
}

#[gpui::test]
fn typing_resegments_only_what_changed(cx: &mut TestAppContext) {
    let vault = vault_with("One two three four five six seven.\n\nOther paragraph.\n");
    let (workspace, cx) = open_workspace(cx, vault.path());
    let editor = editor(&workspace, cx);
    assert_eq!(tinted(&editor, cx)[0].1, Length::Medium);
    editor.update(cx, |view, cx| view.replace(3..33, "", cx));
    cx.run_until_parked();
    assert_eq!(
        tinted(&editor, cx),
        [
            ("One.".to_owned(), Length::Short),
            ("Other paragraph.".to_owned(), Length::Short)
        ]
    );
}

/// Waits, on the real clock, for the grammar worker (a thread of its own)
/// to answer, moving the fake clock past the pause each round.
fn wait_for_flags(editor: &Entity<EditorView>, cx: &mut VisualTestContext) {
    let started = Instant::now();
    while editor.read_with(cx, |view, _| view.shown_flags().is_empty()) {
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "no flags came"
        );
        cx.executor().advance_clock(CHECK_DELAY * 2);
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(20));
        cx.run_until_parked();
    }
}

#[gpui::test]
fn flags_underline_problems_and_accept_or_ignore_them(cx: &mut TestAppContext) {
    let vault = vault_with("It was the the end of a mispeled day.\n\nRun `teh` in code.\n");
    let (workspace, cx) = open_workspace(cx, vault.path());
    let editor = editor(&workspace, cx);
    editor.update(cx, |view, cx| view.move_to(0, false, cx));
    wait_for_flags(&editor, cx);
    let flags = editor.read_with(cx, |view, _| view.shown_flags().to_vec());
    let text = editor.read_with(cx, |view, _| view.text());
    let words: Vec<&str> = flags.iter().map(|flag| &text[flag.range.clone()]).collect();
    assert_eq!(words, ["the the", "mispeled"], "code is never checked");
    assert_eq!(flags[1].kind, FlagKind::Spelling);

    // Accepting the first fix takes the doubled word out.
    editor.update(cx, |view, cx| view.accept_flag(&flags[0], "the", cx));
    cx.run_until_parked();
    assert!(
        editor
            .read_with(cx, |view, _| view.text())
            .starts_with("It was the end")
    );

    // Ignoring a phrase hides it at once and remembers it in the vault.
    let flag = editor.read_with(cx, |view, _| {
        view.shown_flags()
            .iter()
            .find(|flag| flag.kind == FlagKind::Spelling)
            .cloned()
    });
    let flag = match flag {
        Some(flag) => flag,
        None => {
            wait_for_flags(&editor, cx);
            editor.read_with(cx, |view, _| view.shown_flags()[0].clone())
        }
    };
    editor.update(cx, |view, cx| view.ignore_flag(&flag, cx));
    cx.run_until_parked();
    assert!(editor.read_with(cx, |view, _| view.shown_flags().is_empty()));
    let ignored = std::fs::read_to_string(vault.path().join(".editor/prose/ignored.txt")).unwrap();
    assert!(ignored.lines().any(|line| line == "mispeled"), "{ignored}");
}

#[gpui::test]
fn an_earlier_version_can_be_recovered(cx: &mut TestAppContext) {
    let vault = vault_with("The first draft.\n");
    let (workspace, cx) = open_workspace(cx, vault.path());
    let editor = editor(&workspace, cx);
    editor.update(cx, |view, cx| {
        let all = 0..view.doc().len();
        view.replace(all, "A rewrite I regret.\n", cx)
    });
    // Autosave keeps the version it replaces.
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    let saved = std::fs::read_to_string(vault.path().join("Note.md")).unwrap();
    assert_eq!(saved, "A rewrite I regret.\n");

    run(&workspace, cx, "note.recover");
    let dialog = cx
        .read(|cx| workspace.read(cx).active_modal::<RecoveryDialog>())
        .expect("the dialog opens");
    cx.run_until_parked();
    let versions = dialog.read_with(cx, |dialog, _| dialog.versions().to_vec());
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].text, "The first draft.\n");

    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        editor.read_with(cx, |view, _| view.text()),
        "The first draft.\n"
    );
    assert!(
        cx.read(|cx| workspace.read(cx).active_modal::<RecoveryDialog>())
            .is_none()
    );
    // Undo takes the restore back.
    editor.update(cx, |view, cx| view.undo(cx));
    assert_eq!(
        editor.read_with(cx, |view, _| view.text()),
        "A rewrite I regret.\n"
    );
}

#[gpui::test]
fn resting_on_a_flag_shows_what_is_wrong(cx: &mut TestAppContext) {
    let vault = vault_with("It was the the end.\n");
    let (workspace, cx) = open_workspace(cx, vault.path());
    let editor = editor(&workspace, cx);
    editor.update(cx, |view, cx| view.move_to(0, false, cx));
    wait_for_flags(&editor, cx);
    let (flag, bounds) = editor.read_with(cx, |view, _| {
        let flag = view.shown_flags()[0].clone();
        let bounds = view.frame().unwrap().range_bounds(&flag.range).unwrap();
        (flag, bounds)
    });
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    let shown = editor.read_with(cx, |view, _| {
        match view.hover_preview().map(|p| &p.content) {
            Some(PreviewContent::Flag(shown)) => Some(shown.clone()),
            _ => None,
        }
    });
    assert_eq!(shown, Some(flag));
}
