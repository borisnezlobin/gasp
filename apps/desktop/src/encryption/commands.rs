//! The encryption commands: encrypting the active note, taking its
//! encryption off, changing how it's encrypted, locking every note and
//! changing the vault password.

use std::path::{Path, PathBuf};

use gasp_config::settings::{EncryptedClaudeAccess, EncryptionSettings};
use gasp_crypt::{ClaudeAccess, KeySource, NameVisibility, NoteOptions, peek};
use gasp_vault::encryption::{Encryption, EncryptionError, hidden_file_name};
use gpui::{App, Context, Entity, PromptLevel, WeakEntity, Window};

use super::dialogs::{
    NoteOptionsDialog, OptionsChoice, PasswordDialog, PasswordForm, PasswordSource, PasswordSubmit,
    matching,
};
use crate::workspace::Workspace;
use crate::workspace::note_doc::NoteDoc;

/// Wires the encryption commands into `workspace`, and starts keeping its
/// vault's encryption.
pub fn install(workspace: &mut Workspace, cx: &mut Context<Workspace>) {
    let settings = workspace.config().settings.encryption.clone();
    super::register_vault(workspace.vault(), &settings, cx);
    workspace.on_command("encryption.encrypt-note", encrypt_note);
    workspace.on_command("encryption.decrypt-note", decrypt_note);
    workspace.on_command("encryption.lock", |workspace, _, cx| {
        super::lock_vault(workspace.vault(), cx);
    });
    workspace.on_command("encryption.note-options", open_note_options);
    workspace.on_command("encryption.change-password", change_password);
}

fn active_doc(workspace: &Workspace, cx: &App) -> Option<Entity<NoteDoc>> {
    let pane = workspace.active_pane().read(cx);
    Some(pane.active_tab()?.note()?.doc.clone())
}

fn notice(message: &str, cx: &mut App) {
    crate::notices::show(crate::notices::Notice::offer(message.to_owned()), cx);
}

/// Runs `action` on the vault's encryption.
fn vault_encryption<R>(
    vault: &Path,
    cx: &mut App,
    action: impl FnOnce(&mut Encryption) -> R,
) -> Option<R> {
    super::with_encryption(vault, cx, |encryption, _| action(encryption))
}

fn vault_state(vault: &Path, cx: &App) -> (bool, bool) {
    super::read_encryption(vault, cx, |encryption, _| {
        (encryption.has_vault_password(), encryption.is_vault_unlocked())
    })
    .unwrap_or_default()
}

/// The options a newly encrypted note starts with.
fn default_options(settings: &EncryptionSettings) -> NoteOptions {
    let claude = match settings.claude_access {
        EncryptedClaudeAccess::Never => ClaudeAccess::Never,
        EncryptedClaudeAccess::WhileUnlocked => ClaudeAccess::WhileUnlocked,
        EncryptedClaudeAccess::Always => ClaudeAccess::Always,
    };
    let name = match settings.hide_names {
        true => NameVisibility::Hidden,
        false => NameVisibility::Shown,
    };
    NoteOptions { claude, name }
}

fn crypt_message(error: EncryptionError) -> String {
    error.to_string()
}

/// `encryption.encrypt-note`: encrypts the active note with the vault
/// password, asking for one first when the vault has none or it's locked.
fn encrypt_note(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    let Some(doc) = active_doc(workspace, cx) else {
        return;
    };
    if doc.read(cx).is_encrypted() {
        notice("This note is already encrypted.", cx);
        return;
    }
    let vault = workspace.vault().to_path_buf();
    let this = cx.entity().downgrade();
    match vault_state(&vault, cx) {
        (false, _) => ask_for_new_vault_password(workspace, vault, this, doc, window, cx),
        (true, false) => ask_for_vault_password(workspace, vault, this, doc, window, cx),
        // Renaming takes the workspace, which is busy running this.
        (true, true) => cx.defer(move |cx| {
            if let Err(error) = finish_encrypting(&this, &doc, cx) {
                crate::notices::problem(error, cx);
            }
        }),
    }
}

