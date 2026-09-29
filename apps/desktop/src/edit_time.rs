//! Edit-time tracking (PLAN.md): how long each note has been edited. It
//! replaces Chronotyper without touching frontmatter. Each device keeps
//! its own file under `.gasp/stats/`, named by an id kept in its
//! `device.toml`, so two devices never write the same file and sync never
//! sees a conflict; the app sums every device's file. A note's
//! `edited_seconds` frontmatter from Chronotyper is its starting value,
//! read from the note itself (the migration that moves it out isn't run
//! here, so it's never counted twice).
//!
//! Time counts while typing: the gap since the last edit to the same note
//! counts when it's under [`IDLE_GAP`], so pausing to think counts and
//! walking away doesn't. Files are read and written off the main thread.

use std::collections::{BTreeMap, HashMap};
use std::hash::{BuildHasher, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Longer pauses than this between edits aren't editing.
pub const IDLE_GAP: Duration = Duration::from_secs(60);

/// Where each device's file lives, from the vault root.
pub const STATS_DIR: &str = concat!(gasp_config::config_dir!(), "/stats");

/// How long after an edit the device's file is written.
pub const SAVE_DELAY: Duration = Duration::from_secs(5);

/// One device's file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct StatsFile {
    /// The device's name, for people reading the folder.
    pub device: String,
    /// Seconds spent editing each note, by vault-relative path.
    pub edited_seconds: BTreeMap<String, u64>,
}

/// Every device's files, as read from disk.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoadedStats {
    pub own: StatsFile,
    /// Other devices' seconds, summed per note.
    pub others: HashMap<String, u64>,
}

/// The edit times in memory: this device's own, and the other devices'.
#[derive(Debug)]
pub struct EditTime {
    device_id: String,
    device_name: String,
    own: BTreeMap<String, f64>,
    others: HashMap<String, u64>,
    /// The note last edited here, and when.
    last_edit: Option<(String, Instant)>,
    dirty: bool,
}

impl EditTime {
    pub fn new(device_id: &str, device_name: &str) -> EditTime {
        EditTime {
            device_id: device_id.to_owned(),
            device_name: device_name.to_owned(),
            own: BTreeMap::new(),
            others: HashMap::new(),
            last_edit: None,
            dirty: false,
        }
    }

    /// The id that names this device's file.
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// Names the device in its file, for people reading the folder.
    pub fn set_device_name(&mut self, name: &str) {
        name.clone_into(&mut self.device_name);
    }

    /// The file this device writes, from the vault root.
    pub fn own_file(&self, vault: &Path) -> PathBuf {
        own_file(vault, &self.device_id)
    }

    /// Adds what was read from disk to what was counted since.
    pub fn merge_loaded(&mut self, loaded: LoadedStats) {
        for (note, seconds) in loaded.own.edited_seconds {
            *self.own.entry(note).or_default() += seconds as f64;
        }
        self.others = loaded.others;
    }

    /// Counts an edit to `note` at `now`. Says whether time was added.
    pub fn record_edit(&mut self, note: &str, now: Instant) -> bool {
        let gap = match &self.last_edit {
            Some((last, at)) if last == note => now.saturating_duration_since(*at),
            _ => Duration::MAX,
        };
        self.last_edit = Some((note.to_owned(), now));
        if gap > IDLE_GAP || gap.is_zero() {
            return false;
        }
        *self.own.entry(note.to_owned()).or_default() += gap.as_secs_f64();
        self.dirty = true;
        true
    }

    /// Whole seconds spent editing `note` on every device.
    pub fn seconds(&self, note: &str) -> u64 {
        let own = self.own.get(note).copied().unwrap_or(0.) as u64;
        own + self.others.get(note).copied().unwrap_or(0)
    }

    /// Follows a rename: the time spent on `from`, here and elsewhere,
    /// is this device's under `to` from now on.
    pub fn rename(&mut self, from: &str, to: &str) {
        let own = self.own.remove(from).unwrap_or(0.);
        let others = self.others.remove(from).unwrap_or(0);
        let moved = own + others as f64;
        if moved > 0. {
            *self.own.entry(to.to_owned()).or_default() += moved;
            self.dirty = true;
        }
        if let Some((last, _)) = &mut self.last_edit
            && last == from
        {
            *last = to.to_owned();
        }
    }

