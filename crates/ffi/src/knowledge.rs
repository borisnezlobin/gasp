//! What the vault knows across notes: backlinks, outgoing links, tags and
//! search, from the same index and search engine the desktop uses.

use std::collections::BTreeSet;
use std::sync::atomic::AtomicUsize;

use gasp_search::engine::{LineHit, NoteCache, NoteResult, search};
use gasp_search::tags::{search_tagged, tag_query};
use gasp_vault::ops::slash_path;

use crate::offsets::{TextRange, Utf16Offsets};
use crate::vault::{NoteSummary, VaultFolder};

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TagInfo {
    /// As first written, such as `Physics/waves`.
    pub name: String,
    /// How many notes carry it or a tag nested under it.
    pub notes: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SearchResult {
    pub note: NoteSummary,
    pub hits: Vec<SearchHit>,
}

/// A line of a note that matches.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SearchHit {
    /// Zero-based.
    pub line: u32,
    /// Where the first match on the line is in the note, in UTF-16.
    pub offset: u32,
    pub excerpt: String,
    /// The matches within `excerpt`, in UTF-16.
    pub highlights: Vec<TextRange>,
}

#[uniffi::export]
impl VaultFolder {
    /// The notes that link to the note at `path`.
    pub fn backlinks(&self, path: String) -> Vec<NoteSummary> {
        let sources: Vec<String> = self
            .built_index()
            .as_ref()
            .map(|index| {
                index
                    .backlinks(&path)
                    .into_iter()
                    .map(|backlink| backlink.source.to_owned())
                    .collect()
            })
            .unwrap_or_default();
        sources
            .into_iter()
            .map(|source| self.summary(source))
            .collect()
    }

    /// The notes the note at `path` links to, once each.
    pub fn outgoing_links(&self, path: String) -> Vec<NoteSummary> {
        let targets: BTreeSet<String> = self
            .built_index()
            .as_ref()
            .and_then(|index| index.note(&path).map(|entry| entry.resolved.clone()))
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .filter(|target| target.to_lowercase().ends_with(".md"))
            .collect();
        targets
            .into_iter()
            .map(|target| self.summary(target))
            .collect()
    }

    /// Every tag in the vault, with how many notes carry it.
    pub fn tags(&self) -> Vec<TagInfo> {
        self.built_index()
            .as_ref()
            .map(|index| {
                index
                    .tags()
                    .into_iter()
                    .map(|tag| TagInfo {
                        name: tag.name,
                        notes: tag.notes as u32,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Searches every note, best first, with the desktop's ranking.
    /// `tag:name` or `#name` finds tagged notes.
    pub fn search(&self, query: String) -> Vec<SearchResult> {
        let mut cache = NoteCache::default();
        cache.refresh(&self.root);
        let notes = cache.notes();
        let generation = AtomicUsize::new(0);
        let results = match tag_query(&query) {
            Some(tag) => {
                let tagged = self
                    .built_index()
                    .as_ref()
                    .map(|index| index.notes_tagged(tag))
                    .unwrap_or_default();
                search_tagged(&notes, tag, &tagged, &generation, 0)
            }
            None => search(&notes, &query, &generation, 0),
        };
        results
            .into_iter()
            .map(|result| self.search_result(result, &notes))
            .collect()
    }
}

impl VaultFolder {
    fn search_result(
        &self,
        result: NoteResult,
        notes: &[gasp_search::engine::Note],
    ) -> SearchResult {
        let text = notes
            .iter()
            .find(|note| note.path == result.path)
            .map(|note| note.text.clone());
        let offsets = text.as_deref().map(Utf16Offsets::new);
        SearchResult {
            note: self.summary(slash_path(&result.path)),
            hits: result
                .hits
                .iter()
                .map(|hit| search_hit(hit, offsets.as_ref()))
                .collect(),
        }
    }
}

fn search_hit(hit: &LineHit, note: Option<&Utf16Offsets>) -> SearchHit {
    let excerpt = Utf16Offsets::new(&hit.excerpt);
    SearchHit {
        line: hit.line as u32,
        offset: note.map_or(0, |offsets| offsets.utf16(hit.offset)),
        excerpt: hit.excerpt.clone(),
        highlights: excerpt.ranges(&hit.ranges),
    }
}

#[cfg(test)]
mod tests {
    use crate::vault::tests::vault_with;

    #[test]
    fn links_run_both_ways() {
        let (_dir, vault) = vault_with(&[("A.md", "see [[B]]"), ("B.md", "#physics text")]);
        let titles = |notes: Vec<crate::vault::NoteSummary>| -> Vec<String> {
            notes.into_iter().map(|note| note.title).collect()
        };
        assert_eq!(titles(vault.backlinks("B.md".into())), ["A"]);
        assert_eq!(titles(vault.outgoing_links("A.md".into())), ["B"]);
        assert_eq!(vault.tags()[0].name, "physics");
    }

    #[test]
    fn search_finds_words_and_tags() {
        let (_dir, vault) = vault_with(&[("A.md", "waves and tides"), ("B.md", "#physics")]);
        let found = vault.search("tides".into());
        assert_eq!(found[0].note.title, "A");
        assert_eq!(found[0].hits[0].highlights[0].start, 10);
        assert_eq!(vault.search("#physics".into())[0].note.title, "B");
    }
}
