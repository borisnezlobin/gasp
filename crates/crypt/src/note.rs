//! An encrypted note's file:
//!
//! ```text
//! <!-- Encrypted with Gasp. Open it in Gasp to read it. -->
//! gasp-encrypted: 1
//! key: vault
//! claude: never
//! name: shown
//! wrapped: <the note's own key, sealed by the vault's key>
//!
//! <the note's text, sealed by the note's own key, as base64 lines>
//! ```
//!
//! A note with its own password says `key: note` and adds its `salt:` and
//! `kdf: <KiB> <passes> <lanes>`. Every header line but `wrapped` is bound
//! into the seal: changing `claude: never` to `always` by hand makes the
//! note fail to open instead of changing who may read it. `wrapped` is
//! left out so a new password can rewrap the note's key without
//! resealing its text.

use std::collections::HashMap;

use base64::Engine;
use base64::engine::general_purpose::{STANDARD as BASE64, URL_SAFE_NO_PAD};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::CryptError;
use crate::aead::{open_bytes, seal_bytes};
use crate::key::{KEY_LEN, KdfParams, Key, derive_key, random_salt};

const NOTICE: &str = "<!-- Encrypted with Gasp. Open it in Gasp to read it. -->";
const MARKER: &str = "gasp-encrypted";
const VERSION: u32 = 1;
const WRAP_AAD: &[u8] = b"gasp note key v1";
const LINE_WIDTH: usize = 76;
const TITLE_PREFIX: &str = "title: ";

/// Who may read the note through Claude's Gasp connector.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClaudeAccess {
    #[default]
    Never,
    /// Only while it's unlocked in the app.
    WhileUnlocked,
    Always,
}

impl ClaudeAccess {
    pub const ALL: [ClaudeAccess; 3] = [Self::Never, Self::WhileUnlocked, Self::Always];

    pub fn name(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::WhileUnlocked => "while-unlocked",
            Self::Always => "always",
        }
    }

    pub fn from_name(name: &str) -> Option<ClaudeAccess> {
        Self::ALL.into_iter().find(|access| access.name() == name)
    }
}

/// Whether the file's name is the note's title, or a stand-in that hides
/// it until the note is unlocked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NameVisibility {
    #[default]
    Shown,
    Hidden,
}

impl NameVisibility {
    pub fn name(self) -> &'static str {
        match self {
            Self::Shown => "shown",
            Self::Hidden => "hidden",
        }
    }

    pub fn from_name(name: &str) -> Option<NameVisibility> {
        [Self::Shown, Self::Hidden]
            .into_iter()
            .find(|visibility| visibility.name() == name)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoteOptions {
    pub claude: ClaudeAccess,
    pub name: NameVisibility,
}

/// Which password's key wraps the note's own key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeySource {
    Vault,
    Note { salt: Vec<u8>, params: KdfParams },
}

impl KeySource {
    /// A source for a new note password, and the key it makes.
    pub fn new_note_password(
        password: &str,
        params: KdfParams,
    ) -> Result<(KeySource, Key), CryptError> {
        let salt = random_salt().to_vec();
        let key = derive_key(password, &salt, params)?;
        Ok((KeySource::Note { salt, params }, key))
    }

    /// The key `password` makes for a note with its own password; `None`
    /// for a note the vault password opens.
    pub fn note_password_key(&self, password: &str) -> Option<Result<Key, CryptError>> {
        match self {
            KeySource::Vault => None,
            KeySource::Note { salt, params } => Some(derive_key(password, salt, *params)),
        }
    }

    /// A name for a note password that never reveals it, for naming a key
    /// kept in the system keychain; `None` for the vault password, which
    /// [`crate::VaultLock::id`] names.
    pub fn id(&self) -> Option<String> {
        match self {
            KeySource::Vault => None,
            KeySource::Note { salt, .. } => Some(format!("note-{}", URL_SAFE_NO_PAD.encode(salt))),
        }
    }

    fn describe(&self) -> String {
        match self {
            KeySource::Vault => "vault".to_owned(),
            KeySource::Note { salt, params } => {
                format!("note {} {}", BASE64.encode(salt), describe_params(*params))
            }
        }
    }
}

/// What a note's header says, readable without any key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub source: KeySource,
    pub options: NoteOptions,
    wrapped: Vec<u8>,
}