    /// Whether there are counts not yet written.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// This device's file as it should be now; clears the dirty mark.
    pub fn take_file(&mut self) -> StatsFile {
        self.dirty = false;
        StatsFile {
            device: self.device_name.clone(),
            edited_seconds: self
                .own
                .iter()
                .map(|(note, seconds)| (note.clone(), *seconds as u64))
                .filter(|(_, seconds)| *seconds > 0)
                .collect(),
        }
    }
}

fn own_file(vault: &Path, device_id: &str) -> PathBuf {
    vault.join(STATS_DIR).join(format!("{device_id}.json"))
}

/// Reads every device's file. A missing folder is no stats yet; a file
/// that doesn't parse is skipped rather than stopping the rest.
pub fn load(vault: &Path, device_id: &str) -> LoadedStats {
    let mut loaded = LoadedStats::default();
    let Ok(entries) = std::fs::read_dir(vault.join(STATS_DIR)) else {
        return loaded;
    };
    let own = own_file(vault, device_id);
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let Some(file) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<StatsFile>(&text).ok())
        else {
            continue;
        };
        if path == own {
            loaded.own = file;
            continue;
        }
        for (note, seconds) in file.edited_seconds {
            *loaded.others.entry(note).or_default() += seconds;
        }
    }
    loaded
}

/// Writes this device's file, through a temporary file so a crash never
/// leaves half of one.
pub fn save(path: &Path, file: &StatsFile) -> io::Result<()> {
    if !crate::sandbox::writes_allowed() {
        return Ok(());
    }
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let text = serde_json::to_string_pretty(file).map_err(io::Error::other)?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, text + "\n")?;
    std::fs::rename(&temporary, path)
}

/// A new id for this device's file: its name, for people, and a random
/// part, so two machines with one name still get their own files.
pub fn new_device_id(device_name: &str) -> String {
    let slug: String = device_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "device" } else { slug };
    let random = std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish();
    format!("{slug}-{:06x}", random & 0xff_ffff)
}

/// This computer's short name: the one Windows or the shell gives, or
/// else the machine's hostname, up to its first dot.
pub fn device_name() -> String {
    ["COMPUTERNAME", "HOSTNAME"]
        .iter()
        .find_map(|name| std::env::var(name).ok())
        .or_else(hostname)
        .and_then(|name| short_name(&name))
        .unwrap_or_else(|| "this device".to_owned())
}

fn short_name(hostname: &str) -> Option<String> {
    let short = hostname.trim().split('.').next()?.trim();
    (!short.is_empty()).then(|| short.to_owned())
}

#[cfg(unix)]
fn hostname() -> Option<String> {
    let uname = rustix::system::uname();
    Some(uname.nodename().to_string_lossy().into_owned())
}

#[cfg(not(unix))]
fn hostname() -> Option<String> {
    None
}

