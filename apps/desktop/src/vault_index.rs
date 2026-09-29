//! What the editor's suggestions know about the vault: every note's path
//! and every tag in use, with how often it's used. It also holds the
//! vault's link graph ([`LinkIndex`]), which the right sidebar, link
//! updates on rename and anything else that needs backlinks read.
//!
//! The workspace builds one per window on a background thread, then keeps
//! it current from the file watcher, re-reading only the note that
//! changed. Editors read it on each keystroke inside `[[` or after `#`, so
//! matching works on prepared [`Candidate`]s and never touches the disk.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use gasp_core::syntax::{self, NodeKind};

use crate::knowledge::build::{LinkChange, build_index, read_changes};
use crate::knowledge::index::LinkIndex;
use crate::knowledge::parse::ParsedNote;
use crate::picker::fuzzy::{Candidate, Matcher, Query};

const NOTE_EXTENSION: &str = ".md";
/// Extra score for a match inside the file name rather than the folders,
/// as in the quick switcher.
const NAME_MATCH_BONUS: i32 = 50;

/// A note the index knows.
#[derive(Clone, Debug)]
pub struct IndexedNote {
    /// Vault-relative, with `/` separators and the `.md` extension.
    pub path: String,
    name_start: usize,
    name_char_start: usize,
    candidate: Candidate,
}

impl IndexedNote {
    fn new(path: String) -> IndexedNote {
        let name_start = path.rfind('/').map_or(0, |slash| slash + 1);
        let candidate = Candidate::new(&path);
        IndexedNote {
            name_char_start: candidate.char_index_of_byte(name_start),
            name_start,
            path,
            candidate,
        }
    }

    /// The file name without `.md`, which is what a wikilink names.
    pub fn name(&self) -> &str {
        let file = &self.path[self.name_start..];
        file.strip_suffix(NOTE_EXTENSION).unwrap_or(file)
    }

    /// The folders above the note, empty at the vault root.
    pub fn folder(&self) -> &str {
        self.path[..self.name_start].trim_end_matches('/')
    }

    /// The path without `.md`, for links to a name several notes share.
    pub fn path_without_extension(&self) -> &str {
        self.path.strip_suffix(NOTE_EXTENSION).unwrap_or(&self.path)
    }
}

/// A note that matched a query, best first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteHit {
    pub note: usize,
    pub score: i32,
    /// Matched byte offsets in the note's name.
    pub name_positions: Vec<usize>,
}

/// A tag that matched a query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagHit {
    pub tag: String,
    pub uses: usize,
    /// Matched byte offsets in the tag.
    pub positions: Vec<usize>,
}

/// Every note and tag in a vault.
#[derive(Clone, Debug, Default)]
pub struct VaultIndex {
    root: PathBuf,
    notes: Vec<IndexedNote>,
    /// Tag name to the number of notes using it.
    tags: BTreeMap<String, usize>,
    /// The tags each note uses, so a changed note can be taken out again.
    note_tags: HashMap<String, Vec<String>>,
    /// Every note's links and backlinks.
    links: LinkIndex,
    ready: bool,
}

/// One note read from disk: its vault-relative path and tags.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteScan {
    pub path: String,
    pub tags: Vec<String>,
}

impl VaultIndex {
    /// Reads every note under `vault`. This walks the folder and reads
    /// and parses each note once, on every core, so run it off the main
    /// thread.
    pub fn scan(vault: &Path) -> VaultIndex {
        let links = build_index(vault);
        let mut paths: Vec<&str> = links.note_paths().collect();
        paths.sort_unstable();
        let mut index = VaultIndex::new(vault);
        index.notes = paths
            .iter()
            .map(|path| IndexedNote::new(path.to_string()))
            .collect();
        for path in paths {
            let tags = links
                .note(path)
                .map(|entry| distinct_tags(&entry.parsed))
                .unwrap_or_default();
            for tag in &tags {
                *index.tags.entry(tag.clone()).or_default() += 1;
            }
            index.note_tags.insert(path.to_string(), tags);
        }
        index.links = links;
        index.ready = true;
        index
    }

