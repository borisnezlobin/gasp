//! The temporary folder a whole-window snapshot works in: a copy of the
//! vault, and the app's own folders, both removed when the run ends.
//!
//! The copy leaves out `.git`, so nothing in it can sync, and folders
//! reached through a symbolic link, so nothing written in it can reach
//! the original. Files are copied with APFS clones where the disk allows,
//! so even a large vault copies quickly and takes no room.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Folders a copy leaves out.
const LEFT_OUT: [&str; 1] = [".git"];

pub struct ScratchFolder {
    root: PathBuf,
    vault: PathBuf,
    original_vault: PathBuf,
}

impl ScratchFolder {
    /// Copies `vault` into a new folder in the system's temporary folder.
    pub fn copy_vault(vault: &Path) -> Result<ScratchFolder, String> {
        let original_vault = fs::canonicalize(vault)
            .map_err(|error| format!("{} isn't a folder: {error}", vault.display()))?;
        if !original_vault.is_dir() {
            return Err(format!("{} isn't a folder", vault.display()));
        }
        // The system's temporary folder is behind a link on macOS; the
        // app compares notes' paths with the vault's real one.
        let root = unique_root();
        fs::create_dir_all(&root).map_err(|error| format!("could not make a folder: {error}"))?;
        let root = fs::canonicalize(&root).unwrap_or(root);
        let name = original_vault.file_name().unwrap_or("vault".as_ref());
        let copy = root.join("vault").join(name);
        let scratch = ScratchFolder {
            vault: copy.clone(),
            root,
            original_vault,
        };
        if let Err(error) = copy_folder(&scratch.original_vault, &copy) {
            scratch.remove();
            return Err(format!("could not copy the vault: {error}"));
        }
        Ok(scratch)
    }

    /// The copy of the vault.
    pub fn vault(&self) -> &Path {
        &self.vault
    }

    /// Where the app keeps its own folders during the run.
    pub fn data_root(&self) -> PathBuf {
        self.root.join("app")
    }

    /// The whole temporary folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `path` in the copy: a path relative to the vault, or one inside
    /// the original vault. Anything else stays out of reach.
    pub fn in_copy(&self, path: &Path) -> Result<PathBuf, String> {
        let outside = || format!("{} isn't in the vault", path.display());
        let relative = match path.is_relative() {
            true => path.to_path_buf(),
            false => {
                let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
                let inside = canonical.strip_prefix(&self.original_vault);
                inside.map_err(|_| outside())?.to_path_buf()
            }
        };
        let climbs = relative
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir));
        match climbs {
            true => Err(outside()),
            false => Ok(self.vault.join(relative)),
        }
    }

    /// Deletes the temporary folder.
    pub fn remove(&self) {
        if let Err(error) = fs::remove_dir_all(&self.root)
            && error.kind() != io::ErrorKind::NotFound
        {
            eprintln!("could not remove {}: {error}", self.root.display());
        }
    }
}

fn unique_root() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let name = format!(
        "{}-snapshot-{}-{nanos}",
        gasp_config::COMMAND_NAME,
        std::process::id()
    );
    std::env::temp_dir().join(name)
}

fn copy_folder(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if LEFT_OUT.iter().any(|left_out| name == *left_out) {
            continue;
        }
        let source = entry.path();
        let Ok(metadata) = fs::metadata(&source) else {
            continue;
        };
        let linked = entry.file_type()?.is_symlink();
        let target = to.join(&name);
        if !metadata.is_dir() {
            fs::copy(&source, &target)?;
        } else if !linked {
            copy_folder(&source, &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_the_vault_but_not_its_history() {
        let vault = tempfile::tempdir().unwrap();
        let notes = vault.path().join("My Vault");
        fs::create_dir_all(notes.join("Maths")).unwrap();
        fs::create_dir_all(notes.join(".git")).unwrap();
        fs::write(notes.join("Maths/Lemma.md"), "lemma").unwrap();
        fs::write(notes.join(".git/HEAD"), "ref").unwrap();
        let scratch = ScratchFolder::copy_vault(&notes).unwrap();
        assert!(scratch.vault().ends_with("My Vault"));
        let copied = scratch.vault().join("Maths/Lemma.md");
        assert_eq!(fs::read_to_string(&copied).unwrap(), "lemma");
        assert!(!scratch.vault().join(".git").exists());
        assert_eq!(
            scratch.in_copy(Path::new("Maths/Lemma.md")),
            Ok(copied.clone())
        );
        assert_eq!(scratch.in_copy(&notes.join("Maths/Lemma.md")), Ok(copied));
        assert!(scratch.in_copy(Path::new("../elsewhere.md")).is_err());
        assert!(scratch.in_copy(vault.path()).is_err());
        scratch.remove();
        assert!(!scratch.root().exists());
        assert!(notes.join("Maths/Lemma.md").exists());
    }
}
