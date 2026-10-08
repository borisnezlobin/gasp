//! Keeping a vault in iCloud Drive, the sync that needs nothing but an
//! Apple account. The vault is the Documents folder of Gasp's own iCloud
//! container, which iCloud Drive shows as a folder named Gasp on every
//! device; iCloud copies it between the person's devices and Gasp reads
//! and writes the files as it does any vault's. Before the container,
//! the vault was an ordinary `Gasp` folder at the top of iCloud Drive
//! ([`old_icloud_vault`]), which still counts as an iCloud vault.
//!
//! What iCloud does to the files that Gasp has to know about:
//!
//! - A file not downloaded yet is a hidden placeholder beside where it
//!   will be, `.Plan.md.icloud` for `Plan.md` ([`download_target`]).
//! - Two devices changing a note before iCloud caught up leaves a copy
//!   beside it, `Plan 2.md` for `Plan.md` ([`icloud_copies`]).
//!
//! [`move_vault`] brings a vault that lives elsewhere into iCloud Drive:
//! it copies every file, reads each copy back to check it matches, and
//! only then reports success, so the app can switch to the new folder.
//! The original is never touched.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The name iCloud Drive shows for the folder that holds the vault.
pub const ICLOUD_FOLDER_NAME: &str = "Gasp";

/// The folder every iCloud Drive item lives under, on a Mac and an iPhone.
const MOBILE_DOCUMENTS: &str = "Mobile Documents";
/// iCloud Drive's own container inside it.
const ICLOUD_DRIVE_CONTAINER: &str = "com~apple~CloudDocs";
/// Gasp's container inside it, `iCloud.com.borisnezlobin.gasp` as the
/// folder name spells it.
pub const GASP_CONTAINER: &str = "iCloud~com~borisnezlobin~gasp";
/// The container's folder that iCloud Drive shows under the app's name.
const CONTAINER_DOCUMENTS: &str = "Documents";

/// Where a placeholder's name starts and ends.
const PLACEHOLDER_SUFFIX: &str = ".icloud";

/// The folder holding iCloud Drive and every app's iCloud container on a
/// Mac, for the person whose home folder is `home`.
pub fn mobile_documents(home: &Path) -> PathBuf {
    home.join("Library").join(MOBILE_DOCUMENTS)
}

/// iCloud Drive's own folder inside `mobile_documents`, there whenever
/// iCloud Drive is on.
pub fn icloud_drive(mobile_documents: &Path) -> PathBuf {
    mobile_documents.join(ICLOUD_DRIVE_CONTAINER)
}

/// Where the vault goes: the Documents folder of Gasp's container inside
/// `mobile_documents`, which iCloud Drive shows as Gasp.
pub fn icloud_vault(mobile_documents: &Path) -> PathBuf {
    mobile_documents
        .join(GASP_CONTAINER)
        .join(CONTAINER_DOCUMENTS)
}

/// Where the vault went before Gasp had a container: a folder called Gasp
/// at the top of iCloud Drive.
pub fn old_icloud_vault(mobile_documents: &Path) -> PathBuf {
    icloud_drive(mobile_documents).join(ICLOUD_FOLDER_NAME)
}

/// Whether `path` is the folder in Gasp's container that holds the vault.
pub fn is_gasp_container(path: &Path) -> bool {
    path.ends_with(Path::new(GASP_CONTAINER).join(CONTAINER_DOCUMENTS))
}

/// A vault folder's name as people see it: Gasp for the container's
/// Documents folder, as iCloud Drive shows it, and the folder's own name
/// otherwise.
pub fn shown_folder_name(path: &Path) -> String {
    if is_gasp_container(path) {
        return ICLOUD_FOLDER_NAME.to_owned();
    }
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Whether `path` is somewhere iCloud keeps in step: iCloud Drive, or an
/// app's iCloud container.
pub fn is_in_icloud(path: &Path) -> bool {
    gasp_config::device_file::is_in_icloud(path)
}

/// Whether the vault at `root` syncs with git, which it then keeps doing:
/// iCloud isn't offered for it.
pub fn is_git_clone(root: &Path) -> bool {
    root.join(".git").exists()
}

/// The file a placeholder stands for: `Plan.md` for `.Plan.md.icloud`.
pub fn download_target(file_name: &str) -> Option<&str> {
    let target = file_name
        .strip_prefix('.')?
        .strip_suffix(PLACEHOLDER_SUFFIX)?;
    (!target.is_empty()).then_some(target)
}

/// Files in the vault at `root` that iCloud hasn't downloaded yet, by the
/// vault-relative path each will have.
pub fn waiting_downloads(root: &Path) -> Vec<PathBuf> {
    let mut waiting = Vec::new();
    walk_visible(root, Path::new(""), &mut |relative, name| {
        if let Some(target) = download_target(name) {
            waiting.push(relative.with_file_name(target));
        }
    });
    waiting.sort();
    waiting
}

/// A note iCloud found changed on two devices at once, and the copy it
/// kept of the other version beside it. Both vault-relative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ICloudCopy {
    pub original: PathBuf,
    pub copy: PathBuf,
}

