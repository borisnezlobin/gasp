#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use editor_sync::{Author, MergeOutcome, Token, Vault, VaultConfig};
use git2::{Repository, RepositoryInitOptions};
use tempfile::TempDir;

/// A temp dir holding a bare "GitHub" repo plus any number of device clones.
pub struct World {
    pub dir: TempDir,
    pub remote_url: String,
}

impl World {
    /// A bare remote on `branch`, seeded with `files` by a throwaway device.
    pub fn seeded(files: &[(&str, &[u8])]) -> Self {
        Self::seeded_on(VaultConfig::default(), files)
    }

    pub fn seeded_on(config: VaultConfig, files: &[(&str, &[u8])]) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let bare = dir.path().join("remote.git");
        let mut options = RepositoryInitOptions::new();
        options.bare(true).initial_head(&config.branch);
        Repository::init_opts(&bare, &options).expect("bare remote");
        let remote_url = bare.to_str().expect("utf-8 temp path").to_owned();
        let world = Self { dir, remote_url };
        let seed = Vault::init(world.path("seed"), &world.remote_url, config).expect("seed vault");
        for (path, contents) in files {
            write(&seed, path, contents);
        }
        seed.commit_all(&author("seed"), "Seed vault")
            .expect("seed commit");
        seed.push().expect("seed push");
        world
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    pub fn device(&self, name: &str) -> Vault {
        self.device_with(name, VaultConfig::default())
    }

    pub fn device_with(&self, name: &str, config: VaultConfig) -> Vault {
        let token = Some(Token::new("synthetic-token-never-used-locally"));
        Vault::clone_remote(&self.remote_url, self.path(name), config, token).expect("clone")
    }

    /// The file at `path` on the remote's branch head, if it exists.
    pub fn remote_file(&self, branch: &str, path: &str) -> Option<Vec<u8>> {
        let repo = Repository::open_bare(&self.remote_url).expect("open bare");
        let reference = repo.find_reference(&format!("refs/heads/{branch}")).ok()?;
        let tree = reference.peel_to_tree().expect("tree");
        let entry = tree.get_path(Path::new(path)).ok()?;
        let blob = repo.find_blob(entry.id()).expect("blob");
        Some(blob.content().to_vec())
    }
}

pub fn author(name: &str) -> Author {
    Author::new(name, format!("{name}@devices.invalid"))
}

pub fn write(vault: &Vault, path: &str, contents: &[u8]) {
    let full = vault.root().join(path);
    fs::create_dir_all(full.parent().expect("parent")).expect("mkdir");
    fs::write(full, contents).expect("write");
}

pub fn read(vault: &Vault, path: &str) -> String {
    fs::read_to_string(vault.root().join(path)).expect("read")
}

pub fn read_bytes(vault: &Vault, path: &str) -> Vec<u8> {
    fs::read(vault.root().join(path)).expect("read")
}

/// Commit, fetch, merge and push, as one device would after going online.
pub fn sync(vault: &Vault, who: &str) -> MergeOutcome {
    let author = author(who);
    vault
        .commit_all(&author, &format!("Sync from {who}"))
        .expect("commit");
    vault.fetch().expect("fetch");
    let outcome = vault.merge(&author).expect("merge");
    if !matches!(outcome, MergeOutcome::Conflicts(_)) {
        vault.push().expect("push");
    }
    outcome
}

/// A note of `count` numbered lines.
pub fn numbered_note(count: usize) -> String {
    (1..=count)
        .map(|line| format!("Line {line} of the note.\n"))
        .collect()
}

/// `text` with line `number` (1-based) replaced.
pub fn replace_line(text: &str, number: usize, replacement: &str) -> String {
    text.lines()
        .enumerate()
        .map(|(index, line)| {
            let line = if index + 1 == number {
                replacement
            } else {
                line
            };
            format!("{line}\n")
        })
        .collect()
}