fn ask_for_new_vault_password(
    workspace: &mut Workspace,
    vault: PathBuf,
    this: WeakEntity<Workspace>,
    doc: Entity<NoteDoc>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let on_submit: PasswordSubmit = Box::new(move |values, _, cx| {
        matching(&values[0], &values[1])?;
        vault_encryption(&vault, cx, |encryption| encryption.set_vault_password(&values[0]))
            .unwrap_or(Err(EncryptionError::NoVaultPassword))
            .map_err(crypt_message)?;
        super::lock_screen::keep_vault_key_for_touch_id(&vault, cx);
        finish_encrypting(&this, &doc, cx)
    });
    let form = PasswordForm {
        title: "Choose a vault password",
        detail: Some(
            "It unlocks every encrypted note in this vault. Gasp can’t recover it if it’s forgotten, so keep it somewhere safe.".to_owned(),
        ),
        fields: vec!["Vault password", "Type it again"],
        submit_label: "Encrypt note",
        on_submit,
    };
    open_password_dialog(workspace, form, window, cx);
}

fn ask_for_vault_password(
    workspace: &mut Workspace,
    vault: PathBuf,
    this: WeakEntity<Workspace>,
    doc: Entity<NoteDoc>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let on_submit: PasswordSubmit = Box::new(move |values, _, cx| {
        super::reload_lock(&vault, cx);
        vault_encryption(&vault, cx, |encryption| encryption.unlock_vault(&values[0]))
            .unwrap_or(Err(EncryptionError::NoVaultPassword))
            .map_err(crypt_message)?;
        super::lock_screen::keep_vault_key_for_touch_id(&vault, cx);
        super::reopen_locked_notes(&vault, cx);
        finish_encrypting(&this, &doc, cx)
    });
    let form = PasswordForm {
        title: "Type the vault password",
        detail: Some("The note is encrypted with the password the vault already has.".to_owned()),
        fields: vec!["Vault password"],
        submit_label: "Encrypt note",
        on_submit,
    };
    open_password_dialog(workspace, form, window, cx);
}

fn open_password_dialog(
    workspace: &mut Workspace,
    form: PasswordForm,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    workspace.toggle_modal(window, cx, move |window, cx| {
        PasswordDialog::new(form, window, cx)
    });
}

/// Encrypts `doc` with the vault's key and the settings' defaults, then
/// gives it a stand-in name when those say to hide names.
fn finish_encrypting(
    this: &WeakEntity<Workspace>,
    doc: &Entity<NoteDoc>,
    cx: &mut App,
) -> Result<(), String> {
    let path = doc.read(cx).path().to_path_buf();
    let options = default_options(&super::settings_for(&path, cx));
    doc.update(cx, |doc, cx| doc.encrypt(options, None, cx))?;
    if options.name == NameVisibility::Hidden {
        rename_doc(this, doc, &hidden_title(), cx)?;
    }
    notice("Encrypted. The note locks with the vault.", cx);
    Ok(())
}

/// A stand-in title, the hidden file name without `.md`.
fn hidden_title() -> String {
    let name = hidden_file_name();
    name.strip_suffix(".md").unwrap_or(&name).to_owned()
}

fn rename_doc(
    this: &WeakEntity<Workspace>,
    doc: &Entity<NoteDoc>,
    title: &str,
    cx: &mut App,
) -> Result<(), String> {
    this.update(cx, |workspace, cx| workspace.rename_note(doc, title, cx))
        .map_err(|_| "The window closed.".to_owned())?
}