    /// Every note's links, resolved, and the backlinks to every file.
    pub fn links(&self) -> &LinkIndex {
        &self.links
    }

    /// Takes `text` as the note at `path` (vault-relative) now, ahead of
    /// the watcher, as after an edit the app made for the user.
    pub fn note_text_changed(&mut self, path: &str, text: &str) {
        let unchanged = self
            .links
            .note(path)
            .is_some_and(|entry| &*entry.text == text);
        if unchanged {
            return;
        }
        self.links.set_note(path, text);
        let tags = self
            .links
            .note(path)
            .map(|entry| distinct_tags(&entry.parsed))
            .unwrap_or_default();
        self.upsert(NoteScan {
            path: path.to_string(),
            tags,
        });
    }

    /// Follows a rename the app made, ahead of the watcher.
    pub fn renamed(&mut self, from: &str, to: &str) {
        let tags = self.note_tags.get(from).cloned();
        self.remove_note_entries(from);
        self.links.rename(from, to);
        if let Some(tags) = tags {
            self.upsert(NoteScan {
                path: to.to_string(),
                tags,
            });
        }
    }

    /// An empty index of the vault at `root`, before its scan.
    pub fn new(root: &Path) -> VaultIndex {
        VaultIndex {
            root: root.to_path_buf(),
            ..VaultIndex::default()
        }
    }

    /// The vault folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether the first scan has finished.
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    pub fn notes(&self) -> &[IndexedNote] {
        &self.notes
    }

    pub fn note(&self, index: usize) -> &IndexedNote {
        &self.notes[index]
    }

    /// Tags by name with their use counts.
    pub fn tags(&self) -> &BTreeMap<String, usize> {
        &self.tags
    }

    /// Adds a note, or replaces what the index knew about it.
    pub fn upsert(&mut self, scan: NoteScan) {
        self.forget_tags(&scan.path);
        if let Err(at) = self
            .notes
            .binary_search_by(|note| note.path.as_str().cmp(&scan.path))
        {
            self.notes.insert(at, IndexedNote::new(scan.path.clone()));
        }
        for tag in &scan.tags {
            *self.tags.entry(tag.clone()).or_default() += 1;
        }
        self.note_tags.insert(scan.path, scan.tags);
    }

    /// Forgets the note at `path`, or every note under it when it's a
    /// folder.
    pub fn remove(&mut self, path: &str) {
        self.links.remove(path);
        self.remove_note_entries(path);
    }

    fn remove_note_entries(&mut self, path: &str) {
        let folder = format!("{}/", path.trim_end_matches('/'));
        let gone: Vec<String> = self
            .notes
            .iter()
            .filter(|note| note.path == path || note.path.starts_with(&folder))
            .map(|note| note.path.clone())
            .collect();
        for path in gone {
            self.forget_tags(&path);
            self.notes.retain(|note| note.path != path);
        }
    }

    fn forget_tags(&mut self, path: &str) {
        for tag in self.note_tags.remove(path).unwrap_or_default() {
            if let Some(uses) = self.tags.get_mut(&tag) {
                *uses -= 1;
                if *uses == 0 {
                    self.tags.remove(&tag);
                }
            }
        }
    }

    /// Whether another note has the same name as note `index`, so a link
    /// to it needs its path.
    pub fn name_is_shared(&self, index: usize) -> bool {
        let name = self.notes[index].name();
        self.notes
            .iter()
            .enumerate()
            .any(|(other, note)| other != index && note.name().eq_ignore_ascii_case(name))
    }

    /// The note a wikilink target names: an exact path, else the shortest
    /// path whose file name matches, ignoring case.
    pub fn find_note(&self, target: &str) -> Option<&IndexedNote> {
        let target = target.trim().trim_end_matches(NOTE_EXTENSION);
        let by_path = self
            .notes
            .iter()
            .find(|note| note.path_without_extension().eq_ignore_ascii_case(target));
        by_path.or_else(|| {
            self.notes
                .iter()
                .filter(|note| note.name().eq_ignore_ascii_case(target))
                .min_by_key(|note| note.path.len())
        })
    }

