//! The grammar checker's worker and what every editor shares about it:
//! the settings, the words the vault knows, and the phrases the writer
//! dismissed.
//!
//! Harper takes a moment to build and holds caches, so one checker lives
//! on its own thread for the life of the app, and editors send it
//! paragraphs in batches. Nothing on the main thread waits for it.
//!
//! Dismissed phrases live in the vault's `.editor/prose/ignored.txt`, one
//! per line, so they sync to every device like the rest of the config.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use editor_config::settings::{EnglishVariant, GrammarSettings};
use editor_prose::vocabulary::{NOTES_TO_LEARN, ignored_file_text, learn, parse_ignored};
use editor_prose::{CheckOptions, Checker, English, Flag, Unit};
use futures::channel::oneshot;
use gpui::{App, AppContext, Global};

use crate::note_texts::NoteTexts;

pub use editor_prose::vocabulary::IGNORED_FILE;

/// How long after a vault opens the checker starts building, so it
/// doesn't compete with the first frames.
const WARM_UP_DELAY: Duration = Duration::from_secs(2);

/// The one worker, shared by every window.
static WORKER: OnceLock<Sender<Request>> = OnceLock::new();

/// One paragraph to check: its text alone, with the unit's pieces
/// counted from the paragraph's start.
pub struct Job {
    pub key: u64,
    pub text: String,
    pub unit: Unit,
}

/// Flags for each paragraph, counted from its start.
pub type Checked = Vec<(u64, Vec<Flag>)>;

/// What the worker needs besides the paragraphs.
#[derive(Clone, Default)]
struct Setup {
    options: CheckOptions,
    known: Arc<HashSet<String>>,
    ignored: Arc<HashSet<String>>,
}

struct Request {
    jobs: Vec<Job>,
    setup: Setup,
    reply: oneshot::Sender<Checked>,
}

/// One open vault's settings, the words its notes use and the phrases
/// its writer dismissed.
struct VaultWords {
    vault: PathBuf,
    setup: Setup,
    ignored_file: PathBuf,
}

/// The shared grammar state: each open vault's own words and settings,
/// so two vault windows never mix their ignore lists.
#[derive(Default)]
pub struct Grammar {
    vaults: Vec<VaultWords>,
    /// Moves on whenever something changes what a check finds, so editors
    /// know their cached flags are stale.
    generation: u64,
}

impl Global for Grammar {}

impl Grammar {
    /// The vault `note` is in, the deepest if vaults nest; a note with no
    /// file goes with the vault opened last.
    fn vault_for(&self, note: Option<&Path>) -> Option<usize> {
        let Some(note) = note else {
            return self.vaults.len().checked_sub(1);
        };
        (0..self.vaults.len())
            .filter(|&at| note.starts_with(&self.vaults[at].vault))
            .max_by_key(|&at| self.vaults[at].vault.components().count())
            .or(self.vaults.len().checked_sub(1))
    }

    fn setup_for(&self, note: Option<&Path>) -> Setup {
        self.vault_for(note)
            .map(|at| self.vaults[at].setup.clone())
            .unwrap_or_default()
    }

    /// The entry for `vault`, made on first use.
    fn vault_mut(&mut self, vault: &Path) -> &mut VaultWords {
        let at = match self.vaults.iter().position(|open| open.vault == vault) {
            Some(at) => at,
            None => {
                self.vaults.push(VaultWords {
                    vault: vault.to_path_buf(),
                    setup: Setup::default(),
                    ignored_file: vault.join(IGNORED_FILE),
                });
                self.vaults.len() - 1
            }
        };
        &mut self.vaults[at]
    }
}

fn grammar(cx: &mut App) -> &mut Grammar {
    cx.default_global::<Grammar>()
}

/// The generation cached flags must match to be current.
pub fn generation(cx: &App) -> u64 {
    cx.try_global::<Grammar>().map_or(0, |g| g.generation)
}

/// Lower-cased phrases never flagged in the vault of the note at `note`.
pub fn ignored(note: Option<&Path>, cx: &App) -> Arc<HashSet<String>> {
    cx.try_global::<Grammar>()
        .map(|g| g.setup_for(note).ignored)
        .unwrap_or_default()
}

