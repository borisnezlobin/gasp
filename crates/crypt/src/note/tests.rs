use super::*;

fn plain(body: &str) -> Plain {
    Plain {
        body: body.to_owned(),
        title: None,
    }
}

fn sealed_with_vault(body: &str, options: NoteOptions) -> (String, Key) {
    let vault = Key::random();
    let text = seal(
        &plain(body),
        options,
        KeySource::Vault,
        &vault,
        &Key::random(),
    );
    (text, vault)
}

#[test]
fn a_note_opens_to_what_was_sealed() {
    let (text, vault) = sealed_with_vault(
        "# Tides\n\nThey turn twice a day.\n",
        NoteOptions::default(),
    );
    assert!(is_encrypted(&text));
    assert!(!text.contains("Tides"));
    let opened = open(&text, &vault).unwrap();
    assert_eq!(opened.plain.body, "# Tides\n\nThey turn twice a day.\n");
    assert_eq!(opened.plain.title, None);
}

#[test]
fn an_empty_note_round_trips() {
    let (text, vault) = sealed_with_vault("", NoteOptions::default());
    assert_eq!(open(&text, &vault).unwrap().plain.body, "");
}

#[test]
fn a_body_that_starts_like_a_title_line_stays_body() {
    let (text, vault) = sealed_with_vault("title: not a title\nmore", NoteOptions::default());
    let opened = open(&text, &vault).unwrap();
    assert_eq!(opened.plain.body, "title: not a title\nmore");
    assert_eq!(opened.plain.title, None);
}

#[test]
fn a_hidden_name_keeps_its_title_inside() {
    let options = NoteOptions {
        name: NameVisibility::Hidden,
        ..NoteOptions::default()
    };
    let vault = Key::random();
    let secret = Plain {
        body: "body".to_owned(),
        title: Some("Diary\nentry".to_owned()),
    };
    let text = seal(&secret, options, KeySource::Vault, &vault, &Key::random());
    assert!(!text.contains("Diary"));
    let opened = open(&text, &vault).unwrap();
    assert_eq!(opened.plain.title.as_deref(), Some("Diary entry"));
    assert_eq!(opened.plain.body, "body");
}

#[test]
fn the_wrong_key_is_a_wrong_password() {
    let (text, _) = sealed_with_vault("x", NoteOptions::default());
    assert_eq!(
        open(&text, &Key::random()).unwrap_err(),
        CryptError::WrongPassword
    );
}

#[test]
fn a_note_password_opens_its_note() {
    let (source, key) = KeySource::new_note_password("orca", KdfParams::QUICK).unwrap();
    let text = seal(
        &plain("pod"),
        NoteOptions::default(),
        source,
        &key,
        &Key::random(),
    );
    assert!(text.contains("key: note"));
    let header = peek(&text).unwrap();
    let again = header.source.note_password_key("orca").unwrap().unwrap();
    assert_eq!(open(&text, &again).unwrap().plain.body, "pod");
    let wrong = header.source.note_password_key("Orca").unwrap().unwrap();
    assert_eq!(open(&text, &wrong).unwrap_err(), CryptError::WrongPassword);
}

#[test]
fn the_header_is_readable_without_a_key() {
    let options = NoteOptions {
        claude: ClaudeAccess::WhileUnlocked,
        name: NameVisibility::Shown,
    };
    let (text, _) = sealed_with_vault("x", options);
    let header = peek(&text).unwrap();
    assert_eq!(header.options, options);
    assert_eq!(header.source, KeySource::Vault);
}

#[test]
fn editing_who_may_read_it_breaks_the_seal() {
    let (text, vault) = sealed_with_vault("x", NoteOptions::default());
    let edited = text.replace("claude: never", "claude: always");
    assert_eq!(open(&edited, &vault).unwrap_err(), CryptError::Corrupt);
}