    /// The file an embed in a note in `note_dir` names, found as Obsidian
    /// finds it: by path, else by file name anywhere in the vault, the
    /// note's own folder first, then the shortest path. Attachments are in
    /// the index with the notes, and the watcher keeps both current.
    pub fn find_file(&self, note_dir: &Path, target: &str) -> Option<PathBuf> {
        let dir = vault_relative(&self.root, note_dir).unwrap_or_default();
        let found = self.links.resolve_embed(&dir, target.trim())?;
        Some(self.root.join(found))
    }

    /// Up to `limit` notes matching `query`, best first. File names rank
    /// above folders; an empty query lists notes by path.
    pub fn match_notes(&self, query: &str, limit: usize) -> Vec<NoteHit> {
        let query = Query::new(query);
        let mut matcher = Matcher::new();
        let mut hits: Vec<NoteHit> = self
            .notes
            .iter()
            .enumerate()
            .filter_map(|(index, note)| {
                let by_name = matcher.score_from(&query, &note.candidate, note.name_char_start);
                let (score, positions) = match by_name {
                    Some(found) => (found.score + NAME_MATCH_BONUS, found.positions),
                    None => matcher
                        .score(&query, &note.candidate)
                        .map(|found| (found.score, found.positions))?,
                };
                let name_positions = positions
                    .into_iter()
                    .filter(|position| *position >= note.name_start)
                    .map(|position| position - note.name_start)
                    .collect();
                Some(NoteHit {
                    note: index,
                    score,
                    name_positions,
                })
            })
            .collect();
        let notes = &self.notes;
        hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| notes[a.note].path.len().cmp(&notes[b.note].path.len()))
                .then(a.note.cmp(&b.note))
        });
        hits.truncate(limit);
        hits
    }

    /// Up to `limit` tags matching `query`: the best matches first, and
    /// among equals the most used.
    pub fn match_tags(&self, query: &str, limit: usize) -> Vec<TagHit> {
        let query = Query::new(query);
        let mut matcher = Matcher::new();
        let mut hits: Vec<(i32, TagHit)> = self
            .tags
            .iter()
            .filter_map(|(tag, uses)| {
                let found = matcher.score(&query, &Candidate::new(tag))?;
                Some((
                    found.score,
                    TagHit {
                        tag: tag.clone(),
                        uses: *uses,
                        positions: found.positions,
                    },
                ))
            })
            .collect();
        hits.sort_by(|(a_score, a), (b_score, b)| {
            b_score
                .cmp(a_score)
                .then(b.uses.cmp(&a.uses))
                .then_with(|| a.tag.cmp(&b.tag))
        });
        hits.into_iter().take(limit).map(|(_, hit)| hit).collect()
    }
}

/// Reads one note for the index. `None` when it isn't a note in the vault
/// or can't be read.
pub fn scan_note(vault: &Path, path: &Path) -> Option<NoteScan> {
    let relative = vault_relative(vault, path)?;
    if !relative.ends_with(NOTE_EXTENSION) {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    Some(NoteScan {
        path: relative,
        tags: tags_in(&text),
    })
}

/// `path` relative to `vault` with `/` separators.
pub fn vault_relative(vault: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(vault).ok()?;
    Some(relative.to_string_lossy().replace('\\', "/"))
}

/// Every distinct tag a note uses, inline or in its frontmatter, without
/// the `#`.
pub fn tags_in(text: &str) -> Vec<String> {
    let tree = syntax::parse(text);
    let mut tags: Vec<String> = frontmatter_tags(text);
    for id in tree.preorder() {
        if let NodeKind::Tag { name } = &tree.node(id).kind {
            tags.push(name.clone());
        }
    }
    tags.sort();
    tags.dedup();
    tags
}

/// Tags from a `tags:` (or `tag:`) key in YAML frontmatter, written inline
/// (`tags: [a, b]` or `tags: a, b`) or as a list of `- a` lines.
fn frontmatter_tags(text: &str) -> Vec<String> {
    let Some(body) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return Vec::new();
    };
    let body = body
        .split("\n---")
        .next()
        .expect("split always yields one part");
    let mut lines = body.lines().peekable();
    let mut tags = Vec::new();
    while let Some(line) = lines.next() {
        let Some(value) = line
            .strip_prefix("tags:")
            .or_else(|| line.strip_prefix("tag:"))
        else {
            continue;
        };
        tags.extend(inline_tags(value));
        while let Some(item) = lines.peek().and_then(|next| list_item(next)) {
            tags.extend(inline_tags(item));
            lines.next();
        }
    }
    tags
}

