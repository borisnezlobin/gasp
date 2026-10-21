use gasp_crypt::{ClaudeAccess, NameVisibility};

use super::*;

fn vault() -> (tempfile::TempDir, Encryption) {
    let dir = tempfile::tempdir().unwrap();
    let encryption = Encryption::load_with(dir.path(), KdfParams::QUICK);
    (dir, encryption)
}

fn with_password(password: &str) -> (tempfile::TempDir, Encryption) {
    let (dir, mut encryption) = vault();
    encryption.set_vault_password(password).unwrap();
    (dir, encryption)
}

fn write(dir: &tempfile::TempDir, path: &str, text: &str) {
    let absolute = dir.path().join(path);
    std::fs::create_dir_all(absolute.parent().unwrap()).unwrap();
    std::fs::write(absolute, text).unwrap();
}

fn read(dir: &tempfile::TempDir, path: &str) -> String {
    std::fs::read_to_string(dir.path().join(path)).unwrap()
}

#[test]
fn ordinary_notes_pass_through() {
    let (_dir, mut encryption) = vault();
    assert_eq!(
        encryption.read("Plain.md", "# Hi\n"),
        NoteText::Plain("# Hi\n".to_owned())
    );
    assert_eq!(encryption.seal("Plain.md", "# Hi\n"), None);
}

#[test]
fn encrypting_needs_a_vault_password_or_its_own() {
    let (_dir, mut encryption) = vault();
    let refused = encryption.encrypt("A.md", "x", NoteOptions::default(), None);
    assert_eq!(refused, Err(EncryptionError::NoVaultPassword));
    let own = encryption.encrypt("A.md", "x", NoteOptions::default(), Some("mine"));
    assert!(own.is_ok());
}

#[test]
fn the_vault_password_is_kept_as_a_lock_file_only() {
    let (dir, encryption) = with_password("seagrass");
    let lock = read(&dir, ".gasp/encryption.toml");
    assert!(!lock.contains("seagrass"));
    assert!(encryption.is_vault_unlocked());
    let again = Encryption::load_with(dir.path(), KdfParams::QUICK);
    assert!(again.has_vault_password());
    assert!(!again.is_vault_unlocked());
}

#[test]
fn a_note_reopens_after_locking_with_the_vault_password() {
    let (dir, mut encryption) = with_password("seagrass");
    let file = encryption
        .encrypt("Diary.md", "dear diary", NoteOptions::default(), None)
        .unwrap();
    write(&dir, "Diary.md", &file);
    encryption.lock_all();
    assert!(matches!(
        encryption.read("Diary.md", &file),
        NoteText::Locked(_)
    ));
    assert_eq!(
        encryption.unlock_note("Diary.md", &file, "wrong"),
        Err(EncryptionError::Crypt(CryptError::WrongPassword))
    );
    let plain = encryption
        .unlock_note("Diary.md", &file, "seagrass")
        .unwrap();
    assert_eq!(plain.body, "dear diary");
    assert!(encryption.is_vault_unlocked());
}

#[test]
fn an_unlocked_vault_opens_its_notes_without_asking() {
    let (_dir, mut encryption) = with_password("seagrass");
    let file = encryption
        .encrypt("A.md", "a", NoteOptions::default(), None)
        .unwrap();
    encryption.decrypt("A.md");
    let NoteText::Unlocked(plain) = encryption.read("A.md", &file) else {
        panic!("the vault key opens it");
    };
    assert_eq!(plain.body, "a");
}

#[test]
fn saving_an_unlocked_note_seals_the_new_text() {
    let (_dir, mut encryption) = with_password("seagrass");
    let file = encryption
        .encrypt("A.md", "first", NoteOptions::default(), None)
        .unwrap();
    encryption.read("A.md", &file);
    let saved = encryption.seal("A.md", "second").unwrap();
    assert!(!saved.contains("second"));
    encryption.lock_all();
    let plain = encryption.unlock_note("A.md", &saved, "seagrass").unwrap();
    assert_eq!(plain.body, "second");
}

