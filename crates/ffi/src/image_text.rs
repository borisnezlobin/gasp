//! Search inside the vault's images and PDFs. The phone recognises their
//! text with Vision and hands it here; this keeps it in a cache on the
//! device, so each file is read once, and folds matches into vault
//! search as hits on the notes that embed the file.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use gasp_search::ocr::{FileHit, FileStamp, OcrCache, is_recognised};
use gasp_search::tags::tag_query;
use gasp_vault::link_update::file_name;

use crate::knowledge::{SearchHit, SearchResult};
use crate::offsets::Utf16Offsets;
use crate::vault::{VaultError, VaultFolder};

#[derive(uniffi::Object)]
pub struct ImageTexts {
    vault: Arc<VaultFolder>,
    cache_file: PathBuf,
    cache: Mutex<OcrCache>,
}

#[uniffi::export]
impl ImageTexts {
    /// The text recognised so far in `vault`'s files, cached in
    /// `cache_file`, which belongs to this device.
    #[uniffi::constructor]
    pub fn new(vault: Arc<VaultFolder>, cache_file: String) -> Arc<Self> {
        let cache_file = PathBuf::from(cache_file);
        Arc::new(Self {
            cache: Mutex::new(OcrCache::load(&cache_file)),
            vault,
            cache_file,
        })
    }

    /// The images and PDFs (vault-relative) never read or changed since,
    /// forgetting files that are gone.
    pub fn files_to_read(&self) -> Vec<String> {
        let files: Vec<(String, FileStamp)> = self
            .vault
            .built_index()
            .as_ref()
            .map(|index| index.file_paths())
            .unwrap_or_default()
            .into_iter()
            .filter(|path| is_recognised(path))
            .filter_map(|path| {
                let stamp = FileStamp::of(&self.vault.root.join(&path))?;
                Some((path, stamp))
            })
            .collect();
        self.cache().to_read(&files)
    }

    /// Where a vault-relative file is on disk.
    pub fn full_path(&self, path: String) -> String {
        self.vault.root.join(path).to_string_lossy().into_owned()
    }

    /// Keeps `text` as what the file at `path` says, as it is now.
    pub fn record(&self, path: String, text: String) {
        let Some(stamp) = FileStamp::of(&self.vault.root.join(&path)) else {
            return;
        };
        self.cache().record(&path, stamp, text);
    }

    /// Writes the cache to its file.
    pub fn save(&self) -> Result<(), VaultError> {
        let text = self.cache().to_text();
        if let Some(folder) = self.cache_file.parent() {
            std::fs::create_dir_all(folder)?;
        }
        Ok(std::fs::write(&self.cache_file, text)?)
    }

    /// How many files have been read.
    pub fn count(&self) -> u32 {
        self.cache().len() as u32
    }
}

impl ImageTexts {
    fn cache(&self) -> MutexGuard<'_, OcrCache> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[uniffi::export]
impl VaultFolder {
    /// [`VaultFolder::search`], plus the notes that embed an image or PDF
    /// whose recognised text matches, each with a hit on the embed's line
    /// that quotes the file.
    pub fn search_everything(&self, query: String, images: Arc<ImageTexts>) -> Vec<SearchResult> {
        let mut results = self.search(query.clone());
        if tag_query(&query).is_some() {
            return results;
        }
        for found in images.cache().search(&query) {
            for note in self.embedding_notes(&found.path) {
                let Some(hit) = self.embed_hit(&note, &found) else {
                    continue;
                };
                match results.iter_mut().find(|result| result.note.path == note) {
                    Some(result) => result.hits.push(hit),
                    None => results.push(SearchResult {
                        note: self.summary(note),
                        hits: vec![hit],
                    }),
                }
            }
        }
        results
    }
}

impl VaultFolder {
    fn embedding_notes(&self, file: &str) -> Vec<String> {
        self.built_index()
            .as_ref()
            .map(|index| index.linking_to(file))
            .unwrap_or_default()
    }

    /// A hit on the line of `note` that embeds the file, reading "name:
    /// matching text".
    fn embed_hit(&self, note: &str, found: &FileHit) -> Option<SearchHit> {
        let text = std::fs::read_to_string(self.root.join(note)).ok()?;
        let name = file_name(&found.path);
        let at = text.find(name)?;
        let first = found.hits.first()?;
        let label = format!("{name}: ");
        let excerpt = format!("{label}{}", first.excerpt);
        let shifted: Vec<std::ops::Range<usize>> = first
            .ranges
            .iter()
            .map(|range| range.start + label.len()..range.end + label.len())
            .collect();
        Some(SearchHit {
            line: text[..at].matches('\n').count() as u32,
            offset: Utf16Offsets::new(&text).utf16(at),
            highlights: Utf16Offsets::new(&excerpt).ranges(&shifted),
            excerpt,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offsets::TextRange;
    use crate::vault::tests::vault_with;

    #[test]
    fn a_note_is_found_by_the_text_in_its_image() {
        let (dir, vault) = vault_with(&[
            ("Lecture.md", "# Notes\n\n![[board.png]]\n"),
            ("images/board.png", "png"),
            ("Other.md", "nothing"),
        ]);
        let cache_file = dir.path().join("cache/ocr.txt");
        let images = ImageTexts::new(vault.clone(), cache_file.to_string_lossy().into());
        assert_eq!(images.files_to_read(), ["images/board.png"]);
        images.record("images/board.png".into(), "Wave equation".into());
        images.save().unwrap();
        let reloaded = ImageTexts::new(vault.clone(), cache_file.to_string_lossy().into());
        assert!(reloaded.files_to_read().is_empty());
        let found = vault.search_everything("wave".into(), reloaded);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].note.path, "Lecture.md");
        let hit = &found[0].hits[0];
        assert_eq!(hit.line, 2);
        assert_eq!(hit.excerpt, "board.png: Wave equation");
        assert_eq!(hit.highlights, [TextRange { start: 11, end: 15 }]);
    }
}