fn list_item(line: &str) -> Option<&str> {
    line.trim_start().strip_prefix("- ")
}

fn inline_tags(value: &str) -> impl Iterator<Item = String> + '_ {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|tag| {
            tag.trim()
                .trim_matches(|c| c == '"' || c == '\'')
                .trim_start_matches('#')
        })
        .filter(|tag| !tag.is_empty())
        .map(str::to_owned)
}

/// A note's tags, each once, sorted.
fn distinct_tags(parsed: &ParsedNote) -> Vec<String> {
    let mut tags: Vec<String> = parsed.tags.iter().map(|tag| tag.name.clone()).collect();
    tags.sort();
    tags.dedup();
    tags
}

/// Where a note's changes land in the index, for the watcher.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IndexChange {
    Upsert(NoteScan),
    Remove(String),
    /// A note's text or a file, for the link graph.
    Links(LinkChange),
}

/// What the index should do about a changed, removed or renamed path.
/// Reads the file, so run it off the main thread.
pub fn index_changes(vault: &Path, changed: &[PathBuf], removed: &[PathBuf]) -> Vec<IndexChange> {
    let removals = removed
        .iter()
        .filter_map(|path| vault_relative(vault, path))
        .map(IndexChange::Remove);
    // Each note is read and parsed once, for its tags and its links; a
    // folder that moved in is read whole.
    let updates = read_changes(vault, changed).into_iter().flat_map(|change| {
        let scan = match &change {
            LinkChange::Note(path, _, parsed) => Some(IndexChange::Upsert(NoteScan {
                path: path.clone(),
                tags: distinct_tags(parsed),
            })),
            LinkChange::File(_) => None,
        };
        scan.into_iter().chain([IndexChange::Links(change)])
    });
    removals.chain(updates).collect()
}

impl VaultIndex {
    pub fn apply(&mut self, changes: Vec<IndexChange>) {
        for change in changes {
            match change {
                IndexChange::Upsert(scan) => self.upsert(scan),
                IndexChange::Remove(path) => self.remove(&path),
                IndexChange::Links(change) => self.apply_link_change(change),
            }
        }
    }
}

impl VaultIndex {
    fn apply_link_change(&mut self, change: LinkChange) {
        match change {
            LinkChange::Note(path, text, parsed) => {
                let unchanged = self
                    .links
                    .note(&path)
                    .is_some_and(|entry| entry.text == text);
                if !unchanged {
                    self.links.set_parsed_note(&path, text, parsed);
                }
            }
            LinkChange::File(path) => self.links.add_file(&path),
        }
    }

