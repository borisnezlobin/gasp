//! The grammar checker in a process of its own.
//!
//! Harper's dictionaries take about 100 MB, which it keeps for the life
//! of the process that built them. So the app checks paragraphs in a child
//! process (`gasp grammar-worker`) that it closes after a minute without
//! work and starts again when there's more, and the app's own memory never
//! holds them. Requests and replies are JSON lines on the child's standard
//! input and output.
//!
//! Only the app itself runs the checker this way (see
//! [`use_worker_process`]); tests, and anything that can't start the child,
//! check on the worker thread instead.

use std::collections::HashSet;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, OnceLock};

use gasp_prose::projection::{Piece, PieceKind};
use gasp_prose::{CheckOptions, English, Flag, FlagKind, Unit};
use serde::{Deserialize, Serialize};

use super::checker::{Checked, InProcess, Job, Setup};

/// The argument that makes the program a grammar worker.
pub const WORKER_ARG: &str = "grammar-worker";

/// Tracing a child's own startup would only add noise to the app's.
const QUIET_ENV: [&str; 2] = ["EDITOR_TRACE_STARTUP", "EDITOR_TRACE_KEYS"];

static WORKER_EXE: OnceLock<PathBuf> = OnceLock::new();

/// Checks in a child process started from `exe`, this program, from now on.
pub fn use_worker_process(exe: PathBuf) {
    WORKER_EXE.get_or_init(|| exe);
}

pub(super) fn worker_exe() -> Option<&'static Path> {
    WORKER_EXE.get().map(PathBuf::as_path)
}

const ENGLISH: [English; 4] = [
    English::American,
    English::British,
    English::Canadian,
    English::Australian,
];
const PIECE_KINDS: [PieceKind; 3] = [PieceKind::Text, PieceKind::Atom, PieceKind::Space];
const FLAG_KINDS: [FlagKind; 2] = [FlagKind::Spelling, FlagKind::Mechanical];

fn index_of<T: PartialEq>(all: &[T], value: &T) -> u8 {
    all.iter().position(|item| item == value).unwrap_or(0) as u8
}

fn from_index<T: Copy>(all: &[T], index: u8) -> T {
    all.get(usize::from(index)).copied().unwrap_or(all[0])
}

#[derive(Serialize, Deserialize)]
struct WireSetup {
    spelling: bool,
    english: u8,
    known: Vec<String>,
    ignored: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct WireJob {
    key: u64,
    text: String,
    range: (usize, usize),
    pieces: Vec<(usize, usize, u8)>,
}

#[derive(Serialize, Deserialize)]
struct WireRequest {
    /// Left out when it's the setup sent last.
    setup: Option<WireSetup>,
    jobs: Vec<WireJob>,
}

#[derive(Serialize, Deserialize)]
struct WireFlag {
    start: usize,
    end: usize,
    kind: u8,
    rule: String,
    message: String,
    replacements: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct WireReply {
    checked: Vec<(u64, Vec<WireFlag>)>,
}

impl WireSetup {
    fn of(setup: &Setup) -> WireSetup {
        WireSetup {
            spelling: setup.options.spelling,
            english: index_of(&ENGLISH, &setup.options.english),
            known: setup.known.iter().cloned().collect(),
            ignored: setup.ignored.iter().cloned().collect(),
        }
    }

    fn into_setup(self) -> Setup {
        Setup {
            options: CheckOptions {
                spelling: self.spelling,
                english: from_index(&ENGLISH, self.english),
            },
            known: Arc::new(self.known.into_iter().collect::<HashSet<_>>()),
            ignored: Arc::new(self.ignored.into_iter().collect::<HashSet<_>>()),
        }
    }
}

impl WireJob {
    fn of(job: &Job) -> WireJob {
        WireJob {
            key: job.key,
            text: job.text.clone(),
            range: (job.unit.range.start, job.unit.range.end),
            pieces: job
                .unit
                .pieces
                .iter()
                .map(|piece| {
                    let kind = index_of(&PIECE_KINDS, &piece.kind);
                    (piece.range.start, piece.range.end, kind)
                })
                .collect(),
        }
    }

    fn into_job(self) -> Job {
        let pieces = self
            .pieces
            .into_iter()
            .map(|(start, end, kind)| Piece::new(start..end, from_index(&PIECE_KINDS, kind)))
            .collect();
        Job {
            key: self.key,
            text: self.text,
            unit: Unit {
                range: self.range.0..self.range.1,
                pieces,
            },
        }
    }
}

impl WireFlag {
    fn of(flag: Flag) -> WireFlag {
        WireFlag {
            start: flag.range.start,
            end: flag.range.end,
            kind: index_of(&FLAG_KINDS, &flag.kind),
            rule: flag.rule,
            message: flag.message,
            replacements: flag.replacements,
        }
    }

