//! Keeps object lookups quick as fetches pile up packs.
//!
//! Every fetch that brings something in writes one more pack, git never
//! runs here to repack them, and libgit2 looks for an object in each pack
//! in turn. A few hundred packs made a merge twenty times slower, so once
//! there are many, the smallest are written again as one.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use git2::Oid;

use super::Vault;
use crate::error::{SyncError, SyncResult};

/// How many packs a clone keeps before the smallest are combined.
const PACKS_BEFORE_COMBINING: usize = 24;

/// The most pack bytes one combining writes again. Notes come in packs of
/// a few kilobytes, so this usually takes every pack but the clone's own,
/// while a run of fetched photos is combined a slice at a time.
const COMBINED_BYTES_LIMIT: u64 = 16 * 1024 * 1024;

const INDEX_MAGIC: [u8; 4] = [0xff, b't', b'O', b'c'];
const INDEX_VERSION: u32 = 2;
const FANOUT_START: usize = 8;
const NAMES_START: usize = FANOUT_START + 256 * 4;
const OID_BYTES: usize = 20;

/// Files that belong to a pack besides the pack and its index.
const PACK_COMPANIONS: [&str; 3] = ["rev", "mtimes", "bitmap"];

/// One pack in the clone's object folder, named by its index file.
struct Pack {
    index: PathBuf,
    bytes: u64,
}

impl Vault {
    /// Combines the smallest packs into one once the clone has many. An
    /// old pack is only removed after every object in it was found in the
    /// new one, so a failure part way leaves duplicates, never a gap.
    pub(super) fn combine_small_packs(&self) -> SyncResult<()> {
        if self.pack_combining_failed.get() {
            return Ok(());
        }
        let folder = self.repo.path().join("objects").join("pack");
        let packs = packs_in(&folder)?;
        if packs.len() < PACKS_BEFORE_COMBINING {
            return Ok(());
        }
        let chosen = smallest_within_limit(packs);
        if chosen.len() < 2 {
            return Ok(());
        }
        let combined = self.write_combined(&folder, &chosen)?;
        let removed = chosen
            .iter()
            .filter(|pack| pack.index != combined)
            .try_for_each(|pack| remove_pack(&pack.index));
        // Where open packs can't be removed, each try would add a pack.
        self.pack_combining_failed.set(removed.is_err());
        removed
    }

    /// Writes every object of `packs` into one new pack and returns its
    /// index, after checking the index lists every one of them.
    fn write_combined(&self, folder: &Path, packs: &[Pack]) -> SyncResult<PathBuf> {
        let mut ids = Vec::new();
        for pack in packs {
            ids.extend(object_ids(&pack.index)?);
        }
        let mut builder = self.repo.packbuilder()?;
        for id in &ids {
            builder.insert_object(*id, None)?;
        }
        builder.write(folder, 0)?;
        let name = builder
            .name()
            .ok_or_else(|| unreadable_pack(folder))?
            .to_owned();
        let index = folder.join(format!("pack-{name}.idx"));
        let mut written = object_ids(&index)?;
        written.sort_unstable();
        if ids.iter().any(|id| written.binary_search(id).is_err()) {
            return Err(unreadable_pack(&index));
        }
        Ok(index)
    }
}

/// Every pack in `folder` that has both its files and isn't marked to be
/// kept as it is.
fn packs_in(folder: &Path) -> SyncResult<Vec<Pack>> {
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut packs = Vec::new();
    for entry in entries {
        let index = entry?.path();
        if index.extension().is_none_or(|extension| extension != "idx") {
            continue;
        }
        if index.with_extension("keep").exists() {
            continue;
        }
        if let Ok(metadata) = fs::metadata(index.with_extension("pack")) {
            packs.push(Pack {
                index,
                bytes: metadata.len(),
            });
        }
    }
    Ok(packs)
}

/// The smallest packs whose sizes add up to at most the limit.
fn smallest_within_limit(mut packs: Vec<Pack>) -> Vec<Pack> {
    packs.sort_by_key(|pack| pack.bytes);
    let mut total = 0;
    packs
        .into_iter()
        .take_while(|pack| {
            total += pack.bytes;
            total <= COMBINED_BYTES_LIMIT
        })
        .collect()
}

/// The objects a version 2 pack index lists.
fn object_ids(index: &Path) -> SyncResult<Vec<Oid>> {
    let bytes = fs::read(index)?;
    let header_fits = bytes.len() >= NAMES_START
        && bytes[..4] == INDEX_MAGIC
        && read_u32(&bytes, 4) == INDEX_VERSION;
    if !header_fits {
        return Err(unreadable_pack(index));
    }
    let count = read_u32(&bytes, NAMES_START - 4) as usize;
    let names = bytes
        .get(NAMES_START..NAMES_START + count * OID_BYTES)
        .ok_or_else(|| unreadable_pack(index))?;
    names
        .chunks_exact(OID_BYTES)
        .map(|name| Oid::from_bytes(name).map_err(SyncError::from))
        .collect()
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Removes a pack's index first, so a pack whose removal stops part way
/// is one libgit2 no longer sees rather than one it half sees.
fn remove_pack(index: &Path) -> SyncResult<()> {
    remove_if_present(index)?;
    remove_if_present(&index.with_extension("pack"))?;
    for companion in PACK_COMPANIONS {
        remove_if_present(&index.with_extension(companion))?;
    }
    Ok(())
}

fn remove_if_present(path: &Path) -> SyncResult<()> {
    allow_removal(path);
    match fs::remove_file(path) {
        Err(error) if error.kind() != ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}

/// Windows refuses to remove the read-only files git writes packs as.
#[cfg(windows)]
fn allow_removal(path: &Path) {
    if let Ok(metadata) = fs::metadata(path) {
        let mut permissions = metadata.permissions();
        permissions.set_readonly(false);
        let _ = fs::set_permissions(path, permissions);
    }
}

#[cfg(not(windows))]
fn allow_removal(_: &Path) {}

fn unreadable_pack(path: &Path) -> SyncError {
    SyncError::Git(git2::Error::from_str(&format!(
        "unexpected pack index {}",
        path.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(bytes: u64) -> Pack {
        Pack {
            index: PathBuf::from(format!("pack-{bytes}.idx")),
            bytes,
        }
    }

    #[test]
    fn the_smallest_packs_are_taken_up_to_the_limit() {
        let megabyte = 1024 * 1024;
        let packs = vec![
            pack(250 * megabyte),
            pack(3 * megabyte),
            pack(2_000),
            pack(9 * megabyte),
        ];
        let chosen: Vec<u64> = smallest_within_limit(packs)
            .iter()
            .map(|pack| pack.bytes)
            .collect();
        assert_eq!(chosen, [2_000, 3 * megabyte, 9 * megabyte]);
    }

    #[test]
    fn an_index_that_isnt_version_two_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let index = dir.path().join("pack-a.idx");
        fs::write(&index, [0u8; NAMES_START]).unwrap();
        assert!(object_ids(&index).is_err());
    }
}