    /// For tests: an index with these notes' links, ready.
    pub fn with_notes(root: &Path, notes: &[(&str, &str)]) -> VaultIndex {
        let mut index = VaultIndex::new(root);
        for (path, text) in notes {
            index.note_text_changed(path, text);
        }
        index.ready = true;
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(notes: &[(&str, &str)]) -> VaultIndex {
        let mut index = VaultIndex::default();
        for (path, text) in notes {
            index.upsert(NoteScan {
                path: path.to_string(),
                tags: tags_in(text),
            });
        }
        index
    }

    #[test]
    fn finds_inline_and_frontmatter_tags_but_not_code() {
        let text = "---\ntags: [physics, \"#waves\"]\naliases:\n  - x\n---\n\
                    #quantum and `#code` and #nested/tag\n```\n#fenced\n```\n";
        assert_eq!(tags_in(text), ["nested/tag", "physics", "quantum", "waves"]);
        let listed = "---\ntags:\n  - a\n  - b\ntitle: t\n---\n";
        assert_eq!(tags_in(listed), ["a", "b"]);
    }

    #[test]
    fn counts_tags_and_forgets_them_with_the_note() {
        let mut index = index(&[("a.md", "#x #y"), ("b.md", "#x")]);
        assert_eq!(index.tags().get("x"), Some(&2));
        index.remove("a.md");
        assert_eq!(index.tags().get("x"), Some(&1));
        assert_eq!(index.tags().get("y"), None);
        index.upsert(NoteScan {
            path: "b.md".into(),
            tags: vec!["z".into()],
        });
        assert_eq!(index.tags().keys().collect::<Vec<_>>(), ["z"]);
        assert_eq!(index.notes().len(), 1);
    }

    #[test]
    fn removing_a_folder_removes_its_notes() {
        let mut index = index(&[("f/a.md", ""), ("f/b.md", ""), ("fa.md", "")]);
        index.remove("f");
        let paths: Vec<&str> = index.notes().iter().map(|n| n.path.as_str()).collect();
        assert_eq!(paths, ["fa.md"]);
    }

    #[test]
    fn notes_match_by_name_first() {
        let index = index(&[
            ("waves/notes.md", ""),
            ("Wave Packets.md", ""),
            ("daily/2024-01-01.md", ""),
        ]);
        let hits = index.match_notes("wave", 10);
        assert_eq!(index.note(hits[0].note).name(), "Wave Packets");
        assert_eq!(hits[0].name_positions, [0, 1, 2, 3]);
        assert_eq!(index.match_notes("", 10).len(), 3);
        assert!(index.match_notes("zzz", 10).is_empty());
    }

    #[test]
    fn tags_rank_by_match_then_use() {
        let index = index(&[("a.md", "#physics #phd"), ("b.md", "#physics")]);
        let hits: Vec<String> = index.match_tags("", 5).into_iter().map(|h| h.tag).collect();
        assert_eq!(hits, ["physics", "phd"]);
        let hits: Vec<String> = index
            .match_tags("phd", 5)
            .into_iter()
            .map(|h| h.tag)
            .collect();
        assert_eq!(hits, ["phd"]);
    }

    #[test]
    fn finds_notes_by_name_or_path_ignoring_case() {
        let index = index(&[
            ("deep/x/Topic.md", ""),
            ("x/topic.md", ""),
            ("Other.md", ""),
        ]);
        assert_eq!(index.find_note("TOPIC").unwrap().path, "x/topic.md");
        assert_eq!(
            index.find_note("deep/x/Topic").unwrap().path,
            "deep/x/Topic.md"
        );
        let position = |path: &str| index.notes().iter().position(|n| n.path == path).unwrap();
        assert!(index.name_is_shared(position("x/topic.md")));
        assert!(!index.name_is_shared(position("Other.md")));
    }

    #[test]
    fn finds_attachments_anywhere_by_name() {
        let root = Path::new("/vault");
        let mut index = VaultIndex::with_notes(root, &[("notes/Note.md", "")]);
        let file = |path: &str| IndexChange::Links(LinkChange::File(path.into()));
        index.apply(vec![
            file("assets/deep/pic.png"),
            file("zz/pic.png"),
            file("notes/own.png"),
            file("other/own.png"),
        ]);
        let notes = root.join("notes");
        assert_eq!(
            index.find_file(&notes, "pic.png"),
            Some(root.join("zz/pic.png")),
            "the shortest path"
        );
        assert_eq!(
            index.find_file(&notes, "OWN.png"),
            Some(root.join("notes/own.png")),
            "the note's own folder first, ignoring case"
        );
        assert_eq!(
            index.find_file(&notes, "deep/pic.png"),
            Some(root.join("assets/deep/pic.png"))
        );
        assert_eq!(index.find_file(root, "missing.png"), None);
        index.apply(vec![IndexChange::Remove("zz/pic.png".into())]);
        assert_eq!(
            index.find_file(&notes, "pic.png"),
            Some(root.join("assets/deep/pic.png")),
            "the watcher's removals reach it"
        );
    }
}
