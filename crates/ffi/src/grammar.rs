//! The grammar checker on the phone: the desktop's first two layers,
//! Harper's mechanical checks and spelling that learns the vault's words,
//! with the same ignore file.
//!
//! Harper takes a moment to build, so the checker is built on first use
//! and kept. Checking is slow next to drawing: the phone calls
//! [`GrammarChecker::check`] off the main thread. Paragraphs are checked
//! once until their text changes, and the note is only locked while its
//! paragraphs are copied out, so typing never waits for a check.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use editor_config::settings::{EnglishVariant, GrammarSettings};
use editor_prose::projection::Piece;
use editor_prose::vocabulary::{
    IGNORED_FILE, NOTES_TO_LEARN, ignored_file_text, learn, parse_ignored,
};
use editor_prose::{CheckOptions, Checker, English, Flag, FlagKind, Purpose, Unit, units};
use editor_search::engine::load_vault;
use editor_vault::files::atomic_write;

use crate::document::NoteDocument;
use crate::offsets::{TextRange, Utf16Offsets};
use crate::vault::{VaultError, VaultFolder};

/// Paragraphs whose flags are kept before the cache starts over.
const CACHE_LIMIT: usize = 4096;

/// What a flag is about, which picks its underline's colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum GrammarFlagKind {
    Spelling,
    /// Spacing, repeated words, articles and the like.
    Mechanical,
}

/// One problem in a note.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct GrammarFlag {
    pub range: TextRange,
    pub kind: GrammarFlagKind,
    /// A plain sentence saying what's wrong.
    pub message: String,
    /// Text that could replace the range, best first. Empty text removes it.
    pub replacements: Vec<String>,
}

#[derive(uniffi::Object)]
pub struct GrammarChecker {
    settings: GrammarSettings,
    root: PathBuf,
    words: Mutex<Words>,
    checking: Mutex<Checking>,
}

/// The vault's own words and the phrases its writer dismissed.
#[derive(Default)]
struct Words {
    known: Arc<HashSet<String>>,
    ignored: Arc<HashSet<String>>,
}

/// The checker, once built, and each paragraph's flags by its text.
#[derive(Default)]
struct Checking {
    checker: Option<Checker>,
    /// Flags counted from the paragraph's start, by a hash of its text.
    flags: HashMap<u64, Vec<Flag>>,
    /// The known words the cached flags were found with.
    known: Arc<HashSet<String>>,
}

/// A paragraph copied out of the note to check.
struct Paragraph {
    key: u64,
    text: String,
    unit: Unit,
    /// Where it starts in the note, in UTF-16.
    start: u32,
}

#[uniffi::export]
impl GrammarChecker {
    /// A checker following the vault's grammar settings. It reads the
    /// ignore file now; the vault's words are learned by
    /// [`GrammarChecker::learn_vault_words`].
    #[uniffi::constructor]
    pub fn new(vault: Arc<VaultFolder>) -> Arc<Self> {
        let settings = vault.config().settings.prose.grammar.clone();
        let ignored = std::fs::read_to_string(vault.root.join(IGNORED_FILE)).unwrap_or_default();
        Arc::new(Self {
            settings,
            root: vault.root.clone(),
            words: Mutex::new(Words {
                known: Arc::default(),
                ignored: Arc::new(parse_ignored(&ignored)),
            }),
            checking: Mutex::default(),
        })
    }

    /// Whether problems are underlined, from `prose.grammar.enabled`.
    pub fn is_enabled(&self) -> bool {
        self.settings.enabled
    }

    /// Reads every note to learn the words the vault uses in several of
    /// them, which spelling then leaves alone. Slow; call it off the main
    /// thread.
    pub fn learn_vault_words(&self) {
        let notes = load_vault(&self.root);
        let known = learn(notes.iter().map(|note| &*note.text), NOTES_TO_LEARN);
        self.words().known = Arc::new(known);
    }

    /// The problems in the paragraphs of `document` that overlap `within`
    /// (UTF-16), leaving out dismissed phrases. Slow the first time a
    /// paragraph is seen; call it off the main thread.
    pub fn check(&self, document: Arc<NoteDocument>, within: TextRange) -> Vec<GrammarFlag> {
        if !self.settings.enabled {
            return Vec::new();
        }
        let paragraphs = paragraphs(&document, within);
        let (known, ignored) = {
            let words = self.words();
            (words.known.clone(), words.ignored.clone())
        };
        let mut checking = self.checking();
        let checking = &mut *checking;
        if !Arc::ptr_eq(&checking.known, &known) || checking.flags.len() > CACHE_LIMIT {
            checking.flags.clear();
            checking.known = known.clone();
        }
        let checker = checking
            .checker
            .get_or_insert_with(|| Checker::new(self.options()));
        checker.set_known_words(known);
        let mut found = Vec::new();
        for paragraph in &paragraphs {
            let flags = checking
                .flags
                .entry(paragraph.key)
                .or_insert_with(|| checker.check(&paragraph.text, &paragraph.unit));
            found.extend(shown_flags(paragraph, flags, &ignored));
        }
        found
    }

