use gasp_crypt::{KdfParams, NameVisibility};
use gasp_vault::encryption::{Encryption, NoteText};
use serde_json::json;

use super::*;
use crate::claude_keys::MemoryKeys;
use crate::tools::testing::{call, call_err, run, text, vault};

const BODY: &str = "# Diary\nSecret plans.\n";

fn options(claude: ClaudeAccess) -> NoteOptions {
    NoteOptions {
        claude,
        name: NameVisibility::Shown,
    }
}

/// A vault with `Diary.md` sealed by the vault password, Claude's access
/// to it set to `claude`, and its own key kept for Claude when `kept`.
fn sealed_vault(claude: ClaudeAccess, kept: bool) -> (tempfile::TempDir, Context) {
    let (dir, context) = vault(&[("Plain.md", "plain words\n")]);
    let mut encryption = Encryption::load_with(dir.path(), KdfParams::QUICK);
    encryption.set_vault_password("seagrass").unwrap();
    let file = encryption
        .encrypt("Diary.md", BODY, options(claude), None)
        .unwrap();
    std::fs::write(dir.path().join("Diary.md"), file).unwrap();
    let keys = MemoryKeys::default();
    if kept {
        let (id, key) = encryption.claude_key("Diary.md").unwrap();
        keys.insert(&id, key.clone());
    }
    (dir, context.with_claude_keys(keys))
}

fn diary_file(dir: &tempfile::TempDir) -> String {
    std::fs::read_to_string(dir.path().join("Diary.md")).unwrap()
}

/// The diary's text, opened with the vault password.
fn opened_diary(dir: &tempfile::TempDir) -> String {
    let mut encryption = Encryption::load_with(dir.path(), KdfParams::QUICK);
    encryption.unlock_vault("seagrass").unwrap();
    match encryption.read("Diary.md", &diary_file(dir)) {
        NoteText::Unlocked(plain) => plain.body.clone(),
        other => panic!("the diary didn't open: {other:?}"),
    }
}

#[test]
fn a_note_claude_may_never_read_stays_shut() {
    let (dir, context) = sealed_vault(ClaudeAccess::Never, true);
    let before = diary_file(&dir);
    let refused = "Diary.md is encrypted and its owner hasn't let Claude read it.";
    assert_eq!(
        call_err(&context, "read_note", json!({"path": "Diary"})),
        refused
    );
    assert_eq!(
        call_err(&context, "render_note", json!({"path": "Diary"})),
        refused
    );
    let write = json!({"path": "Diary", "text": "plain now"});
    assert_eq!(call_err(&context, "write_note", write), refused);
    let patch = json!({"path": "Diary", "operation": "append", "content": "x"});
    assert_eq!(call_err(&context, "patch_note", patch), refused);
    assert_eq!(diary_file(&dir), before);
}

#[test]
fn encrypted_notes_are_marked_and_found_by_name_only() {
    let (_dir, context) = sealed_vault(ClaudeAccess::Never, false);
    let listing = call(&context, "list_notes", json!({}));
    assert_eq!(listing["files"][0]["path"], "Diary.md");
    assert_eq!(listing["files"][0]["encrypted"], true);
    assert_eq!(listing["files"][1].get("encrypted"), None);
    let found = call(&context, "search", json!({"query": "diary"}));
    assert_eq!(found["results"][0]["path"], "Diary.md");
    assert_eq!(found["results"][0]["encrypted"], true);
    assert_eq!(found["results"][0]["lines"], json!([]));
    let found = call(&context, "search", json!({"query": "gasp-encrypted"}));
    assert_eq!(found["total_notes"], 0);
}