    fn into_flag(self) -> Flag {
        Flag {
            range: self.start..self.end,
            kind: from_index(&FLAG_KINDS, self.kind),
            rule: self.rule,
            message: self.message,
            replacements: self.replacements,
        }
    }
}

/// What tells two setups apart without comparing every word: the options,
/// and which word lists they share.
#[derive(Clone, PartialEq)]
struct SetupKey {
    options: CheckOptions,
    known: usize,
    ignored: usize,
}

impl SetupKey {
    fn of(setup: &Setup) -> SetupKey {
        SetupKey {
            options: setup.options.clone(),
            known: Arc::as_ptr(&setup.known) as usize,
            ignored: Arc::as_ptr(&setup.ignored) as usize,
        }
    }
}

/// The child process, as the app's worker thread talks to it.
pub(super) struct ChildChecker {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    /// The setup the child has, which later requests leave out.
    sent: Option<SetupKey>,
}

impl ChildChecker {
    pub(super) fn start(exe: &Path) -> io::Result<ChildChecker> {
        let mut command = Command::new(exe);
        command
            .arg(WORKER_ARG)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        for name in QUIET_ENV {
            command.env_remove(name);
        }
        let mut child = command.spawn()?;
        let (Some(input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
            return Err(io::Error::other("the grammar worker has no pipes"));
        };
        Ok(ChildChecker {
            child,
            input: Some(input),
            output: BufReader::new(output),
            sent: None,
        })
    }

    /// Checks `jobs` with `setup` in the child, waiting for its answer.
    pub(super) fn check(&mut self, setup: &Setup, jobs: &[Job]) -> io::Result<Checked> {
        let key = SetupKey::of(setup);
        let request = WireRequest {
            setup: (self.sent.as_ref() != Some(&key)).then(|| WireSetup::of(setup)),
            jobs: jobs.iter().map(WireJob::of).collect(),
        };
        let input = self
            .input
            .as_mut()
            .ok_or_else(|| io::Error::other("the grammar worker's input is closed"))?;
        write_line(input, &request)?;
        self.sent = Some(key);
        let reply: WireReply = read_line(&mut self.output)?
            .ok_or_else(|| io::Error::other("the grammar worker stopped"))?;
        Ok(reply
            .checked
            .into_iter()
            .map(|(key, flags)| (key, flags.into_iter().map(WireFlag::into_flag).collect()))
            .collect())
    }
}

impl Drop for ChildChecker {
    fn drop(&mut self) {
        // Closing its input is how the child learns to finish.
        drop(self.input.take());
        self.child.wait().ok();
    }
}

fn write_line<T: Serialize>(to: &mut impl Write, value: &T) -> io::Result<()> {
    serde_json::to_writer(&mut *to, value).map_err(io::Error::other)?;
    to.write_all(b"\n")?;
    to.flush()
}

/// The next JSON line, or `None` at the end of the input.
fn read_line<T: for<'de> Deserialize<'de>>(from: &mut impl BufRead) -> io::Result<Option<T>> {
    let mut line = String::new();
    if from.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    serde_json::from_str(&line)
        .map(Some)
        .map_err(io::Error::other)
}

/// `gasp grammar-worker`: answers requests from `input` on `output` until
/// the input closes.
pub fn serve(input: impl BufRead, output: impl Write) -> io::Result<()> {
    let (mut input, mut output) = (input, output);
    let mut checker = InProcess::default();
    let mut setup = Setup::default();
    while let Some(request) = read_line::<WireRequest>(&mut input)? {
        if let Some(sent) = request.setup {
            setup = sent.into_setup();
        }
        let jobs: Vec<Job> = request.jobs.into_iter().map(WireJob::into_job).collect();
        let checked = checker.check(setup.clone(), jobs);
        let reply = WireReply {
            checked: checked
                .into_iter()
                .map(|(key, flags)| (key, flags.into_iter().map(WireFlag::of).collect()))
                .collect(),
        };
        match write_line(&mut output, &reply) {
            // The app quit while this was checking.
            Err(error) if error.kind() == io::ErrorKind::BrokenPipe => return Ok(()),
            written => written?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(key: u64, text: &str) -> Job {
        Job {
            key,
            text: text.to_owned(),
            unit: Unit {
                range: 0..text.len(),
                pieces: vec![Piece::new(0..text.len(), PieceKind::Text)],
            },
        }
    }

    #[test]
    fn jobs_and_flags_survive_the_trip() {
        let original = job(7, "one `code` two");
        let back = WireJob::of(&original).into_job();
        assert_eq!(back.key, original.key);
        assert_eq!(back.text, original.text);
        assert_eq!(back.unit, original.unit);
        let flag = Flag {
            range: 3..8,
            kind: FlagKind::Mechanical,
            rule: "Spaces".to_owned(),
            message: "Two spaces".to_owned(),
            replacements: vec![" ".to_owned()],
        };
        assert_eq!(WireFlag::of(flag.clone()).into_flag(), flag);
        let setup = Setup {
            options: CheckOptions {
                spelling: true,
                english: English::British,
            },
            known: Arc::new(HashSet::from(["gasp".to_owned()])),
            ignored: Arc::new(HashSet::new()),
        };
        let back = WireSetup::of(&setup).into_setup();
        assert_eq!(back.options, setup.options);
        assert_eq!(back.known, setup.known);
    }

    #[test]
    fn a_served_check_matches_one_on_the_thread() {
        let setup = Setup {
            options: CheckOptions {
                spelling: true,
                english: English::American,
            },
            ..Setup::default()
        };
        let jobs = vec![job(1, "This is  a a test of teh checker."), job(2, "Fine.")];
        let mut requests = Vec::new();
        let request = WireRequest {
            setup: Some(WireSetup::of(&setup)),
            jobs: jobs.iter().map(WireJob::of).collect(),
        };
        write_line(&mut requests, &request).unwrap();
        let mut replies = Vec::new();
        serve(requests.as_slice(), &mut replies).unwrap();
        let reply: WireReply = read_line(&mut replies.as_slice()).unwrap().unwrap();
        let served: Checked = reply
            .checked
            .into_iter()
            .map(|(key, flags)| (key, flags.into_iter().map(WireFlag::into_flag).collect()))
            .collect();
        let here = InProcess::default().check(setup, jobs);
        assert_eq!(served, here);
        assert!(!served[0].1.is_empty(), "the first paragraph has mistakes");
    }
}