/// Follows the grammar settings of the vault at `vault`.
pub fn configure(vault: &Path, settings: &GrammarSettings, cx: &mut App) {
    let options = CheckOptions {
        spelling: settings.spelling,
        english: english(settings.english),
    };
    let grammar = grammar(cx);
    let words = grammar.vault_mut(vault);
    if words.setup.options != options {
        words.setup.options = options;
        grammar.generation += 1;
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

/// Reads the vault's dismissed phrases and learns the words its notes
/// use, both off the main thread, then starts building the checker so
/// the first paragraph doesn't wait for it.
pub fn open_vault(vault: &Path, texts: NoteTexts, cx: &mut App) {
    let file = grammar(cx).vault_mut(vault).ignored_file.clone();
    let vault = vault.to_path_buf();
    let load = cx.background_spawn(async move {
        let ignored = read_ignored(&file);
        let notes = texts.load();
        let known = learn(notes.iter().map(|note| &*note.text), NOTES_TO_LEARN);
        (ignored, known)
    });
    cx.spawn(async move |cx| {
        let (ignored, known) = load.await;
        cx.update(|cx| {
            let grammar = grammar(cx);
            let words = grammar.vault_mut(&vault);
            words.setup.ignored = Arc::new(ignored);
            words.setup.known = Arc::new(known);
            grammar.generation += 1;
        })
        .ok();
        cx.background_executor().timer(WARM_UP_DELAY).await;
        // An empty batch builds the checker.
        cx.update(|cx| drop(check(Vec::new(), None, cx))).ok();
    })
    .detach();
}

/// Checks paragraphs of the note at `note` on the worker, with its
/// vault's words and settings. Resolves to nothing if the worker has gone.
pub fn check(jobs: Vec<Job>, note: Option<&Path>, cx: &mut App) -> oneshot::Receiver<Checked> {
    let (reply, receiver) = oneshot::channel();
    let setup = grammar(cx).setup_for(note);
    // A worker that died drops the request, and with it the reply.
    WORKER
        .get_or_init(spawn_worker)
        .send(Request { jobs, setup, reply })
        .ok();
    receiver
}

fn spawn_worker() -> Sender<Request> {
    let (sender, requests) = mpsc::channel();
    std::thread::Builder::new()
        .name("grammar".into())
        .spawn(move || run_worker(requests))
        .ok();
    sender
}

fn run_worker(requests: Receiver<Request>) {
    let mut checker: Option<Checker> = None;
    while let Ok(request) = requests.recv() {
        let current = checker
            .as_ref()
            .is_some_and(|checker| *checker.options() == request.setup.options);
        if !current {
            checker = Some(Checker::new(request.setup.options.clone()));
        }
        let Some(active) = checker.as_mut() else {
            continue;
        };
        active.set_known_words(request.setup.known);
        active.set_ignored(request.setup.ignored);
        let checked = request
            .jobs
            .into_iter()
            .map(|job| (job.key, active.check(&job.text, &job.unit)))
            .collect();
        request.reply.send(checked).ok();
    }
}

/// Never flags `phrase` again in the vault of the note at `note`, here
/// or on other devices.
pub fn ignore(phrase: &str, note: Option<&Path>, cx: &mut App) {
    let grammar = grammar(cx);
    let Some(at) = grammar.vault_for(note) else {
        return;
    };
    let words = &mut grammar.vaults[at];
    let mut ignored = (*words.setup.ignored).clone();
    if !ignored.insert(phrase.to_lowercase()) {
        return;
    }
    words.setup.ignored = Arc::new(ignored.clone());
    let file = words.ignored_file.clone();
    grammar.generation += 1;
    cx.background_spawn(async move {
        if let Err(error) = write_ignored(&file, &ignored) {
            eprintln!("could not save {}: {error}", file.display());
        }
    })
    .detach();
}

fn read_ignored(file: &Path) -> HashSet<String> {
    parse_ignored(&std::fs::read_to_string(file).unwrap_or_default())
}

fn write_ignored(file: &Path, ignored: &HashSet<String>) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::workspace::files::atomic_write(file, &ignored_file_text(ignored))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_vault_keeps_its_own_ignore_list() {
        let mut grammar = Grammar::default();
        let (a, b) = (Path::new("/vaults/a"), Path::new("/vaults/b"));
        grammar.vault_mut(a).setup.ignored = Arc::new(HashSet::from(["teh".to_owned()]));
        grammar.vault_mut(b);
        let ignored = |note: &str| grammar.setup_for(Some(Path::new(note))).ignored;
        assert!(ignored("/vaults/a/Note.md").contains("teh"));
        assert!(ignored("/vaults/b/Note.md").is_empty());
        assert!(
            grammar.setup_for(None).ignored.is_empty(),
            "a note with no file goes with the vault opened last"
        );
    }

    #[test]
    fn ignored_phrases_round_trip_through_the_file() {
        let vault = tempfile::tempdir().unwrap();
        let file = vault.path().join(IGNORED_FILE);
        let phrases = HashSet::from(["teh".to_owned(), "the the".to_owned()]);
        write_ignored(&file, &phrases).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.ends_with("teh\nthe the\n"), "{text}");
        assert_eq!(read_ignored(&file), phrases);
    }
}