#[test]
fn a_note_claude_may_always_read_opens_with_the_kept_key() {
    let (dir, context) = sealed_vault(ClaudeAccess::Always, true);
    let read = call(&context, "read_note", json!({"path": "Diary"}));
    assert_eq!(read["text"], BODY);
    assert_eq!(read["source"], "disk");
    assert_eq!(read["encrypted"], true);

    let patch = json!({"path": "Diary", "operation": "append", "content": "More plans."});
    let said = text(&context, "patch_note", patch);
    assert!(said.contains("to the file"), "{said}");
    assert!(is_encrypted(&diary_file(&dir)));
    assert!(!diary_file(&dir).contains("plans"));
    assert_eq!(opened_diary(&dir), format!("{BODY}More plans.\n"));

    text(
        &context,
        "write_note",
        json!({"path": "Diary", "text": "All new.\n"}),
    );
    assert!(is_encrypted(&diary_file(&dir)));
    assert_eq!(opened_diary(&dir), "All new.\n");
}

#[test]
fn a_note_password_key_opens_its_note() {
    let (dir, context) = vault(&[]);
    let mut encryption = Encryption::load_with(dir.path(), KdfParams::QUICK);
    let file = encryption
        .encrypt(
            "Own.md",
            "mine\n",
            options(ClaudeAccess::Always),
            Some("pw"),
        )
        .unwrap();
    std::fs::write(dir.path().join("Own.md"), file).unwrap();
    let (id, key) = encryption.claude_key("Own.md").unwrap();
    assert!(id.starts_with("claude-"));
    let keys = MemoryKeys::default();
    keys.insert(&id, key.clone());
    let context = context.with_claude_keys(keys);
    let read = call(&context, "read_note", json!({"path": "Own"}));
    assert_eq!(read["text"], "mine\n");
}

#[test]
fn one_notes_kept_key_opens_no_other_note() {
    let (dir, context) = sealed_vault(ClaudeAccess::Always, true);
    let mut encryption = Encryption::load_with(dir.path(), KdfParams::QUICK);
    encryption.unlock_vault("seagrass").unwrap();
    let other = encryption
        .encrypt("Other.md", "other\n", options(ClaudeAccess::Always), None)
        .unwrap();
    std::fs::write(dir.path().join("Other.md"), other).unwrap();
    let error = call_err(&context, "read_note", json!({"path": "Other"}));
    assert!(error.contains("isn't in this device's keychain"), "{error}");
}

#[test]
fn always_without_a_kept_key_says_so() {
    let (_dir, context) = sealed_vault(ClaudeAccess::Always, false);
    let error = call_err(&context, "read_note", json!({"path": "Diary"}));
    assert!(error.contains("isn't in this device's keychain"), "{error}");
}

#[test]
fn while_unlocked_needs_the_app() {
    let (dir, context) = sealed_vault(ClaudeAccess::WhileUnlocked, true);
    let before = diary_file(&dir);
    let error = call_err(&context, "read_note", json!({"path": "Diary"}));
    assert!(
        error.contains("only while it's unlocked in the app"),
        "{error}"
    );
    let write = json!({"path": "Diary", "text": "plain now"});
    assert!(call_err(&context, "write_note", write).contains("unlocked in the app"));
    assert_eq!(diary_file(&dir), before);
}

#[test]
fn an_edited_header_doesnt_let_claude_in() {
    let (dir, context) = sealed_vault(ClaudeAccess::Never, true);
    let edited = diary_file(&dir).replace("claude: never", "claude: always");
    std::fs::write(dir.path().join("Diary.md"), &edited).unwrap();
    let error = call_err(&context, "read_note", json!({"path": "Diary"}));
    assert!(error.contains("doesn't open it"), "{error}");
    let write = json!({"path": "Diary", "text": "plain now"});
    assert!(run(&context, "write_note", write).is_err());
    assert_eq!(diary_file(&dir), edited);
}

#[test]
fn the_app_shares_only_notes_claude_may_read() {
    assert!(!app_may_share(options(ClaudeAccess::Never)));
    assert!(app_may_share(options(ClaudeAccess::WhileUnlocked)));
    assert!(app_may_share(options(ClaudeAccess::Always)));
}
