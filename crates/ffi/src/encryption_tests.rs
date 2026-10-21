use std::sync::Arc;

use gasp_crypt::KdfParams;
use gasp_vault::encryption::Encryption;

use super::*;
use crate::vault::tests::vault_with;

const SECRET: &str = "the kelp forest map\n";
const PASSWORD: &str = "seagrass";

/// A vault with a password, where keys are quick to make.
fn vault_with_password(notes: &[(&str, &str)]) -> (tempfile::TempDir, Arc<VaultFolder>) {
    let (dir, vault) = vault_with(notes);
    *vault.encryption() = Encryption::load_with(dir.path(), KdfParams::QUICK);
    vault.set_vault_password(PASSWORD.into()).unwrap();
    (dir, vault)
}

fn on_disk(dir: &tempfile::TempDir, path: &str) -> String {
    std::fs::read_to_string(dir.path().join(path)).unwrap()
}

fn hide_new_names(dir: &tempfile::TempDir, vault: &VaultFolder) {
    let config = dir.path().join(gasp_config::CONFIG_DIR);
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("settings.toml"),
        "[encryption]\nhide-names = true\n",
    )
    .unwrap();
    vault.reload_config();
}

#[test]
fn saving_an_unlocked_note_seals_it() {
    let (dir, vault) = vault_with_password(&[("Map.md", "draft")]);
    vault
        .encrypt_note("Map.md".into(), "draft".into(), None)
        .unwrap();
    vault.save_note("Map.md".into(), SECRET.into()).unwrap();
    let file = on_disk(&dir, "Map.md");
    assert!(is_encrypted_text(file.clone()));
    assert!(!file.contains("kelp"));
    assert_eq!(
        vault.open_note("Map.md".into()).unwrap(),
        OpenedNote::Unlocked {
            text: SECRET.into()
        }
    );
}

#[test]
fn a_locked_note_opens_locked_and_refuses_plain_saves() {
    let (dir, vault) = vault_with_password(&[("Map.md", SECRET)]);
    vault
        .encrypt_note("Map.md".into(), SECRET.into(), None)
        .unwrap();
    vault.lock_encrypted_notes();
    let OpenedNote::Locked { note } = vault.open_note("Map.md".into()).unwrap() else {
        panic!("the note should be locked");
    };
    assert!(!note.has_own_password);
    assert!(note.key_id.is_some_and(|id| id.starts_with("vault-")));
    let sealed = on_disk(&dir, "Map.md");
    assert!(matches!(
        vault.save_note("Map.md".into(), "late save".into()),
        Err(VaultError::Refused { .. })
    ));
    assert_eq!(on_disk(&dir, "Map.md"), sealed);
}

#[test]
fn a_wrong_password_is_told_apart() {
    let (_dir, vault) = vault_with_password(&[("Map.md", SECRET)]);
    vault
        .encrypt_note("Map.md".into(), SECRET.into(), None)
        .unwrap();
    vault.lock_encrypted_notes();
    assert!(matches!(
        vault.unlock_note("Map.md".into(), "kelp".into()),
        Err(VaultError::WrongPassword { .. })
    ));
    assert_eq!(
        vault.unlock_note("Map.md".into(), PASSWORD.into()).unwrap(),
        SECRET
    );
    assert!(vault.encryption_state().vault_unlocked);
}

#[test]
fn a_kept_key_unlocks_the_vault_and_a_stale_one_is_refused() {
    let (_dir, vault) = vault_with_password(&[("Map.md", SECRET)]);
    vault
        .encrypt_note("Map.md".into(), SECRET.into(), None)
        .unwrap();
    let kept = vault.note_key("Map.md".into()).unwrap();
    assert_eq!(Some(kept.clone()), vault.vault_key());
    vault.lock_encrypted_notes();
    assert!(matches!(
        vault.unlock_note_with_key("Map.md".into(), vec![7; 32]),
        Err(VaultError::WrongPassword { .. })
    ));
    let text = vault
        .unlock_note_with_key("Map.md".into(), kept.bytes)
        .unwrap();
    assert_eq!(text, SECRET);
    assert!(vault.encryption_state().vault_unlocked);
}

#[test]
fn encrypted_notes_keep_no_snapshots() {
    let (_dir, vault) = vault_with_password(&[("Map.md", SECRET)]);
    assert!(vault.record_snapshot("Map.md".into(), SECRET.into(), true));
    vault
        .encrypt_note("Map.md".into(), SECRET.into(), None)
        .unwrap();
    assert!(vault.snapshots("Map.md".into()).is_empty());
    assert!(!vault.record_snapshot("Map.md".into(), SECRET.into(), true));
}

