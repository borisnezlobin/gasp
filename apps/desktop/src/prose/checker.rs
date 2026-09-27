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
use editor_prose::vocabulary::{NOTES_TO_LEARN, learn};
use editor_prose::{CheckOptions, Checker, English, Flag, Unit};
use futures::channel::oneshot;
use gpui::{App, AppContext, Global};

use crate::note_texts::NoteTexts;

/// Where dismissed phrases are kept, from the vault root.
pub const IGNORED_FILE: &str = ".editor/prose/ignored.txt";

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

/// The shared grammar state.
#[derive(Default)]
pub struct Grammar {
    setup: Setup,
    /// Moves on whenever something changes what a check finds, so editors
    /// know their cached flags are stale.
    generation: u64,
    ignored_file: Option<PathBuf>,
}

impl Global for Grammar {}

fn grammar(cx: &mut App) -> &mut Grammar {
    cx.default_global::<Grammar>()
}

/// The generation cached flags must match to be current.
pub fn generation(cx: &App) -> u64 {
    cx.try_global::<Grammar>().map_or(0, |g| g.generation)
}

/// Lower-cased phrases never flagged.
pub fn ignored(cx: &App) -> Arc<HashSet<String>> {
    cx.try_global::<Grammar>()
        .map(|g| g.setup.ignored.clone())
        .unwrap_or_default()
}

/// Follows the grammar settings.
pub fn configure(settings: &GrammarSettings, cx: &mut App) {
    let options = CheckOptions {
        spelling: settings.spelling,
        english: english(settings.english),
    };
    let grammar = grammar(cx);
    if grammar.setup.options != options {
        grammar.setup.options = options;
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
    let file = vault.join(IGNORED_FILE);
    grammar(cx).ignored_file = Some(file.clone());
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
            grammar.setup.ignored = Arc::new(ignored);
            grammar.setup.known = Arc::new(known);
            grammar.generation += 1;
        })
        .ok();
        cx.background_executor().timer(WARM_UP_DELAY).await;
        // An empty batch builds the checker.
        cx.update(|cx| drop(check(Vec::new(), cx))).ok();
    })
    .detach();
}

/// Checks paragraphs on the worker. Resolves to nothing if the worker
/// has gone.
pub fn check(jobs: Vec<Job>, cx: &mut App) -> oneshot::Receiver<Checked> {
    let (reply, receiver) = oneshot::channel();
    let setup = grammar(cx).setup.clone();
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

/// Never flags `phrase` again, here or on other devices.
pub fn ignore(phrase: &str, cx: &mut App) {
    let grammar = grammar(cx);
    let mut ignored = (*grammar.setup.ignored).clone();
    if !ignored.insert(phrase.to_lowercase()) {
        return;
    }
    grammar.setup.ignored = Arc::new(ignored.clone());
    grammar.generation += 1;
    let Some(file) = grammar.ignored_file.clone() else {
        return;
    };
    cx.background_spawn(async move {
        if let Err(error) = write_ignored(&file, &ignored) {
            eprintln!("could not save {}: {error}", file.display());
        }
    })
    .detach();
}

fn read_ignored(file: &Path) -> HashSet<String> {
    std::fs::read_to_string(file)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_lowercase)
        .collect()
}

fn write_ignored(file: &Path, ignored: &HashSet<String>) -> std::io::Result<()> {
    let mut phrases: Vec<&str> = ignored.iter().map(String::as_str).collect();
    phrases.sort_unstable();
    let mut text = String::from("# Phrases the grammar checker leaves alone, one per line.\n");
    for phrase in phrases {
        text.push_str(phrase);
        text.push('\n');
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::workspace::files::atomic_write(file, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

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