/// The note a copy's name points back to: `Plan.md` for `Plan 2.md`.
pub fn copied_from(file_name: &str) -> Option<String> {
    let stem = file_name.strip_suffix(".md")?;
    let (base, number) = stem.rsplit_once(' ')?;
    let is_copy_number = number.parse::<u32>().is_ok_and(|number| number >= 2)
        && number.chars().all(|c| c.is_ascii_digit());
    (is_copy_number && !base.is_empty()).then(|| format!("{base}.md"))
}

/// The copies iCloud has left beside notes in the vault at `root`.
///
/// A note called `Chapter 2` beside `Chapter` is as likely to be a note
/// of its own, so a copy only counts when at least half its lines are in
/// the note it's named after.
pub fn icloud_copies(root: &Path) -> Vec<ICloudCopy> {
    let mut copies = Vec::new();
    walk_visible(root, Path::new(""), &mut |relative, name| {
        let Some(original_name) = copied_from(name) else {
            return;
        };
        let original = relative.with_file_name(original_name);
        if reads_like_a_copy(&root.join(&original), &root.join(relative)) {
            copies.push(ICloudCopy {
                original,
                copy: relative.to_path_buf(),
            });
        }
    });
    copies.sort_by(|a, b| a.copy.cmp(&b.copy));
    copies
}

fn reads_like_a_copy(original: &Path, copy: &Path) -> bool {
    let (Ok(original), Ok(copy)) = (fs::read_to_string(original), fs::read_to_string(copy)) else {
        return false;
    };
    let shared: HashSet<&str> = original.lines().map(str::trim).collect();
    let lines: HashSet<&str> = copy
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return original.trim().is_empty();
    }
    let found = lines.iter().filter(|line| shared.contains(*line)).count();
    found * 2 >= lines.len()
}

/// Calls `visit` with each file's vault-relative path and name under
/// `folder`, leaving out hidden folders such as `.gasp` and `.git` but
/// not hidden files, which placeholders are.
fn walk_visible(root: &Path, folder: &Path, visit: &mut dyn FnMut(&Path, &str)) {
    let Ok(entries) = fs::read_dir(root.join(folder)) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let relative = folder.join(&name);
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() && !name.starts_with('.') {
            walk_visible(root, &relative, visit);
        } else if kind.is_file() {
            visit(&relative, &name);
        }
    }
}

// ---- Moving a vault in ----

/// What moving a vault into iCloud Drive did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MoveReport {
    /// Files copied in, vault-relative.
    pub copied: Vec<PathBuf>,
    /// Files the iCloud folder already had, byte for byte.
    pub already_there: Vec<PathBuf>,
    /// Files the iCloud folder had with other contents, so this vault's
    /// copy went beside it: (the file, where this vault's version went).
    pub kept_beside: Vec<(PathBuf, PathBuf)>,
    /// Folders behind symbolic links, which weren't followed.
    pub left_out: Vec<PathBuf>,
}

impl MoveReport {
    /// How many notes (Markdown files) came across, copied or already there.
    pub fn notes(&self) -> usize {
        let beside = self.kept_beside.iter().map(|(_, beside)| beside);
        self.copied
            .iter()
            .chain(&self.already_there)
            .chain(beside)
            .filter(|path| is_note(path))
            .count()
    }
}

fn is_note(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "md")
}

/// Why a vault couldn't move into iCloud Drive. Nothing in the vault
/// changed in any case.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MoveError {
    #[error("This vault syncs with git, so it stays where it is.")]
    SyncsWithGit,
    #[error("This vault is in iCloud already.")]
    AlreadyThere,
    #[error("The iCloud folder can’t be inside the vault, or the vault inside it.")]
    Nested,
    #[error("There’s no vault at {0}.")]
    NoVault(PathBuf),
    #[error("{} didn’t copy exactly, so nothing moved. Try again.", .0.display())]
    Mismatch(PathBuf),
    #[error("Copying stopped at {}: {message}. Nothing moved.", .path.display())]
    Io { path: PathBuf, message: String },
}

