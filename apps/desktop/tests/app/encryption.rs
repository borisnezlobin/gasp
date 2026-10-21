//! Encrypted notes in the workspace: they open locked and never show
//! their sealed text, unlock with the password, save sealed, lock again
//! on command or after the window has been away, stay out of previews
//! and never get a recovery snapshot.

use std::path::Path;
use std::time::Duration;

use gasp_crypt::{KdfParams, NoteOptions, is_encrypted};
use gasp_desktop::actions::bind_keys;
use gasp_desktop::encryption::dialogs::PasswordDialog;
use gasp_desktop::workspace::note_doc::AUTOSAVE_DELAY;
use gasp_desktop::workspace::note_shape::{ShapeLine, read_shape};
use gasp_desktop::workspace::{OpenIn, Workspace};
use gasp_vault::encryption::Encryption;
use gpui::{Entity, TestAppContext, VisualTestContext};
use tempfile::TempDir;

const PASSWORD: &str = "tide pool";
const SECRET: &str = "The whale sings at dawn.";

/// A vault with a password, an encrypted `secret.md` and a plain
/// `plain.md`.
fn encrypted_vault() -> TempDir {
    let vault = tempfile::tempdir().unwrap();
    let mut encryption = Encryption::load_with(vault.path(), KdfParams::QUICK);
    encryption.set_vault_password(PASSWORD).unwrap();
    let sealed = encryption
        .encrypt("secret.md", SECRET, NoteOptions::default(), None)
        .unwrap();
    std::fs::write(vault.path().join("secret.md"), sealed).unwrap();
    std::fs::write(vault.path().join("plain.md"), "Plain words").unwrap();
    vault
}

fn open_workspace<'a>(
    cx: &'a mut TestAppContext,
    vault: &Path,
) -> (Entity<Workspace>, &'a mut VisualTestContext) {
    cx.update(|cx| {
        bind_keys(cx);
        gasp_desktop::encryption::use_quick_keys(cx);
    });
    let vault = vault.to_path_buf();
    let (workspace, cx) = cx.add_window_view(move |window, cx| Workspace::new(&vault, window, cx));
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    (workspace, cx)
}

fn open(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, name: &str) {
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace
                .open_path(Path::new(name), OpenIn::ActiveTab, window, cx)
                .unwrap()
        })
    });
    cx.run_until_parked();
}

fn run(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, id: &str) {
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| workspace.run_command(id, window, cx))
    });
    cx.run_until_parked();
}

fn editor_text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.read(|cx| {
        let editor = workspace.read(cx).active_editor(cx).unwrap();
        editor.read(cx).text()
    })
}

fn is_locked(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    cx.read(|cx| {
        let pane = workspace.read(cx).active_pane().read(cx);
        let note = pane.active_tab().unwrap().note().unwrap();
        note.doc.read(cx).is_locked()
    })
}

/// Types `password` on the active tab's lock screen and presses Unlock.
/// Answers the lock screen's error, if any.
fn unlock(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    password: &str,
) -> Option<String> {
    let screen = cx.read(|cx| {
        let pane = workspace.read(cx).active_pane().read(cx);
        pane.active_tab().unwrap().note().unwrap().lock_screen.clone()
    });
    cx.update(|window, cx| {
        screen.update(cx, |screen, cx| {
            let field = screen.password_field().clone();
            field.update(cx, |field, cx| field.set_text(password, cx));
            screen.unlock(window, cx);
        })
    });
    cx.run_until_parked();
    cx.read(|cx| screen.read(cx).error().map(str::to_owned))
}

fn advance(cx: &mut VisualTestContext, duration: Duration) {
    cx.executor().advance_clock(duration);
    cx.run_until_parked();
}

fn read(vault: &TempDir, name: &str) -> String {
    std::fs::read_to_string(vault.path().join(name)).unwrap()
}

#[gpui::test]
fn an_encrypted_note_opens_locked_and_never_shows_its_sealed_text(cx: &mut TestAppContext) {
    let vault = encrypted_vault();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "secret.md");
    assert!(is_locked(&workspace, cx));
    assert_eq!(editor_text(&workspace, cx), "");
    assert!(cx.debug_bounds("lock-screen").is_some(), "the locked view shows");
    let tab = cx.read(|cx| {
        let pane = workspace.read(cx).active_pane().read(cx);
        pane.active_tab().unwrap().title(cx)
    });
    assert_eq!(tab, "secret");
}

#[gpui::test]
fn the_password_unlocks_the_note_and_a_wrong_one_says_so(cx: &mut TestAppContext) {
    let vault = encrypted_vault();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "secret.md");
    let refused = unlock(&workspace, cx, "wrong");
    assert!(refused.is_some_and(|error| error.contains("doesn't unlock")));
    assert!(is_locked(&workspace, cx));
    assert_eq!(unlock(&workspace, cx, PASSWORD), None);
    assert!(!is_locked(&workspace, cx));
    assert_eq!(editor_text(&workspace, cx), SECRET);
}