/// A note's text, and its title when the file's name hides it. Wiped
/// from memory when dropped.
#[derive(Clone, PartialEq, Eq, Default, Zeroize, ZeroizeOnDrop)]
pub struct Plain {
    pub body: String,
    pub title: Option<String>,
}

impl std::fmt::Debug for Plain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Plain({} bytes)", self.body.len())
    }
}

/// An opened note: its text, its header, and its own key for sealing it
/// again.
#[derive(Debug)]
pub struct Opened {
    pub plain: Plain,
    pub header: Header,
    pub note_key: Key,
}

/// Whether `text` is an encrypted note's file: its first line, or the
/// first after the notice, is the marker. Looks at two lines at most, so
/// it's cheap enough to ask of every note.
pub fn is_encrypted(text: &str) -> bool {
    let is_marker = |line: &str| {
        line.strip_prefix(MARKER)
            .is_some_and(|rest| rest.starts_with(':'))
    };
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    match lines.next() {
        Some(first) if first.starts_with("<!--") => lines.next().is_some_and(is_marker),
        Some(first) => is_marker(first),
        None => false,
    }
}

/// The header of an encrypted note, without opening it.
pub fn peek(text: &str) -> Result<Header, CryptError> {
    parse(text).map(|(header, _)| header)
}

/// The note's file: `plain` sealed by `note_key`, which is wrapped by
/// `wrapping_key` (the vault's key, or the note password's, as `source`
/// says).
pub fn seal(
    plain: &Plain,
    options: NoteOptions,
    source: KeySource,
    wrapping_key: &Key,
    note_key: &Key,
) -> String {
    let wrapped = seal_bytes(wrapping_key, note_key.bytes(), WRAP_AAD);
    let header = Header {
        source,
        options,
        wrapped,
    };
    let payload = payload_of(plain, options);
    let sealed = seal_bytes(note_key, payload.as_bytes(), &bound_header(&header));
    serialize(&header, &BASE64.encode(sealed))
}

/// The note in `text`, opened with `wrapping_key`.
pub fn open(text: &str, wrapping_key: &Key) -> Result<Opened, CryptError> {
    let (header, body) = parse(text)?;
    let note_key = unwrap_note_key(&header, wrapping_key)?;
    let sealed = BASE64.decode(body).map_err(|_| CryptError::Corrupt)?;
    let payload =
        open_bytes(&note_key, &sealed, &bound_header(&header)).ok_or(CryptError::Corrupt)?;
    let payload = std::str::from_utf8(&payload).map_err(|_| CryptError::Corrupt)?;
    Ok(Opened {
        plain: plain_from(payload)?,
        header,
        note_key,
    })
}

/// `text` with the note's own key wrapped by `new_key` instead of
/// `old_key`, its sealed text untouched. For a vault password change,
/// where the note's key source stays the same.
pub fn rewrap(text: &str, old_key: &Key, new_key: &Key) -> Result<String, CryptError> {
    let (mut header, body) = parse(text)?;
    let note_key = unwrap_note_key(&header, old_key)?;
    header.wrapped = seal_bytes(new_key, note_key.bytes(), WRAP_AAD);
    Ok(serialize(&header, &body))
}

fn unwrap_note_key(header: &Header, wrapping_key: &Key) -> Result<Key, CryptError> {
    let bytes =
        open_bytes(wrapping_key, &header.wrapped, WRAP_AAD).ok_or(CryptError::WrongPassword)?;
    let bytes: [u8; KEY_LEN] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| CryptError::Corrupt)?;
    Ok(Key::from_bytes(bytes))
}

/// The plain text sealed: the title line when the name is hidden, a
/// blank line, then the body.
fn payload_of(plain: &Plain, options: NoteOptions) -> Zeroizing<String> {
    let title = plain
        .title
        .as_deref()
        .filter(|_| options.name == NameVisibility::Hidden)
        .map(|title| format!("{TITLE_PREFIX}{}\n", title.replace(['\n', '\r'], " ")));
    Zeroizing::new(format!("{}\n{}", title.unwrap_or_default(), plain.body))
}