/// `edited_seconds` from a note's frontmatter, as Chronotyper wrote it.
/// Only the note's start is read: frontmatter is at the top.
pub fn frontmatter_seconds(text: &str) -> Option<u64> {
    let mut lines = text.lines();
    if lines.next()?.trim_end() != "---" {
        return None;
    }
    for line in lines {
        if line.trim_end() == "---" {
            return None;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if key.trim() == "edited_seconds" {
            let value = value.trim().trim_matches(['"', '\'']);
            return value.parse::<f64>().ok().map(|seconds| seconds as u64);
        }
    }
    None
}

/// How long a note has been edited, as the status bar says it: "12 min
/// editing" or "3 h 5 min editing", rounded up to the minute, and nothing
/// for a note never edited.
pub fn edit_time_label(seconds: u64) -> Option<String> {
    let minutes = seconds.div_ceil(60);
    let (hours, minutes) = (minutes / 60, minutes % 60);
    match (hours, minutes) {
        (0, 0) => None,
        (0, minutes) => Some(format!("{minutes} min editing")),
        (hours, 0) => Some(format!("{hours} h editing")),
        (hours, minutes) => Some(format!("{hours} h {minutes} min editing")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_counts_and_walking_away_does_not() {
        let mut time = EditTime::new("laptop-1", "Laptop");
        let start = Instant::now();
        assert!(
            !time.record_edit("a.md", start),
            "the first edit starts a run"
        );
        assert!(time.record_edit("a.md", start + Duration::from_secs(10)));
        assert!(time.record_edit("a.md", start + Duration::from_secs(40)));
        assert_eq!(time.seconds("a.md"), 40);
        // Ten minutes away doesn't count; the edit after it starts again.
        assert!(!time.record_edit("a.md", start + Duration::from_secs(640)));
        assert!(time.record_edit("a.md", start + Duration::from_secs(645)));
        assert_eq!(time.seconds("a.md"), 45);
        // Switching notes doesn't hand one note's pause to the other.
        assert!(!time.record_edit("b.md", start + Duration::from_secs(650)));
        assert_eq!(time.seconds("b.md"), 0);
        assert!(time.is_dirty());
        assert_eq!(time.take_file().edited_seconds["a.md"], 45);
        assert!(!time.is_dirty());
    }

    #[test]
    fn devices_write_their_own_files_and_the_app_sums_them() {
        let vault = tempfile::tempdir().unwrap();
        let other = StatsFile {
            device: "Phone".into(),
            edited_seconds: BTreeMap::from([("a.md".to_owned(), 100)]),
        };
        save(&own_file(vault.path(), "phone-1"), &other).unwrap();
        let mut laptop = EditTime::new("laptop-1", "Laptop");
        let start = Instant::now();
        laptop.record_edit("a.md", start);
        laptop.record_edit("a.md", start + Duration::from_secs(30));
        laptop.merge_loaded(load(vault.path(), "laptop-1"));
        assert_eq!(laptop.seconds("a.md"), 130);
        save(&laptop.own_file(vault.path()), &laptop.take_file()).unwrap();
        // Reading back: this device's file is its own, not another's.
        let loaded = load(vault.path(), "laptop-1");
        assert_eq!(loaded.own.edited_seconds["a.md"], 30);
        assert_eq!(loaded.others["a.md"], 100);
        let names: Vec<String> = std::fs::read_dir(vault.path().join(STATS_DIR))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 2, "{names:?}");
    }

    #[test]
    fn renaming_carries_the_time() {
        let mut time = EditTime::new("laptop-1", "Laptop");
        time.merge_loaded(LoadedStats {
            own: StatsFile::default(),
            others: HashMap::from([("old.md".to_owned(), 60)]),
        });
        time.rename("old.md", "new.md");
        assert_eq!(time.seconds("new.md"), 60);
        assert_eq!(time.seconds("old.md"), 0);
    }

    #[test]
    fn chronotyper_frontmatter_is_read() {
        let note = "---\ntitle: x\nedited_seconds: 754\nupdated: 2024-01-01\n---\nBody\n";
        assert_eq!(frontmatter_seconds(note), Some(754));
        assert_eq!(
            frontmatter_seconds("---\nedited_seconds: \"12.7\"\n---\n"),
            Some(12)
        );
        assert_eq!(frontmatter_seconds("edited_seconds: 5\n"), None);
        assert_eq!(
            frontmatter_seconds("---\ntitle: x\n---\nedited_seconds: 5\n"),
            None
        );
    }

    #[test]
    fn labels_round_up_to_the_minute() {
        assert_eq!(edit_time_label(0), None);
        assert_eq!(edit_time_label(1).as_deref(), Some("1 min editing"));
        assert_eq!(edit_time_label(60 * 12).as_deref(), Some("12 min editing"));
        assert_eq!(edit_time_label(3600 * 2).as_deref(), Some("2 h editing"));
        assert_eq!(
            edit_time_label(3600 * 3 + 290).as_deref(),
            Some("3 h 5 min editing")
        );
    }

    #[test]
    fn device_names_are_short_hostnames() {
        assert_eq!(short_name("mac.local").as_deref(), Some("mac"));
        assert_eq!(
            short_name(" studio.lan.example.com\n").as_deref(),
            Some("studio")
        );
        assert_eq!(short_name("LAPTOP-7").as_deref(), Some("LAPTOP-7"));
        assert_eq!(short_name(".hidden"), None);
        assert!(!device_name().is_empty());
        assert!(!device_name().contains('.'));
    }

    #[test]
    fn device_ids_read_as_the_device() {
        let id = new_device_id("Boris's MacBook Pro");
        assert!(id.starts_with("boris-s-macbook-pro-"), "{id}");
        assert_ne!(new_device_id("x"), new_device_id("x"));
        assert!(new_device_id("··").starts_with("device-"));
    }
}