/// Copies the vault at `from` into `to` (made if missing), reads every
/// copy back to check it, and reports what it did. Files `to` already
/// has are left alone: one with the same bytes counts as copied, one that
/// differs keeps its place and this vault's version goes beside it,
/// named after `device`. If anything fails, every file this made is
/// removed again. `from` is only ever read.
pub fn move_vault(from: &Path, to: &Path, device: &str) -> Result<MoveReport, MoveError> {
    check_move(from, to)?;
    let mut copier = Copier::new(from, to, device);
    let result = copier.copy_all().and_then(|()| copier.verify());
    if result.is_err() {
        copier.undo();
    }
    result.map(|()| copier.report)
}

fn check_move(from: &Path, to: &Path) -> Result<(), MoveError> {
    if !from.is_dir() {
        return Err(MoveError::NoVault(from.to_path_buf()));
    }
    if is_git_clone(from) {
        return Err(MoveError::SyncsWithGit);
    }
    let from = from
        .canonicalize()
        .map_err(|error| io_error(from, &error))?;
    let to = canonical_or_itself(to);
    if from == to {
        return Err(MoveError::AlreadyThere);
    }
    if to.starts_with(&from) || from.starts_with(&to) {
        return Err(MoveError::Nested);
    }
    Ok(())
}

/// `path` with its links resolved, or as it is when it doesn't exist yet.
fn canonical_or_itself(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => canonical_or_itself(parent).join(name),
        _ => path.to_path_buf(),
    }
}

fn io_error(path: &Path, error: &io::Error) -> MoveError {
    MoveError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

/// Folders and files a move never copies: git's own, macOS's folder
/// settings, and iCloud placeholders.
fn left_behind(name: &str) -> bool {
    matches!(name, ".git" | ".git-setup" | ".DS_Store") || download_target(name).is_some()
}

struct Copier<'a> {
    from: &'a Path,
    to: &'a Path,
    device: &'a str,
    report: MoveReport,
    /// Every file and folder this made, in the order it made them.
    made: Vec<PathBuf>,
}

impl<'a> Copier<'a> {
    fn new(from: &'a Path, to: &'a Path, device: &'a str) -> Self {
        Copier {
            from,
            to,
            device,
            report: MoveReport::default(),
            made: Vec::new(),
        }
    }

    fn copy_all(&mut self) -> Result<(), MoveError> {
        self.make_folder(Path::new(""))?;
        self.copy_folder(Path::new(""))
    }