fn plain_from(payload: &str) -> Result<Plain, CryptError> {
    let mut rest = payload;
    let mut title = None;
    loop {
        let (line, after) = rest.split_once('\n').ok_or(CryptError::Corrupt)?;
        rest = after;
        if line.is_empty() {
            break;
        }
        if let Some(found) = line.strip_prefix(TITLE_PREFIX) {
            title = Some(found.to_owned());
        }
    }
    Ok(Plain {
        body: rest.to_owned(),
        title,
    })
}

/// The header lines bound into the seal, in a fixed form.
fn bound_header(header: &Header) -> Vec<u8> {
    format!(
        "{MARKER} {VERSION}\nkey {}\nclaude {}\nname {}",
        header.source.describe(),
        header.options.claude.name(),
        header.options.name.name()
    )
    .into_bytes()
}

fn serialize(header: &Header, body: &str) -> String {
    let mut out = format!("{NOTICE}\n{MARKER}: {VERSION}\n");
    match &header.source {
        KeySource::Vault => out.push_str("key: vault\n"),
        KeySource::Note { salt, params } => out.push_str(&format!(
            "key: note\nsalt: {}\nkdf: {}\n",
            BASE64.encode(salt),
            describe_params(*params)
        )),
    }
    out.push_str(&format!(
        "claude: {}\nname: {}\nwrapped: {}\n\n",
        header.options.claude.name(),
        header.options.name.name(),
        BASE64.encode(&header.wrapped)
    ));
    for line in body.as_bytes().chunks(LINE_WIDTH) {
        out.push_str(std::str::from_utf8(line).expect("base64 is ASCII"));
        out.push('\n');
    }
    out
}

/// The header and the base64 body joined into one line.
fn parse(text: &str) -> Result<(Header, String), CryptError> {
    if !is_encrypted(text) {
        return Err(CryptError::NotEncrypted);
    }
    let mut lines = text.lines().map(str::trim);
    let mut fields = HashMap::new();
    for line in lines.by_ref() {
        if line.starts_with("<!--") || (line.is_empty() && fields.is_empty()) {
            continue;
        }
        if line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(':').ok_or(CryptError::Corrupt)?;
        fields.insert(name.trim(), value.trim());
    }
    let body = lines.flat_map(str::split_whitespace).collect();
    Ok((header_from(&fields)?, body))
}

fn header_from(fields: &HashMap<&str, &str>) -> Result<Header, CryptError> {
    let version: u32 = field(fields, MARKER)?
        .parse()
        .map_err(|_| CryptError::Corrupt)?;
    if version > VERSION {
        return Err(CryptError::NewerVersion);
    }
    let options = NoteOptions {
        claude: ClaudeAccess::from_name(field(fields, "claude")?).ok_or(CryptError::Corrupt)?,
        name: NameVisibility::from_name(field(fields, "name")?).ok_or(CryptError::Corrupt)?,
    };
    Ok(Header {
        source: source_from(fields)?,
        options,
        wrapped: decode(field(fields, "wrapped")?)?,
    })
}

fn source_from(fields: &HashMap<&str, &str>) -> Result<KeySource, CryptError> {
    match field(fields, "key")? {
        "vault" => Ok(KeySource::Vault),
        "note" => Ok(KeySource::Note {
            salt: decode(field(fields, "salt")?)?,
            params: parse_params(field(fields, "kdf")?)?,
        }),
        _ => Err(CryptError::Corrupt),
    }
}

fn field<'a>(fields: &HashMap<&str, &'a str>, name: &str) -> Result<&'a str, CryptError> {
    fields.get(name).copied().ok_or(CryptError::Corrupt)
}

fn decode(text: &str) -> Result<Vec<u8>, CryptError> {
    BASE64.decode(text).map_err(|_| CryptError::Corrupt)
}

fn describe_params(params: KdfParams) -> String {
    format!(
        "{} {} {}",
        params.memory_kib, params.iterations, params.parallelism
    )
}

fn parse_params(text: &str) -> Result<KdfParams, CryptError> {
    let numbers: Vec<u32> = text
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .map_err(|_| CryptError::Corrupt)?;
    let [memory_kib, iterations, parallelism] = numbers[..] else {
        return Err(CryptError::Corrupt);
    };
    Ok(KdfParams {
        memory_kib,
        iterations,
        parallelism,
    })
}

#[cfg(test)]
mod tests;