#[test]
fn hidden_names_show_the_title_only_while_unlocked() {
    let (dir, vault) = vault_with_password(&[("Taxes.md", SECRET)]);
    hide_new_names(&dir, &vault);
    let path = vault
        .encrypt_note("Taxes.md".into(), SECRET.into(), None)
        .unwrap();
    assert!(path.starts_with("Encrypted "));
    let titles = |vault: &VaultFolder| -> Vec<(String, bool)> {
        vault
            .notes()
            .into_iter()
            .map(|note| (note.title, note.encrypted))
            .collect()
    };
    assert_eq!(titles(&vault), [("Taxes".to_owned(), true)]);
    vault.lock_encrypted_notes();
    assert_eq!(titles(&vault), [("Locked note".to_owned(), true)]);
    vault.unlock_note(path.clone(), PASSWORD.into()).unwrap();
    let back = vault.decrypt_note(path, SECRET.into()).unwrap();
    assert_eq!(back, "Taxes.md");
    assert_eq!(on_disk(&dir, "Taxes.md"), SECRET);
}

#[test]
fn showing_and_hiding_a_name_renames_the_file() {
    let (_dir, vault) = vault_with_password(&[("Folder/Taxes.md", SECRET)]);
    let path = vault
        .encrypt_note("Folder/Taxes.md".into(), SECRET.into(), None)
        .unwrap();
    let hidden = vault
        .set_encrypted_note_options(path, SECRET.into(), NoteClaudeAccess::Always, true)
        .unwrap();
    assert!(hidden.starts_with("Folder/Encrypted "));
    let options = vault.encrypted_note_options(hidden.clone()).unwrap();
    assert_eq!(options.claude, NoteClaudeAccess::Always);
    assert_eq!(options.title, "Taxes");
    let shown = vault
        .set_encrypted_note_options(hidden, SECRET.into(), NoteClaudeAccess::Never, false)
        .unwrap();
    assert_eq!(shown, "Folder/Taxes.md");
}

#[test]
fn a_note_password_replaces_the_vault_s_and_keeps_the_title() {
    let (dir, vault) = vault_with_password(&[("Taxes.md", SECRET)]);
    hide_new_names(&dir, &vault);
    let path = vault
        .encrypt_note("Taxes.md".into(), SECRET.into(), None)
        .unwrap();
    vault
        .set_note_password(path.clone(), SECRET.into(), Some("own".into()))
        .unwrap();
    assert!(
        vault
            .encrypted_note_options(path.clone())
            .unwrap()
            .has_own_password
    );
    vault.lock_encrypted_notes();
    assert!(vault.unlock_note(path.clone(), PASSWORD.into()).is_err());
    vault.unlock_note(path.clone(), "own".into()).unwrap();
    assert_eq!(vault.encrypted_note_options(path).unwrap().title, "Taxes");
}

#[test]
fn handing_a_note_back_to_a_locked_vault_password_is_refused() {
    let (_dir, vault) = vault_with_password(&[("Map.md", SECRET)]);
    vault
        .encrypt_note("Map.md".into(), SECRET.into(), Some("own".into()))
        .unwrap();
    vault.lock_encrypted_notes();
    vault.unlock_note("Map.md".into(), "own".into()).unwrap();
    assert!(
        vault
            .set_note_password("Map.md".into(), SECRET.into(), None)
            .is_err()
    );
    assert!(vault.is_note_unlocked("Map.md".into()));
}

#[test]
fn moving_a_folder_keeps_its_notes_unlocked() {
    let (_dir, vault) = vault_with_password(&[("Old/Map.md", SECRET)]);
    vault
        .encrypt_note("Old/Map.md".into(), SECRET.into(), None)
        .unwrap();
    vault.rename_folder("Old".into(), "New".into()).unwrap();
    assert!(vault.is_note_unlocked("New/Map.md".into()));
    vault
        .save_note("New/Map.md".into(), "moved".into())
        .unwrap();
    let renamed = vault
        .rename_note("New/Map.md".into(), "Chart".into())
        .unwrap();
    assert!(vault.is_note_unlocked(renamed));
}

#[test]
fn search_finds_encrypted_notes_by_name_only() {
    let (_dir, vault) = vault_with_password(&[("Map.md", SECRET), ("Open.md", "kelp in the open")]);
    vault
        .encrypt_note("Map.md".into(), SECRET.into(), None)
        .unwrap();
    let found: Vec<String> = vault
        .search("kelp".into())
        .into_iter()
        .map(|result| result.note.path)
        .collect();
    assert_eq!(found, ["Open.md"]);
    let by_name: Vec<String> = vault
        .search("map".into())
        .into_iter()
        .map(|result| result.note.path)
        .collect();
    assert_eq!(by_name, ["Map.md"]);
}

#[test]
fn changing_the_vault_password_counts_its_notes() {
    let (_dir, vault) = vault_with_password(&[("A.md", "a"), ("B.md", "b")]);
    for path in ["A.md", "B.md"] {
        vault.encrypt_note(path.into(), "x".into(), None).unwrap();
    }
    assert_eq!(
        vault
            .change_vault_password(PASSWORD.into(), "new".into())
            .unwrap(),
        2
    );
    vault.lock_encrypted_notes();
    assert!(vault.unlock_note("A.md".into(), "new".into()).is_ok());
}

