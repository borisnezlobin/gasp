//! Encrypted notes in the workspace: they open locked and never show
//! their sealed text, unlock with the password, save sealed, lock again
//! on command or after the window has been away, stay out of previews
//! and never get a recovery snapshot.

use std::path::Path;
use std::time::Duration;

use gasp_crypt::{KdfParams, NameVisibility, NoteOptions, is_encrypted};
use gasp_desktop::actions::bind_keys;
use gasp_desktop::encryption::dialogs::{NoteOptionsDialog, PasswordDialog, PasswordSource};
use gasp_desktop::encryption::lock_screen::LockScreen;
use gasp_desktop::workspace::note_doc::AUTOSAVE_DELAY;
use gasp_desktop::workspace::note_shape::{ShapeLine, read_shape};
use gasp_desktop::workspace::watcher::DiskChange;
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
        pane.active_tab()
            .unwrap()
            .note()
            .unwrap()
            .lock_screen
            .clone()
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
    assert!(
        cx.debug_bounds("lock-screen").is_some(),
        "the locked view shows"
    );
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
    assert!(
        is_encrypted(&file),
        "the edit was saved sealed before locking"
    );
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
    assert!(
        cx.debug_bounds("away-cover").is_some(),
        "covered while away"
    );
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

fn lock_screen(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<LockScreen> {
    cx.read(|cx| {
        let pane = workspace.read(cx).active_pane().read(cx);
        let note = pane.active_tab().unwrap().note().unwrap();
        note.lock_screen.clone()
    })
}

/// Types `password` and presses Unlock, leaving the key to be made.
fn submit_password(screen: &Entity<LockScreen>, cx: &mut VisualTestContext, password: &str) {
    cx.update(|window, cx| {
        screen.update(cx, |screen, cx| {
            let field = screen.password_field().clone();
            field.update(cx, |field, cx| field.set_text(password, cx));
            screen.unlock(window, cx);
        })
    });
}

#[gpui::test]
fn the_lock_screen_keeps_working_while_the_password_is_tried(cx: &mut TestAppContext) {
    let vault = encrypted_vault();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "secret.md");
    let screen = lock_screen(&workspace, cx);
    submit_password(&screen, cx, PASSWORD);
    assert!(cx.read(|cx| screen.read(cx).is_unlocking()));
    assert!(is_locked(&workspace, cx), "the key is still being made");
    // The main thread is free: the field still takes typing, and a second
    // submit while the first runs is ignored.
    submit_password(&screen, cx, "wrong");
    assert!(cx.read(|cx| screen.read(cx).is_unlocking()));
    cx.run_until_parked();
    assert!(!cx.read(|cx| screen.read(cx).is_unlocking()));
    assert!(!is_locked(&workspace, cx));
    assert_eq!(editor_text(&workspace, cx), SECRET);
}

#[gpui::test]
fn a_password_tried_while_the_vault_locks_again_opens_nothing(cx: &mut TestAppContext) {
    let vault = encrypted_vault();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "secret.md");
    let screen = lock_screen(&workspace, cx);
    submit_password(&screen, cx, PASSWORD);
    let note = cx.read(|cx| workspace.read(cx).vault().join("secret.md"));
    cx.update(|_, cx| gasp_desktop::encryption::lock_vault(&note, cx));
    cx.run_until_parked();
    assert!(is_locked(&workspace, cx));
    assert!(!cx.read(|cx| screen.read(cx).is_unlocking()));
    assert_eq!(unlock(&workspace, cx, PASSWORD), None);
    assert!(!is_locked(&workspace, cx));
}

#[gpui::test]
fn a_password_dialog_works_off_the_main_thread(cx: &mut TestAppContext) {
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
            dialog.fill(&["pw", "pw"], cx);
            dialog.submit(window, cx);
            dialog.submit(window, cx);
        })
    });
    assert!(cx.read(|cx| dialog.read(cx).is_working()));
    assert!(
        !is_encrypted(&read(&vault, "diary.md")),
        "not before the key is made"
    );
    cx.run_until_parked();
    assert!(is_encrypted(&read(&vault, "diary.md")));
    let still_open = cx.read(|cx| workspace.read(cx).active_modal::<PasswordDialog>());
    assert!(still_open.is_none(), "the dialog closed once it was done");
}

#[gpui::test]
fn changing_the_vault_password_runs_off_the_main_thread(cx: &mut TestAppContext) {
    let vault = encrypted_vault();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "secret.md");
    unlock(&workspace, cx, PASSWORD);
    run(&workspace, cx, "encryption.change-password");
    let dialog = cx
        .read(|cx| workspace.read(cx).active_modal::<PasswordDialog>())
        .expect("the password form opened");
    cx.update(|window, cx| {
        dialog.update(cx, |dialog, cx| {
            dialog.fill(&[PASSWORD, "kelp forest", "kelp forest"], cx);
            dialog.submit(window, cx);
        })
    });
    assert!(cx.read(|cx| dialog.read(cx).is_working()));
    cx.run_until_parked();
    let still_open = cx.read(|cx| workspace.read(cx).active_modal::<PasswordDialog>());
    assert!(still_open.is_none(), "the dialog closed once it was done");
    assert!(!is_locked(&workspace, cx), "open notes stay open");
    run(&workspace, cx, "encryption.lock");
    assert!(unlock(&workspace, cx, PASSWORD).is_some());
    assert_eq!(unlock(&workspace, cx, "kelp forest"), None);
    assert_eq!(editor_text(&workspace, cx), SECRET);
}