#[test]
fn a_changed_body_breaks_the_seal() {
    let (text, vault) = sealed_with_vault("a longer note body to change", NoteOptions::default());
    let last = text.trim_end().rfind('\n').unwrap() + 3;
    let mut bytes = text.into_bytes();
    bytes[last] = if bytes[last] == b'A' { b'B' } else { b'A' };
    let edited = String::from_utf8(bytes).unwrap();
    assert_eq!(open(&edited, &vault).unwrap_err(), CryptError::Corrupt);
}

#[test]
fn a_new_vault_key_rewraps_without_resealing() {
    let (text, old) = sealed_with_vault("keep me", NoteOptions::default());
    let new = Key::random();
    let rewrapped = rewrap(&text, &old, &new).unwrap();
    let body_of = |text: &str| text.split("\n\n").nth(1).unwrap().to_owned();
    assert_eq!(body_of(&rewrapped), body_of(&text));
    assert_eq!(open(&rewrapped, &new).unwrap().plain.body, "keep me");
    assert_eq!(
        open(&rewrapped, &old).unwrap_err(),
        CryptError::WrongPassword
    );
}

#[test]
fn windows_line_endings_still_open() {
    let (text, vault) = sealed_with_vault("crlf", NoteOptions::default());
    let windows = text.replace('\n', "\r\n");
    assert_eq!(open(&windows, &vault).unwrap().plain.body, "crlf");
}

#[test]
fn a_note_from_a_newer_gasp_says_so() {
    let (text, vault) = sealed_with_vault("x", NoteOptions::default());
    let newer = text.replace("gasp-encrypted: 1", "gasp-encrypted: 2");
    assert_eq!(open(&newer, &vault).unwrap_err(), CryptError::NewerVersion);
}

#[test]
fn ordinary_notes_are_not_encrypted() {
    assert!(!is_encrypted(
        "# A note\n\ngasp-encrypted: is mentioned later\n"
    ));
    assert_eq!(peek("plain").unwrap_err(), CryptError::NotEncrypted);
}

#[test]
fn every_body_line_fits_the_width() {
    let (text, _) = sealed_with_vault(&"long ".repeat(200), NoteOptions::default());
    let body = text.split("\n\n").nth(1).unwrap();
    assert!(body.lines().all(|line| line.len() <= LINE_WIDTH));
}

#[test]
fn plain_text_never_prints() {
    assert_eq!(format!("{:?}", plain("secret")), "Plain(6 bytes)");
}

#[test]
fn a_notes_own_key_opens_and_reseals_it_without_any_password() {
    let vault = Key::random();
    let note_key = Key::random();
    let options = NoteOptions {
        claude: ClaudeAccess::Always,
        name: NameVisibility::Shown,
    };
    let text = seal(
        &plain("first"),
        options,
        KeySource::Vault,
        &vault,
        &note_key,
    );
    assert_eq!(
        open_with_note_key(&text, &note_key).unwrap().plain.body,
        "first"
    );
    let resealed = reseal(&text, &plain("second"), &note_key).unwrap();
    assert_eq!(open(&resealed, &vault).unwrap().plain.body, "second");
    assert_eq!(peek(&resealed).unwrap().options, options);
}

#[test]
fn another_notes_key_is_refused() {
    let (text, _) = sealed_with_vault("x", NoteOptions::default());
    assert_eq!(
        open_with_note_key(&text, &Key::random()).unwrap_err(),
        CryptError::WrongPassword
    );
    assert_eq!(
        reseal(&text, &plain("y"), &Key::random()).unwrap_err(),
        CryptError::WrongPassword
    );
}

#[test]
fn the_claude_key_name_follows_the_notes_key_across_saves() {
    let vault = Key::random();
    let note_key = Key::random();
    let first = seal(
        &plain("a"),
        NoteOptions::default(),
        KeySource::Vault,
        &vault,
        &note_key,
    );
    let second = seal(
        &plain("b"),
        NoteOptions::default(),
        KeySource::Vault,
        &vault,
        &note_key,
    );
    let id = peek(&first).unwrap().claude_key_id();
    assert_eq!(id, peek(&second).unwrap().claude_key_id());
    assert_eq!(id, claude_key_id(&note_key));
    assert_ne!(id, claude_key_id(&Key::random()));
}
