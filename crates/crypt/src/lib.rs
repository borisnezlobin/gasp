//! Encrypted notes. A note's file holds a short readable header and its
//! text sealed with XChaCha20-Poly1305; the app keeps the plain text in
//! memory only. Each note has its own random key, and that key is stored
//! in the header wrapped by the vault's key (made from the vault password
//! with Argon2id) or by a key made from the note's own password. Changing
//! a password rewraps each note's key without touching its text.
//!
//! The crate does no file I/O: callers read and write the text it seals.

mod aead;
mod error;
mod key;
mod note;
mod vault_lock;

pub use error::CryptError;
pub use key::{KdfParams, Key, derive_key, random_salt};
pub use note::{
    ClaudeAccess, Header, KeySource, NameVisibility, NoteOptions, Opened, Plain, claude_key_id,
    is_encrypted, open, open_with_note_key, peek, reseal, rewrap, seal,
};
pub use vault_lock::{LOCK_FILE, VaultLock};
