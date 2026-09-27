//! Generator for the synthetic vault that every committed fixture comes from.
//!
//! [`generate`] is deterministic: the same seed and options always give the
//! same notes and attachments, byte for byte. [`scan`] measures features in
//! the generated text independently, and [`manifest`] writes those
//! measurements to `manifest.json`.

use std::fs;
use std::io;
use std::path::Path;

mod blocks;
mod inline;
pub mod manifest;
mod math;
mod note;
mod plan;
mod png;
mod prose;
mod rng;
pub mod scan;
pub mod targets;

pub use rng::Rng;

/// Seed used for the committed corpus.
pub const DEFAULT_SEED: u64 = 20_260_927;

/// Name of the manifest written next to the notes.
pub const MANIFEST_FILE: &str = "manifest.json";

/// A `.gitattributes` written into the output so git never rewrites line
/// endings in the fixtures (tests compare them byte for byte).
pub const GITATTRIBUTES: (&str, &str) = (".gitattributes", "* -text\n");

/// Generator options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Number of notes. Feature counts scale with it from the 204-note table.
    pub notes: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            notes: targets::VAULT_NOTE_COUNT,
        }
    }
}

/// One Markdown note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedNote {
    /// Vault-relative path with `/` separators, ending in `.md`.
    pub path: String,
    pub text: String,
}

/// One binary attachment (a tiny PNG).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    /// Vault-relative path with `/` separators.
    pub path: String,
    pub bytes: Vec<u8>,
}

/// A way a footnote can be broken on purpose.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FootnoteProblem {
    /// Referenced but never defined.
    Missing,
    /// Defined but never referenced.
    Unused,
    /// Defined more than once.
    Duplicate,
    /// Defined with no text.
    Empty,
    /// Referenced as `^[1]` instead of `[^1]`.
    Typo,
}

impl FootnoteProblem {
    /// Lowercase name used in the manifest.
    pub fn name(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Unused => "unused",
            Self::Duplicate => "duplicate",
            Self::Empty => "empty",
            Self::Typo => "typo",
        }
    }
}

/// A footnote the generator broke deliberately.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BrokenFootnote {
    pub note: String,
    pub label: String,
    pub problem: FootnoteProblem,
}

/// A generated vault.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vault {
    pub seed: u64,
    pub notes: Vec<GeneratedNote>,
    pub attachments: Vec<Attachment>,
    /// Footnotes broken on purpose, so tests can tell them from bugs.
    pub broken_footnotes: Vec<BrokenFootnote>,
}

/// Generates a vault.
pub fn generate(seed: u64, options: &Options) -> Vault {
    let mut rng = Rng::stream(seed, 0);
    let plans = plan::plan_notes(options.notes, &mut rng);
    let metas = note::note_metas(&mut rng, &plans);
    let mut ctx = note::VaultContext::new(metas);
    let notes = plans
        .iter()
        .enumerate()
        .map(|(index, plan)| GeneratedNote {
            path: ctx.metas[index].path(),
            text: note::write_note(seed, index, plan, &mut ctx),
        })
        .collect();
    Vault {
        seed,
        notes,
        attachments: ctx.attachments,
        broken_footnotes: ctx.broken,
    }
}

impl Vault {
    /// Writes the notes, attachments, `manifest.json` and `.gitattributes` under `dir`.
    ///
    /// An existing `dir` is replaced only when it is empty or holds an earlier
    /// corpus (it has a `manifest.json`), so a wrong path can't wipe other files.
    pub fn write_to(&self, dir: &Path) -> io::Result<()> {
        prepare_output_dir(dir)?;
        for note in &self.notes {
            write_file(dir, &note.path, note.text.as_bytes())?;
        }
        for attachment in &self.attachments {
            write_file(dir, &attachment.path, &attachment.bytes)?;
        }
        write_file(dir, GITATTRIBUTES.0, GITATTRIBUTES.1.as_bytes())?;
        write_file(dir, MANIFEST_FILE, manifest::manifest_json(self).as_bytes())
    }
}

fn prepare_output_dir(dir: &Path) -> io::Result<()> {
    if dir.exists() {
        let is_empty = fs::read_dir(dir)?.next().is_none();
        if !is_empty && !dir.join(MANIFEST_FILE).is_file() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "{} is not empty and has no {MANIFEST_FILE}; refusing to replace it",
                    dir.display()
                ),
            ));
        }
        fs::remove_dir_all(dir)?;
    }
    fs::create_dir_all(dir)
}

fn write_file(dir: &Path, relative: &str, bytes: &[u8]) -> io::Result<()> {
    let path = relative
        .split('/')
        .fold(dir.to_path_buf(), |path, part| path.join(part));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)
}