#[gpui::test]
fn a_note_password_is_made_off_the_main_thread(cx: &mut TestAppContext) {
    let vault = encrypted_vault();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "secret.md");
    unlock(&workspace, cx, PASSWORD);
    run(&workspace, cx, "encryption.note-options");
    let dialog = cx
        .read(|cx| workspace.read(cx).active_modal::<NoteOptionsDialog>())
        .expect("the options dialog opened");
    cx.update(|window, cx| {
        dialog.update(cx, |dialog, cx| {
            dialog.set_source(PasswordSource::Own, cx);
            dialog.fill_password("barnacle", "barnacle", cx);
            dialog.submit(window, cx);
        })
    });
    assert!(cx.read(|cx| dialog.read(cx).is_working()));
    cx.run_until_parked();
    let still_open = cx.read(|cx| workspace.read(cx).active_modal::<NoteOptionsDialog>());
    assert!(still_open.is_none(), "the dialog closed once it was done");
    run(&workspace, cx, "encryption.lock");
    assert!(unlock(&workspace, cx, PASSWORD).is_some());
    assert_eq!(unlock(&workspace, cx, "barnacle"), None);
    assert_eq!(editor_text(&workspace, cx), SECRET);
}

const STAND_IN: &str = "Encrypted 3f9a2c1b.md";

/// A vault with a password and an encrypted note called "Diary" whose
/// file is `STAND_IN`.
fn hidden_name_vault() -> TempDir {
    let vault = tempfile::tempdir().unwrap();
    let mut encryption = Encryption::load_with(vault.path(), KdfParams::QUICK);
    encryption.set_vault_password(PASSWORD).unwrap();
    let options = NoteOptions {
        name: NameVisibility::Hidden,
        ..NoteOptions::default()
    };
    encryption
        .encrypt("Diary.md", SECRET, options, None)
        .unwrap();
    encryption.rename("Diary.md", STAND_IN);
    let sealed = encryption.seal(STAND_IN, SECRET).unwrap();
    std::fs::write(vault.path().join(STAND_IN), sealed).unwrap();
    vault
}

fn title_field(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.read(|cx| {
        let pane = workspace.read(cx).active_pane().read(cx);
        let note = pane.active_tab().unwrap().note().unwrap();
        note.title.read(cx).text().to_owned()
    })
}

fn tab_title(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.read(|cx| {
        let pane = workspace.read(cx).active_pane().read(cx);
        pane.active_tab().unwrap().title(cx)
    })
}

#[gpui::test]
fn a_hidden_name_shows_as_the_real_name_once_unlocked(cx: &mut TestAppContext) {
    let vault = hidden_name_vault();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, STAND_IN);
    assert_eq!(tab_title(&workspace, cx), "Locked note");
    assert_eq!(title_field(&workspace, cx), "Locked note");
    unlock(&workspace, cx, PASSWORD);
    assert_eq!(tab_title(&workspace, cx), "Diary");
    assert_eq!(title_field(&workspace, cx), "Diary");
    let shown = cx.read(|cx| gasp_desktop::encryption::shown_names(workspace.read(cx).vault(), cx));
    assert_eq!(shown.get(STAND_IN).map(String::as_str), Some("Diary"));
    run(&workspace, cx, "encryption.lock");
    assert_eq!(tab_title(&workspace, cx), "Locked note");
    assert_eq!(title_field(&workspace, cx), "Locked note");
}

#[gpui::test]
fn renaming_a_hidden_name_note_changes_its_sealed_title_not_its_file(cx: &mut TestAppContext) {
    let vault = hidden_name_vault();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, STAND_IN);
    unlock(&workspace, cx, PASSWORD);
    run(&workspace, cx, "note.rename");
    cx.simulate_input("Plans");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(tab_title(&workspace, cx), "Plans");
    assert_eq!(title_field(&workspace, cx), "Plans");
    assert!(vault.path().join(STAND_IN).is_file(), "the stand-in stays");
    assert!(!vault.path().join("Plans.md").exists());
    let file = read(&vault, STAND_IN);
    assert!(is_encrypted(&file) && !file.contains("Plans"));
    let mut fresh = Encryption::load_with(vault.path(), KdfParams::QUICK);
    fresh.unlock_note(STAND_IN, &file, PASSWORD).unwrap();
    assert_eq!(fresh.title(STAND_IN), Some("Plans"));
}

#[gpui::test]
fn a_password_changed_elsewhere_locks_open_notes_at_once(cx: &mut TestAppContext) {
    let vault = encrypted_vault();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "secret.md");
    unlock(&workspace, cx, PASSWORD);
    let mut elsewhere = Encryption::load_with(vault.path(), KdfParams::QUICK);
    elsewhere
        .change_vault_password(PASSWORD, "kelp forest")
        .unwrap();
    let lock = cx.read(|cx| Encryption::lock_path(workspace.read(cx).vault()));
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.apply_disk_changes(vec![DiskChange::Changed(lock)], window, cx)
        })
    });
    cx.run_until_parked();
    assert!(is_locked(&workspace, cx), "asks for the new password");
    let note = cx.read(|cx| workspace.read(cx).vault().join("secret.md"));
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.apply_disk_changes(vec![DiskChange::Changed(note)], window, cx)
        })
    });
    cx.run_until_parked();
    assert!(
        unlock(&workspace, cx, PASSWORD).is_some(),
        "the old password no longer opens it"
    );
    assert_eq!(unlock(&workspace, cx, "kelp forest"), None);
    assert_eq!(editor_text(&workspace, cx), SECRET);
}
