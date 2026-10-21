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
fn search_skips_encrypted_notes() {
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