/// `encryption.decrypt-note`: asks, then saves the active note as plain
/// text again, under its own name.
fn decrypt_note(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    let Some(doc) = active_doc(workspace, cx) else {
        return;
    };
    let unlocked = doc.read(cx).is_unlocked_encrypted();
    if !doc.read(cx).is_encrypted() {
        notice("This note isn’t encrypted.", cx);
        return;
    }
    if !unlocked {
        notice("Unlock the note first, then remove its encryption.", cx);
        return;
    }
    let message = "Remove encryption from this note?";
    let detail = "Its text is saved as plain text again, which anyone who can open the file can read.";
    let answer = window.prompt(
        PromptLevel::Warning,
        message,
        Some(detail),
        &["Remove encryption", "Cancel"],
        cx,
    );
    cx.spawn_in(window, async move |workspace, cx| {
        if answer.await != Ok(0) {
            return;
        }
        cx.update(|_, cx| {
            if let Err(error) = remove_encryption(&workspace, &doc, cx) {
                crate::notices::problem(error, cx);
            }
        })
        .ok();
    })
    .detach();
}

fn remove_encryption(
    this: &WeakEntity<Workspace>,
    doc: &Entity<NoteDoc>,
    cx: &mut App,
) -> Result<(), String> {
    let path = doc.read(cx).path().to_path_buf();
    let real_title = hidden_name_title(&path, cx);
    doc.update(cx, |doc, cx| doc.decrypt(cx))
        .map_err(|error| error.to_string())?;
    if let Some(title) = real_title {
        rename_doc(this, doc, &title, cx)?;
    }
    notice("The note is plain text again.", cx);
    Ok(())
}

/// The real title of the unlocked note at `path` when its file name is
/// hidden.
fn hidden_name_title(path: &Path, cx: &App) -> Option<String> {
    super::read_encryption(path, cx, |encryption, relative| {
        let options = encryption.options(relative)?;
        (options.name == NameVisibility::Hidden)
            .then(|| encryption.title(relative).map(str::to_owned))
            .flatten()
    })
    .flatten()
}

/// `encryption.note-options`: the active note's password, name and
/// Claude access.
fn open_note_options(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    let Some(doc) = active_doc(workspace, cx) else {
        return;
    };
    if !doc.read(cx).is_unlocked_encrypted() {
        let message = match doc.read(cx).is_locked() {
            true => "Unlock the note first, then change how it’s encrypted.",
            false => "This note isn’t encrypted. Encrypt it first.",
        };
        notice(message, cx);
        return;
    }
    let path = doc.read(cx).path().to_path_buf();
    let Some(options) = super::read_encryption(&path, cx, |encryption, relative| {
        encryption.options(relative)
    })
    .flatten() else {
        return;
    };
    let own_password = peek(doc.read(cx).disk_text())
        .is_ok_and(|header| matches!(header.source, KeySource::Note { .. }));
    let title = hidden_name_title(&path, cx)
        .unwrap_or_else(|| crate::workspace::files::note_title(&path));
    let this = cx.entity().downgrade();
    let on_submit = Box::new(move |choice: OptionsChoice, _: &mut Window, cx: &mut App| {
        apply_options(&this, &doc, own_password, choice, cx)
    });
    workspace.toggle_modal(window, cx, move |window, cx| {
        NoteOptionsDialog::new(title, options, own_password, on_submit, window, cx)
    });
}

/// Applies the options dialog's choice to the open note.
fn apply_options(
    this: &WeakEntity<Workspace>,
    doc: &Entity<NoteDoc>,
    had_own_password: bool,
    choice: OptionsChoice,
    cx: &mut App,
) -> Result<(), String> {
    let path = doc.read(cx).path().to_path_buf();
    let hidden_before = hidden_name_title(&path, cx);
    let real_title = super::read_encryption(&path, cx, |encryption, relative| {
        encryption.title(relative).map(str::to_owned)
    })
    .flatten();
    let rekey = needs_new_key(had_own_password, &choice);
    match rekey {
        Some(password) => doc.update(cx, |doc, cx| doc.encrypt(choice.options, password, cx))?,
        None => set_options(doc, &path, choice.options, cx)?,
    }
    super::mark(&path, Some(choice.options.name), cx);
    let was_hidden = hidden_before.is_some();
    rename_for_visibility(this, doc, was_hidden, choice.options.name, real_title, cx)
}