#[test]
fn an_encrypted_template_is_refused_rather_than_pasted() {
    let (_dir, vault) = vault_with_password(&[("Templates/Secret.md", SECRET), ("Note.md", "")]);
    vault
        .encrypt_note("Templates/Secret.md".into(), SECRET.into(), None)
        .unwrap();
    assert!(
        vault
            .template_text("Secret".into(), "Note.md".into())
            .is_err()
    );
}

/// A vault whose keys take as long to make as the app's do.
fn vault_with_slow_keys(notes: &[(&str, &str)]) -> (tempfile::TempDir, Arc<VaultFolder>) {
    let (dir, vault) = vault_with(notes);
    *vault.encryption() = Encryption::load_with(dir.path(), KdfParams::STANDARD);
    vault.set_vault_password(PASSWORD.into()).unwrap();
    (dir, vault)
}

#[test]
fn saves_and_opens_carry_on_while_a_password_is_turned_into_a_key() {
    let (dir, vault) = vault_with_slow_keys(&[("Map.md", SECRET), ("Open.md", "plain")]);
    vault
        .encrypt_note("Map.md".into(), SECRET.into(), None)
        .unwrap();
    vault.lock_encrypted_notes();
    let unlocked = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (started, has_started) = std::sync::mpsc::channel();
    let unlocking = {
        let (vault, unlocked) = (Arc::clone(&vault), Arc::clone(&unlocked));
        std::thread::spawn(move || {
            started.send(()).unwrap();
            vault.unlock_note("Map.md".into(), PASSWORD.into()).unwrap();
            unlocked.store(true, std::sync::atomic::Ordering::SeqCst);
        })
    };
    has_started.recv().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));
    vault.save_note("Open.md".into(), "saved".into()).unwrap();
    assert!(matches!(
        vault.open_note("Map.md".into()).unwrap(),
        OpenedNote::Locked { .. }
    ));
    assert!(!unlocked.load(std::sync::atomic::Ordering::SeqCst));
    unlocking.join().unwrap();
    assert_eq!(on_disk(&dir, "Open.md"), "saved");
    assert!(vault.is_note_unlocked("Map.md".into()));
}

#[test]
fn unlocking_and_saving_on_many_threads_never_deadlocks() {
    let notes: Vec<(String, String)> = (0..4)
        .map(|at| (format!("Note {at}.md"), SECRET.to_owned()))
        .collect();
    let borrowed: Vec<(&str, &str)> = notes
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let (_dir, vault) = vault_with_password(&borrowed);
    for (path, _) in &notes {
        vault
            .encrypt_note(path.clone(), SECRET.into(), None)
            .unwrap();
    }
    vault.lock_encrypted_notes();
    let workers: Vec<_> = notes
        .iter()
        .map(|(path, _)| {
            let (vault, path) = (Arc::clone(&vault), path.clone());
            std::thread::spawn(move || {
                for _ in 0..10 {
                    vault.unlock_note(path.clone(), PASSWORD.into()).unwrap();
                    vault.save_note(path.clone(), SECRET.into()).unwrap();
                    vault.open_note(path.clone()).unwrap();
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert!(vault.is_note_unlocked("Note 0.md".into()));
}

#[test]
fn a_save_while_the_vault_password_changes_is_sealed_with_the_new_key() {
    let (dir, vault) = vault_with_password(&[("Map.md", SECRET)]);
    vault
        .encrypt_note("Map.md".into(), SECRET.into(), None)
        .unwrap();
    let old_sealing = on_disk(&dir, "Map.md");
    let job = vault
        .encryption()
        .password_change_job(PASSWORD, "new")
        .unwrap();
    let changed = job.run().unwrap();
    std::fs::write(dir.path().join("Map.md"), &old_sealing).unwrap();
    {
        let mut encryption = vault.encryption();
        for path in encryption.finish_password_change(changed) {
            vault.reseal_with_new_key(&encryption, &path).unwrap();
        }
    }
    vault.lock_encrypted_notes();
    assert_eq!(
        vault.unlock_note("Map.md".into(), "new".into()).unwrap(),
        SECRET
    );
}

#[test]
fn renaming_a_hidden_name_note_changes_its_sealed_title_only() {
    let (dir, vault) = vault_with_password(&[("Taxes.md", SECRET)]);
    hide_new_names(&dir, &vault);
    let path = vault
        .encrypt_note("Taxes.md".into(), SECRET.into(), None)
        .unwrap();
    let renamed = vault
        .rename_note(path.clone(), "Taxes 2026".into())
        .unwrap();
    assert_eq!(renamed, path);
    vault.lock_encrypted_notes();
    assert!(matches!(
        vault.rename_note(path.clone(), "Peek".into()),
        Err(VaultError::Refused { .. })
    ));
    vault.unlock_note(path.clone(), PASSWORD.into()).unwrap();
    assert_eq!(
        vault.encrypted_note_options(path.clone()).unwrap().title,
        "Taxes 2026"
    );
    assert_eq!(vault.notes()[0].title, "Taxes 2026");
    assert_eq!(
        vault.open_note(path).unwrap(),
        OpenedNote::Unlocked {
            text: SECRET.into()
        }
    );
}