#[gpui::test]
fn edits_to_an_unlocked_note_save_sealed(cx: &mut TestAppContext) {
    let vault = encrypted_vault();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "secret.md");
    unlock(&workspace, cx, PASSWORD);
    cx.simulate_input(" Krill too.");
    advance(cx, AUTOSAVE_DELAY * 2);
    assert!(editor_text(&workspace, cx).contains("Krill too."));
    let file = read(&vault, "secret.md");
    assert!(is_encrypted(&file));
    assert!(!file.contains("Krill") && !file.contains("whale"));
}

#[gpui::test]
fn the_lock_command_locks_open_notes_again(cx: &mut TestAppContext) {
    let vault = encrypted_vault();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "secret.md");
    unlock(&workspace, cx, PASSWORD);
    cx.simulate_input(" Unsaved.");
    run(&workspace, cx, "encryption.lock");
    assert!(is_locked(&workspace, cx));
    assert_eq!(editor_text(&workspace, cx), "");
    let file = read(&vault, "secret.md");
    assert!(is_encrypted(&file), "the edit was saved sealed before locking");
    assert_eq!(unlock(&workspace, cx, PASSWORD), None);
    let text = editor_text(&workspace, cx);
    assert!(text.contains(" Unsaved."), "{text:?}");
}

#[gpui::test]
fn notes_lock_once_the_window_has_been_away_long_enough(cx: &mut TestAppContext) {
    let vault = encrypted_vault();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "secret.md");
    unlock(&workspace, cx, PASSWORD);
    cx.deactivate_window();
    cx.run_until_parked();
    assert!(cx.debug_bounds("away-cover").is_some(), "covered while away");
    advance(cx, Duration::from_secs(60));
    assert!(!is_locked(&workspace, cx), "not before the delay");
    cx.update(|window, _| window.activate_window());
    advance(cx, Duration::from_secs(120));
    assert!(!is_locked(&workspace, cx), "coming back stops the relock");
    cx.deactivate_window();
    advance(cx, Duration::from_secs(121));
    assert!(is_locked(&workspace, cx));
    assert_eq!(editor_text(&workspace, cx), "");
}

#[gpui::test]
fn previews_never_show_an_encrypted_note(cx: &mut TestAppContext) {
    let vault = encrypted_vault();
    let (_workspace, _cx) = open_workspace(cx, vault.path());
    let sealed = read(&vault, "secret.md");
    assert_eq!(
        gasp_desktop::encryption::preview_text(&sealed),
        gasp_desktop::encryption::LOCKED_PREVIEW
    );
    assert_eq!(
        read_shape(&vault.path().join("secret.md"), 6),
        vec![ShapeLine::Locked]
    );
}

#[gpui::test]
fn encrypted_notes_get_no_recovery_snapshots(cx: &mut TestAppContext) {
    let vault = encrypted_vault();
    let (workspace, cx) = open_workspace(cx, vault.path());
    workspace.update(cx, |workspace, cx| {
        gasp_desktop::recovery::install(workspace, cx)
    });
    open(&workspace, cx, "secret.md");
    unlock(&workspace, cx, PASSWORD);
    cx.simulate_input(" More.");
    advance(cx, AUTOSAVE_DELAY * 2);
    open(&workspace, cx, "plain.md");
    cx.simulate_input(" More.");
    advance(cx, AUTOSAVE_DELAY * 2);
    let path = vault.path().join("secret.md");
    let snapshots = cx.read(|cx| {
        let (store, relative) = gasp_desktop::recovery::store_for(&path, cx).unwrap();
        store.list(&relative)
    });
    assert!(snapshots.is_empty(), "{snapshots:?}");
    let plain = vault.path().join("plain.md");
    let kept = cx.read(|cx| {
        let (store, relative) = gasp_desktop::recovery::store_for(&plain, cx).unwrap();
        store.list(&relative)
    });
    assert!(!kept.is_empty(), "plain notes still get them");
}

#[gpui::test]
fn encrypting_a_note_asks_for_a_vault_password_first(cx: &mut TestAppContext) {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("diary.md"), "Dear diary").unwrap();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "diary.md");
    run(&workspace, cx, "encryption.encrypt-note");
    let dialog = cx
        .read(|cx| workspace.read(cx).active_modal::<PasswordDialog>())
        .expect("the password form opened");
    cx.update(|window, cx| {
        dialog.update(cx, |dialog, cx| {
            dialog.fill(&["pw", "different"], cx);
            dialog.submit(window, cx);
        })
    });
    let error = cx.read(|cx| dialog.read(cx).error().map(str::to_owned));
    assert!(error.is_some_and(|error| error.contains("don’t match")));
    cx.update(|window, cx| {
        dialog.update(cx, |dialog, cx| {
            dialog.fill(&["pw", "pw"], cx);
            dialog.submit(window, cx);
        })
    });
    cx.run_until_parked();
    let file = read(&vault, "diary.md");
    assert!(is_encrypted(&file) && !file.contains("diary"));
    assert_eq!(editor_text(&workspace, cx), "Dear diary");
    run(&workspace, cx, "encryption.lock");
    assert!(is_locked(&workspace, cx));
}