/// The note's new key source when it changes: `Some(None)` for the vault
/// password, `Some(Some(password))` for a new password of its own.
fn needs_new_key(had_own_password: bool, choice: &OptionsChoice) -> Option<Option<&str>> {
    match (choice.source, choice.new_password.as_deref()) {
        (PasswordSource::Own, Some(password)) => Some(Some(password)),
        (PasswordSource::Vault, _) if had_own_password => Some(None),
        _ => None,
    }
}

fn set_options(
    doc: &Entity<NoteDoc>,
    path: &Path,
    options: NoteOptions,
    cx: &mut App,
) -> Result<(), String> {
    super::with_encryption(path, cx, |encryption, relative| {
        encryption.set_options(relative, options)
    })
    .unwrap_or(Err(EncryptionError::VaultLocked))
    .map_err(crypt_message)?;
    doc.update(cx, |doc, cx| doc.reseal(cx))
        .map_err(|error| error.to_string())
}

fn rename_for_visibility(
    this: &WeakEntity<Workspace>,
    doc: &Entity<NoteDoc>,
    was_hidden: bool,
    name: NameVisibility,
    real_title: Option<String>,
    cx: &mut App,
) -> Result<(), String> {
    match (was_hidden, name) {
        (false, NameVisibility::Hidden) => rename_doc(this, doc, &hidden_title(), cx),
        (true, NameVisibility::Shown) => match real_title {
            Some(title) => rename_doc(this, doc, &title, cx),
            None => Ok(()),
        },
        _ => Ok(()),
    }
}

/// `encryption.change-password`: the vault password's old and new
/// versions, or a first one when there's none yet.
fn change_password(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    let vault = workspace.vault().to_path_buf();
    let (has_password, _) = vault_state(&vault, cx);
    let form = match has_password {
        true => change_form(vault),
        false => first_password_form(vault),
    };
    open_password_dialog(workspace, form, window, cx);
}

fn change_form(vault: PathBuf) -> PasswordForm {
    let on_submit: PasswordSubmit = Box::new(move |values, _, cx| {
        matching(&values[1], &values[2])?;
        super::reload_lock(&vault, cx);
        let old_id = vault_encryption(&vault, cx, |encryption| encryption.vault_key_id()).flatten();
        let changed = vault_encryption(&vault, cx, |encryption| {
            encryption.change_vault_password(&values[0], &values[1])
        })
        .unwrap_or(Err(EncryptionError::NoVaultPassword))
        .map_err(crypt_message)?;
        if let Some(id) = old_id {
            super::keychain::forget(&id, cx);
        }
        super::lock_screen::keep_vault_key_for_touch_id(&vault, cx);
        super::reopen_locked_notes(&vault, cx);
        notice(&changed_sentence(changed), cx);
        Ok(())
    });
    PasswordForm {
        title: "Change the vault password",
        detail: None,
        fields: vec!["Current password", "New password", "Type the new one again"],
        submit_label: "Change password",
        on_submit,
    }
}

fn first_password_form(vault: PathBuf) -> PasswordForm {
    let on_submit: PasswordSubmit = Box::new(move |values, _, cx| {
        matching(&values[0], &values[1])?;
        vault_encryption(&vault, cx, |encryption| encryption.set_vault_password(&values[0]))
            .unwrap_or(Err(EncryptionError::NoVaultPassword))
            .map_err(crypt_message)?;
        super::lock_screen::keep_vault_key_for_touch_id(&vault, cx);
        notice("The vault has a password now.", cx);
        Ok(())
    });
    PasswordForm {
        title: "Choose a vault password",
        detail: Some(
            "It unlocks every encrypted note in this vault. Gasp can’t recover it if it’s forgotten, so keep it somewhere safe.".to_owned(),
        ),
        fields: vec!["Vault password", "Type it again"],
        submit_label: "Set password",
        on_submit,
    }
}

/// What a password change did, as a sentence.
fn changed_sentence(changed: usize) -> String {
    match changed {
        0 => "Changed the vault password.".to_owned(),
        1 => "Changed the vault password. 1 note now unlocks with the new one.".to_owned(),
        count => format!("Changed the vault password. {count} notes now unlock with the new one."),
    }
}