    /// Never flags `phrase` again, here or on the vault's other devices.
    pub fn ignore(&self, phrase: String) -> Result<(), VaultError> {
        let mut words = self.words();
        let mut ignored = (*words.ignored).clone();
        if !ignored.insert(phrase.to_lowercase()) {
            return Ok(());
        }
        let file = self.root.join(IGNORED_FILE);
        if let Some(folder) = file.parent() {
            std::fs::create_dir_all(folder)?;
        }
        atomic_write(&file, &ignored_file_text(&ignored))?;
        words.ignored = Arc::new(ignored);
        Ok(())
    }
}

impl GrammarChecker {
    fn options(&self) -> CheckOptions {
        CheckOptions {
            spelling: self.settings.spelling,
            english: english(self.settings.english),
        }
    }

    fn words(&self) -> MutexGuard<'_, Words> {
        self.words.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn checking(&self) -> MutexGuard<'_, Checking> {
        self.checking.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn english(variant: EnglishVariant) -> English {
    match variant {
        EnglishVariant::American => English::American,
        EnglishVariant::British => English::British,
        EnglishVariant::Canadian => English::Canadian,
        EnglishVariant::Australian => English::Australian,
    }
}

/// The note's prose paragraphs overlapping `within`, each on its own with
/// its pieces counted from its start.
fn paragraphs(document: &NoteDocument, within: TextRange) -> Vec<Paragraph> {
    let parsed = document.lock();
    let bytes = parsed.offsets.byte_range(within);
    units(&parsed.tree, bytes, Purpose::Grammar)
        .into_iter()
        .map(|unit| {
            let text = parsed.text[unit.range.clone()].to_owned();
            Paragraph {
                key: key_of(&text),
                start: parsed.offsets.utf16(unit.range.start),
                unit: relative_unit(&unit),
                text,
            }
        })
        .collect()
}

fn relative_unit(unit: &Unit) -> Unit {
    let start = unit.range.start;
    Unit {
        range: 0..unit.range.len(),
        pieces: unit
            .pieces
            .iter()
            .map(|piece| {
                Piece::new(
                    piece.range.start - start..piece.range.end - start,
                    piece.kind,
                )
            })
            .collect(),
    }
}

fn key_of(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// A paragraph's flags in the note's UTF-16 offsets, less dismissed ones.
fn shown_flags(
    paragraph: &Paragraph,
    flags: &[Flag],
    ignored: &HashSet<String>,
) -> Vec<GrammarFlag> {
    let offsets = Utf16Offsets::new(&paragraph.text);
    flags
        .iter()
        .filter(|flag| {
            paragraph
                .text
                .get(flag.range.clone())
                .is_some_and(|phrase| !ignored.contains(&phrase.to_lowercase()))
        })
        .map(|flag| {
            let range = offsets.range(&flag.range);
            GrammarFlag {
                range: TextRange {
                    start: paragraph.start + range.start,
                    end: paragraph.start + range.end,
                },
                kind: match flag.kind {
                    FlagKind::Spelling => GrammarFlagKind::Spelling,
                    FlagKind::Mechanical => GrammarFlagKind::Mechanical,
                },
                message: flag.message.clone(),
                replacements: flag.replacements.clone(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::tests::vault_with;

    fn everything(document: &NoteDocument) -> TextRange {
        let end = document.text().encode_utf16().count() as u32;
        TextRange { start: 0, end }
    }

    #[test]
    fn repeated_words_and_misspellings_are_flagged_in_utf16() {
        let (_dir, vault) = vault_with(&[]);
        let checker = GrammarChecker::new(vault.clone());
        let document = vault.document("Café the the tree.\n\nA speling mistake.".into());
        let flags = checker.check(document.clone(), everything(&document));
        let repeated = flags
            .iter()
            .find(|flag| flag.kind == GrammarFlagKind::Mechanical)
            .unwrap();
        assert_eq!(repeated.range.start, 5);
        let spelling = flags
            .iter()
            .find(|flag| flag.kind == GrammarFlagKind::Spelling)
            .unwrap();
        assert!(spelling.replacements.contains(&"spelling".to_owned()));
    }

    #[test]
    fn a_dismissed_phrase_stays_quiet_and_is_written_down() {
        let (dir, vault) = vault_with(&[]);
        let checker = GrammarChecker::new(vault.clone());
        let document = vault.document("A speling mistake.".into());
        checker.ignore("Speling".into()).unwrap();
        assert!(
            checker
                .check(document.clone(), everything(&document))
                .is_empty()
        );
        let written = std::fs::read_to_string(dir.path().join(IGNORED_FILE)).unwrap();
        assert!(written.ends_with("speling\n"));
    }

    #[test]
    fn words_from_several_notes_are_known() {
        let note = "Nezlobin wrote this.";
        let (_dir, vault) = vault_with(&[("A.md", note), ("B.md", note), ("C.md", note)]);
        let checker = GrammarChecker::new(vault.clone());
        checker.learn_vault_words();
        let document = vault.document("Did nezlobin write?".into());
        assert!(
            checker
                .check(document.clone(), everything(&document))
                .is_empty()
        );
    }
}
