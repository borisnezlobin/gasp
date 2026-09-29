//! Keeps object lookups quick as commits and fetches pile up objects.
//!
//! git never runs here to repack. Every fetch that brings something in
//! writes one more pack, and libgit2 looks for an object in each pack in
//! turn. Every commit writes its objects as loose files, each read with its
//! own open and inflate, and libgit2's push walks the whole history, so a
//! push read every commit made on this device from its own file. Small
//! loose objects are packed once there are a few hundred, and the smallest
//! packs are written again as one once there are many.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use git2::Oid;

use super::Vault;
use crate::error::{SyncError, SyncResult};

/// How many packs a clone keeps before the smallest are combined.
const PACKS_BEFORE_COMBINING: usize = 24;

/// Loose objects spread evenly over 256 folders by their first byte, so
/// one folder with this many stands for about 512 in all, as git's own
/// `gc.auto` estimate works.
const SAMPLE_FOLDER: &str = "17";
const LOOSE_IN_SAMPLE_BEFORE_PACKING: usize = 2;

/// Loose objects bigger than this, which are photos and other binaries,
/// stay loose: history walks never read them, and packing would only
/// compress them again.
const LARGEST_PACKED_LOOSE: u64 = 256 * 1024;

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
    /// Packs loose objects and combines small packs when there are many.
    /// An object's old copy is only removed after the new pack's index was
    /// found to list it, so a failure part way leaves duplicates, never a
    /// gap. After a failure, as when Windows won't remove an open pack,
    /// tidying stops until the vault is opened again, so a repeated
    /// failure can't pile up packs.
    pub(super) fn tidy_objects(&self) {
        if self.tidying_failed.get() {
            return;
        }
        let tidied = self
            .pack_loose_objects()
            .and_then(|()| self.combine_small_packs());
        self.tidying_failed.set(tidied.is_err());
    }

    fn pack_loose_objects(&self) -> SyncResult<()> {
        let objects = self.repo.path().join("objects");
        let sample = small_loose_in(&objects.join(SAMPLE_FOLDER))?;
        if sample.len() < LOOSE_IN_SAMPLE_BEFORE_PACKING {
            return Ok(());
        }
        let mut loose = Vec::new();
        for byte in 0..=u8::MAX {
            loose.extend(small_loose_in(&objects.join(format!("{byte:02x}")))?);
        }
        let ids: Vec<Oid> = loose.iter().map(|(id, _)| *id).collect();
        self.write_pack_of(&objects.join("pack"), &ids)?;
        loose
            .iter()
            .try_for_each(|(_, path)| remove_if_present(path))
    }

    fn combine_small_packs(&self) -> SyncResult<()> {
        let folder = self.repo.path().join("objects").join("pack");
        let packs = packs_in(&folder)?;
        if packs.len() < PACKS_BEFORE_COMBINING {
            return Ok(());
        }
        let chosen = smallest_within_limit(packs);
        if chosen.len() < 2 {
            return Ok(());
        }
        let mut ids = Vec::new();
        for pack in &chosen {
            ids.extend(object_ids(&pack.index)?);
        }
        let combined = self.write_pack_of(&folder, &ids)?;
        chosen
            .iter()
            .filter(|pack| pack.index != combined)
            .try_for_each(|pack| remove_pack(&pack.index))
    }

    /// Writes `ids` into one new pack and returns its index, after
    /// checking the index lists every one of them.
    fn write_pack_of(&self, folder: &Path, ids: &[Oid]) -> SyncResult<PathBuf> {
        let mut builder = self.repo.packbuilder()?;
        for id in ids {
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

/// The small loose objects in one of the 256 object folders, with their
/// files.
fn small_loose_in(folder: &Path) -> SyncResult<Vec<(Oid, PathBuf)>> {
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let prefix = folder.file_name().and_then(|name| name.to_str());
    let prefix = prefix.unwrap_or_default();
    let mut loose = Vec::new();
    for entry in entries {
        let entry = entry?;
        let small = entry.metadata()?.len() <= LARGEST_PACKED_LOOSE;
        let path = entry.path();
        let name = path.file_name().and_then(|name| name.to_str());
        let hex = format!("{prefix}{}", name.unwrap_or_default());
        let is_object = hex.len() == 2 * OID_BYTES && hex.bytes().all(|b| b.is_ascii_hexdigit());
        if small
            && is_object
            && let Ok(id) = Oid::from_str(&hex)
        {
            loose.push((id, path));
        }
    }
    Ok(loose)
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