    fn copy_folder(&mut self, folder: &Path) -> Result<(), MoveError> {
        let source = self.from.join(folder);
        let entries = fs::read_dir(&source).map_err(|error| io_error(&source, &error))?;
        let mut names: Vec<(String, fs::FileType)> = entries
            .flatten()
            .filter_map(|entry| {
                Some((
                    entry.file_name().into_string().ok()?,
                    entry.file_type().ok()?,
                ))
            })
            .filter(|(name, _)| !left_behind(name))
            .collect();
        names.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, kind) in names {
            self.copy_entry(&folder.join(name), kind)?;
        }
        Ok(())
    }

    fn copy_entry(&mut self, relative: &Path, kind: fs::FileType) -> Result<(), MoveError> {
        if kind.is_symlink() && self.from.join(relative).is_dir() {
            self.report.left_out.push(relative.to_path_buf());
            return Ok(());
        }
        if kind.is_dir() {
            self.make_folder(relative)?;
            return self.copy_folder(relative);
        }
        self.copy_file(relative)
    }

    fn make_folder(&mut self, relative: &Path) -> Result<(), MoveError> {
        let folder = self.to.join(relative);
        if folder.is_dir() {
            return Ok(());
        }
        fs::create_dir_all(&folder).map_err(|error| io_error(&folder, &error))?;
        self.made.push(folder);
        Ok(())
    }

    fn copy_file(&mut self, relative: &Path) -> Result<(), MoveError> {
        let source = self.from.join(relative);
        let bytes = fs::read(&source).map_err(|error| io_error(&source, &error))?;
        let target = self.to.join(relative);
        match fs::read(&target) {
            Ok(existing) if existing == bytes => {
                self.report.already_there.push(relative.to_path_buf());
                Ok(())
            }
            Ok(_) => {
                let beside = self.beside(relative);
                self.write_new(&beside, &bytes)?;
                self.report
                    .kept_beside
                    .push((relative.to_path_buf(), beside));
                Ok(())
            }
            Err(_) => {
                self.write_new(relative, &bytes)?;
                self.report.copied.push(relative.to_path_buf());
                Ok(())
            }
        }
    }

    /// `Plan (from mac).md` beside `Plan.md`, or `Plan (from mac 2).md`
    /// when that's taken.
    fn beside(&self, relative: &Path) -> PathBuf {
        let stem = relative
            .file_stem()
            .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
        let extension = relative
            .extension()
            .map_or_else(String::new, |ext| format!(".{}", ext.to_string_lossy()));
        let named =
            |suffix: String| relative.with_file_name(format!("{stem} ({suffix}){extension}"));
        let first = named(format!("from {}", self.device));
        std::iter::once(first)
            .chain((2..).map(|number| named(format!("from {} {number}", self.device))))
            .find(|candidate| !self.to.join(candidate).exists())
            .unwrap_or_else(|| relative.to_path_buf())
    }

    /// Writes a file that doesn't exist yet, through a temporary name so a
    /// half-written file is never seen under the real one.
    fn write_new(&mut self, relative: &Path, bytes: &[u8]) -> Result<(), MoveError> {
        let target = self.to.join(relative);
        let partial = target.with_file_name(format!(
            ".{}.gasp-copy",
            relative.file_name().unwrap_or_default().to_string_lossy()
        ));
        let written = fs::write(&partial, bytes).and_then(|()| fs::rename(&partial, &target));
        if let Err(error) = written {
            let _ = fs::remove_file(&partial);
            return Err(io_error(&target, &error));
        }
        self.made.push(target);
        Ok(())
    }

    /// Reads every copy back and compares it with its original.
    fn verify(&self) -> Result<(), MoveError> {
        let beside = self.report.kept_beside.iter();
        let copies = self
            .report
            .copied
            .iter()
            .map(|path| (path, path))
            .chain(beside.map(|(original, beside)| (original, beside)));
        for (original, copy) in copies {
            let source = fs::read(self.from.join(original));
            let written = fs::read(self.to.join(copy));
            match (source, written) {
                (Ok(source), Ok(written)) if source == written => {}
                _ => return Err(MoveError::Mismatch(original.clone())),
            }
        }
        Ok(())
    }

    /// Removes what this made, newest first, so folders are empty by the
    /// time they go.
    fn undo(&mut self) {
        for path in self.made.drain(..).rev() {
            if path.is_dir() {
                let _ = fs::remove_dir(&path);
            } else {
                let _ = fs::remove_file(&path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, relative: &str, text: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn read(root: &Path, relative: &str) -> String {
        fs::read_to_string(root.join(relative)).unwrap()
    }

    #[test]
    fn knows_where_icloud_drive_and_the_vault_are() {
        let mobile = mobile_documents(Path::new("/Users/you"));
        assert_eq!(
            icloud_drive(&mobile),
            Path::new("/Users/you/Library/Mobile Documents/com~apple~CloudDocs")
        );
        let vault = icloud_vault(&mobile);
        assert_eq!(
            vault,
            Path::new(
                "/Users/you/Library/Mobile Documents/iCloud~com~borisnezlobin~gasp/Documents"
            )
        );
        assert!(is_in_icloud(&vault));
        assert!(is_in_icloud(&old_icloud_vault(&mobile)));
        assert!(is_in_icloud(Path::new(
            "/private/var/mobile/Library/Mobile Documents/com~apple~CloudDocs/Gasp"
        )));
        assert!(!is_in_icloud(Path::new("/Users/you/Documents/Notes")));
    }

    #[test]
    fn the_container_folder_shows_as_gasp() {
        let mobile = mobile_documents(Path::new("/Users/you"));
        assert!(is_gasp_container(&icloud_vault(&mobile)));
        assert!(!is_gasp_container(&old_icloud_vault(&mobile)));
        assert_eq!(shown_folder_name(&icloud_vault(&mobile)), "Gasp");
        assert_eq!(shown_folder_name(Path::new("/Users/you/Notes")), "Notes");
    }

    #[test]
    fn placeholders_name_the_file_they_stand_for() {
        assert_eq!(download_target(".Plan.md.icloud"), Some("Plan.md"));
        assert_eq!(download_target(".icloud"), None);
        assert_eq!(download_target("Plan.md"), None);

        let vault = tempfile::tempdir().unwrap();
        write(vault.path(), "Daily/.Monday.md.icloud", "");
        write(vault.path(), "Here.md", "text");
        assert_eq!(
            waiting_downloads(vault.path()),
            [PathBuf::from("Daily/Monday.md")]
        );
    }

    #[test]
    fn a_numbered_note_is_a_copy_only_when_it_reads_like_the_note() {
        assert_eq!(copied_from("Plan 2.md").as_deref(), Some("Plan.md"));
        assert_eq!(copied_from("Plan 1.md"), None);
        assert_eq!(copied_from("Plan.md"), None);
        assert_eq!(copied_from("2.md"), None);

        let vault = tempfile::tempdir().unwrap();
        write(vault.path(), "Plan.md", "# Plan\n\nBuy milk\nCall Sam\n");
        write(vault.path(), "Plan 2.md", "# Plan\n\nBuy milk\nCall Alex\n");
        write(vault.path(), "Chapter.md", "Once upon a time\n");
        write(vault.path(), "Chapter 2.md", "The sequel begins\n");
        assert_eq!(
            icloud_copies(vault.path()),
            [ICloudCopy {
                original: PathBuf::from("Plan.md"),
                copy: PathBuf::from("Plan 2.md"),
            }]
        );
    }

    #[test]
    fn moving_copies_every_file_and_leaves_the_original() {
        let from = tempfile::tempdir().unwrap();
        let drive = tempfile::tempdir().unwrap();
        write(from.path(), "Plan.md", "# Plan\n");
        write(from.path(), "Daily/2026-09-30.md", "Wrote the move\n");
        write(from.path(), ".gasp/settings.toml", "[sync]\n");
        write(from.path(), "images/whale.png", "\u{89}PNG");
        write(from.path(), ".DS_Store", "finder");
        let to = icloud_vault(drive.path());

        let report = move_vault(from.path(), &to, "mac").unwrap();

        assert_eq!(report.copied.len(), 4);
        assert_eq!(report.notes(), 2);
        assert_eq!(read(&to, "Daily/2026-09-30.md"), "Wrote the move\n");
        assert_eq!(read(&to, ".gasp/settings.toml"), "[sync]\n");
        assert!(!to.join(".DS_Store").exists());
        assert_eq!(read(from.path(), "Plan.md"), "# Plan\n");
    }

    #[test]
    fn moving_into_a_folder_with_notes_keeps_both_versions() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        write(from.path(), "Same.md", "same\n");
        write(from.path(), "Plan.md", "this Mac's plan\n");
        write(to.path(), "Same.md", "same\n");
        write(to.path(), "Plan.md", "the iPhone's plan\n");
        write(to.path(), "Plan (from mac).md", "an older one\n");

        let report = move_vault(from.path(), to.path(), "mac").unwrap();

        assert_eq!(report.already_there, [PathBuf::from("Same.md")]);
        assert_eq!(
            report.kept_beside,
            [(
                PathBuf::from("Plan.md"),
                PathBuf::from("Plan (from mac 2).md")
            )]
        );
        assert_eq!(read(to.path(), "Plan.md"), "the iPhone's plan\n");
        assert_eq!(read(to.path(), "Plan (from mac 2).md"), "this Mac's plan\n");
        assert_eq!(read(to.path(), "Plan (from mac).md"), "an older one\n");
    }

    #[test]
    fn a_vault_that_syncs_with_git_stays_on_git() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        write(from.path(), ".git/HEAD", "ref: refs/heads/master\n");
        write(from.path(), "Plan.md", "# Plan\n");
        assert_eq!(
            move_vault(from.path(), &to.path().join("Gasp"), "mac"),
            Err(MoveError::SyncsWithGit)
        );
        assert!(!to.path().join("Gasp").exists());
    }

    #[test]
    fn a_vault_cannot_move_into_itself() {
        let from = tempfile::tempdir().unwrap();
        write(from.path(), "Plan.md", "# Plan\n");
        assert_eq!(
            move_vault(from.path(), &from.path().join("Gasp"), "mac"),
            Err(MoveError::Nested)
        );
        assert_eq!(
            move_vault(from.path(), from.path(), "mac"),
            Err(MoveError::AlreadyThere)
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_copy_removes_everything_it_made() {
        use std::os::unix::fs::PermissionsExt;
        let from = tempfile::tempdir().unwrap();
        let drive = tempfile::tempdir().unwrap();
        write(from.path(), "A.md", "first\n");
        write(from.path(), "B.md", "unreadable\n");
        let unreadable = from.path().join("B.md");
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read(&unreadable).is_ok() {
            // Running as root, which reads anything: nothing to test.
            return;
        }
        let to = drive.path().join("Gasp");

        let result = move_vault(from.path(), &to, "mac");

        assert!(matches!(result, Err(MoveError::Io { .. })));
        assert!(!to.exists());
        assert_eq!(read(from.path(), "A.md"), "first\n");
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o644)).unwrap();
    }
}