#[test]
fn a_note_password_stays_separate_from_the_vault() {
    let (_dir, mut encryption) = with_password("vault");
    let file = encryption
        .encrypt("Own.md", "own", NoteOptions::default(), Some("note"))
        .unwrap();
    encryption.lock_all();
    encryption.unlock_vault("vault").unwrap();
    assert!(matches!(
        encryption.read("Own.md", &file),
        NoteText::Locked(_)
    ));
    let plain = encryption.unlock_note("Own.md", &file, "note").unwrap();
    assert_eq!(plain.body, "own");
}

#[test]
fn a_hidden_name_comes_back_as_the_title() {
    let (_dir, mut encryption) = with_password("vault");
    let options = NoteOptions {
        claude: ClaudeAccess::Never,
        name: NameVisibility::Hidden,
    };
    let file = encryption
        .encrypt("Secret plans.md", "body", options, None)
        .unwrap();
    assert!(!file.contains("Secret plans"));
    let stand_in = hidden_file_name();
    encryption.rename("Secret plans.md", &stand_in);
    encryption.lock_all();
    encryption.unlock_note(&stand_in, &file, "vault").unwrap();
    assert_eq!(encryption.title(&stand_in), Some("Secret plans"));
}

#[test]
fn changing_the_vault_password_rewraps_every_vault_note() {
    let (dir, mut encryption) = with_password("old");
    for path in ["A.md", "Folder/B.md"] {
        let file = encryption
            .encrypt(path, path, NoteOptions::default(), None)
            .unwrap();
        write(&dir, path, &file);
    }
    let own = encryption
        .encrypt("Own.md", "own", NoteOptions::default(), Some("note"))
        .unwrap();
    write(&dir, "Own.md", &own);
    write(&dir, "Plain.md", "plain");

    assert_eq!(encryption.change_vault_password("old", "new").unwrap(), 2);
    assert_eq!(read(&dir, "Own.md"), own);
    assert_eq!(read(&dir, "Plain.md"), "plain");

    let mut fresh = Encryption::load_with(dir.path(), KdfParams::QUICK);
    assert!(fresh.unlock_vault("old").is_err());
    fresh.unlock_vault("new").unwrap();
    let NoteText::Unlocked(plain) = fresh.read("Folder/B.md", &read(&dir, "Folder/B.md")) else {
        panic!("the new password opens it");
    };
    assert_eq!(plain.body, "Folder/B.md");
}

#[test]
fn a_kept_key_stops_working_once_the_password_changes_elsewhere() {
    let (dir, mut encryption) = with_password("old");
    let kept = encryption.vault_key().unwrap().clone();
    let mut elsewhere = Encryption::load_with(dir.path(), KdfParams::QUICK);
    elsewhere.change_vault_password("old", "new").unwrap();
    encryption.reload_lock();
    assert!(!encryption.is_vault_unlocked());
    assert!(encryption.unlock_vault_with_key(kept).is_err());
}

#[test]
fn options_change_from_the_next_save() {
    let (_dir, mut encryption) = with_password("vault");
    encryption
        .encrypt("A.md", "a", NoteOptions::default(), None)
        .unwrap();
    let options = NoteOptions {
        claude: ClaudeAccess::Always,
        name: NameVisibility::Shown,
    };
    encryption.set_options("A.md", options).unwrap();
    let saved = encryption.seal("A.md", "a").unwrap();
    assert_eq!(gasp_crypt::peek(&saved).unwrap().options, options);
}

#[test]
fn renaming_a_shown_note_changes_the_title_it_keeps() {
    let (_dir, mut encryption) = with_password("vault");
    encryption
        .encrypt("Draft.md", "body", NoteOptions::default(), None)
        .unwrap();
    encryption.rename("Draft.md", "Folder/Final plan.md");
    assert_eq!(encryption.title("Folder/Final plan.md"), Some("Final plan"));
}
