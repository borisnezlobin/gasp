//! The vault's link graph: each note's outgoing links, resolved the way
//! Obsidian resolves them, the backlinks to every file, links to notes
//! that don't exist, and how many notes carry each tag.
//!
//! Paths are vault-relative and `/`-separated, such as `Projects/Plan.md`.
//! Everything updates one note at a time: changing a note re-reads only
//! it, and adding or removing a file re-resolves only the notes whose
//! links name a file of that name.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use super::parse::{Link, ParsedNote, Tag, parse_note};
use crate::link_update::{
    candidate_forms, file_name, join, normalize, parent_dir, strip_note_extension,
};

/// One note as the index knows it.
#[derive(Clone, Debug)]
pub struct NoteEntry {
    pub text: Arc<str>,
    pub parsed: ParsedNote,
    /// Where each of `parsed.links` goes, if anywhere.
    pub resolved: Vec<Option<String>>,
}

/// A note that links to another, with the links that do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Backlink<'a> {
    pub source: &'a str,
    pub links: Vec<&'a Link>,
}

/// A tag with how many notes carry it or a tag nested under it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagCount {
    /// As first written, such as `Physics/waves`.
    pub name: String,
    pub notes: usize,
}

/// The whole vault's links and tags.
#[derive(Clone, Debug, Default)]
pub struct LinkIndex {
    notes: HashMap<String, NoteEntry>,
    files: FileNames,
    /// Target file → notes linking to it.
    backlinks: HashMap<String, BTreeSet<String>>,
    /// A link's name key → notes with such a link, to re-resolve when a
    /// file of that name comes or goes.
    by_key: HashMap<String, HashSet<String>>,
    /// Lowercased tag, and every parent of a nested tag → its count.
    tags: HashMap<String, TagCount>,
}

/// Whether a path is a note the index reads.
pub fn is_note_path(path: &str) -> bool {
    path.to_lowercase().ends_with(".md")
}

/// The key links and files meet on: the lowercased file name without
/// `.md`, so `[[plan]]`, `[x](../Plan.md)` and `Plan.md` all share `plan`.
fn name_key(path: &str) -> String {
    strip_note_extension(file_name(path)).to_lowercase()
}

impl LinkIndex {
    pub fn new() -> LinkIndex {
        LinkIndex::default()
    }

    /// An index of `files` (every file in the vault) with the notes'
    /// texts already parsed. Resolving needs every file, so they all go
    /// in first.
    pub fn build(files: Vec<String>, notes: Vec<(String, Arc<str>, ParsedNote)>) -> LinkIndex {
        let names = FileNames::new(files.iter().chain(notes.iter().map(|(path, ..)| path)));
        let notes = notes
            .into_iter()
            .map(|(path, text, parsed)| {
                let resolved = names.resolve_all(&path, &parsed.links);
                (path, text, parsed, resolved)
            })
            .collect();
        LinkIndex::from_resolved(names, notes)
    }

    /// An index of every file in `names` and notes whose links are
    /// already resolved against them, as the parallel build makes.
    pub fn from_resolved(names: FileNames, notes: Vec<ResolvedNote>) -> LinkIndex {
        let mut index = LinkIndex {
            files: names,
            ..LinkIndex::default()
        };
        index.notes.reserve(notes.len());
        for (path, text, parsed, resolved) in notes {
            index.index_resolved(path, text, parsed, resolved);
        }
        index
    }

    pub fn note_count(&self) -> usize {
        self.notes.len()
    }

    pub fn note(&self, path: &str) -> Option<&NoteEntry> {
        self.notes.get(path)
    }

    /// Every note's path, in no order.
    pub fn note_paths(&self) -> impl Iterator<Item = &str> {
        self.notes.keys().map(String::as_str)
    }

    /// Every file in the vault, notes and the rest.
    pub fn file_paths(&self) -> Vec<String> {
        self.files.all().map(str::to_string).collect()
    }

    pub fn contains_file(&self, path: &str) -> bool {
        self.files.exact(path).is_some()
    }

    // ---- Updates ----

    /// Reads `text` as the note at `path`, adding the note if it's new.
    pub fn set_note(&mut self, path: &str, text: impl Into<Arc<str>>) {
        let text = text.into();
        let parsed = parse_note(&text);
        self.set_parsed_note(path, text, parsed);
    }

    /// Like [`LinkIndex::set_note`] with the parsing already done, as the
    /// background reader does.
    pub fn set_parsed_note(&mut self, path: &str, text: Arc<str>, parsed: ParsedNote) {
        self.unindex_note(path);
        let is_new = self.files.insert(path);
        self.index_note(path.to_string(), text, parsed);
        if is_new {
            self.reresolve_key(&name_key(path), Some(path));
        }
    }

    /// Adds a file that isn't a note, such as an image.
    pub fn add_file(&mut self, path: &str) {
        if self.files.insert(path) {
            self.reresolve_key(&name_key(path), None);
        }
    }

    /// Forgets `path`, or everything under it if it's a folder.
    pub fn remove(&mut self, path: &str) {
        let prefix = format!("{}/", path.trim_end_matches('/'));
        let gone: Vec<String> = self
            .files
            .all()
            .filter(|file| *file == path || file.starts_with(&prefix))
            .map(str::to_string)
            .collect();
        for file in gone {
            self.remove_file(&file);
        }
    }

    fn remove_file(&mut self, path: &str) {
        self.unindex_note(path);
        self.notes.remove(path);
        if self.files.remove(path) {
            self.reresolve_key(&name_key(path), None);
        }
    }

    /// Follows a move of a file or folder without reading anything again.
    pub fn rename(&mut self, from: &str, to: &str) {
        let prefix = format!("{}/", from.trim_end_matches('/'));
        let moved: Vec<(String, String)> = self
            .files
            .all()
            .filter_map(|file| {
                let rest = if file == from {
                    ""
                } else {
                    file.strip_prefix(&prefix)?
                };
                let target = if rest.is_empty() {
                    to.to_string()
                } else {
                    format!("{}/{rest}", to.trim_end_matches('/'))
                };
                Some((file.to_string(), target))
            })
            .collect();
        let mut notes = Vec::new();
        for (old, new) in &moved {
            let entry = self.notes.get(old).cloned();
            self.remove_file(old);
            match entry {
                Some(entry) => notes.push((new.clone(), entry)),
                None => self.add_file(new),
            }
        }
        for (new, entry) in notes {
            self.set_parsed_note(&new, entry.text, entry.parsed);
        }
    }

    fn index_note(&mut self, path: String, text: Arc<str>, parsed: ParsedNote) {
        let resolved = self.files.resolve_all(&path, &parsed.links);
        self.index_resolved(path, text, parsed, resolved);
    }

    fn index_resolved(
        &mut self,
        path: String,
        text: Arc<str>,
        parsed: ParsedNote,
        resolved: Vec<Option<String>>,
    ) {
        // Notes link to the same few places many times; each goes in once.
        for key in link_keys(&parsed.links) {
            let sources = self.by_key.entry(key).or_default();
            if !sources.contains(&path) {
                sources.insert(path.clone());
            }
        }
        for target in distinct(resolved.iter().flatten()) {
            match self.backlinks.get_mut(target) {
                Some(sources) => {
                    sources.insert(path.clone());
                }
                None => {
                    let sources = BTreeSet::from([path.clone()]);
                    self.backlinks.insert(target.clone(), sources);
                }
            }
        }
        for tag in tag_prefixes(&parsed.tags) {
            let name = self.tag_display(&tag);
            let count = self
                .tags
                .entry(tag.to_lowercase())
                .or_insert(TagCount { name, notes: 0 });
            count.notes += 1;
        }
        self.notes.insert(
            path,
            NoteEntry {
                text,
                parsed,
                resolved,
            },
        );
    }

    /// How a new tag shows: under its parent's spelling, so `#Maths` and
    /// `#maths/limits` list as `Maths` and `Maths/limits`.
    fn tag_display(&self, tag: &str) -> String {
        match tag.rsplit_once('/') {
            Some((parent, name)) => match self.tags.get(&parent.to_lowercase()) {
                Some(parent) => format!("{}/{name}", parent.name),
                None => tag.to_string(),
            },
            None => tag.to_string(),
        }
    }

    fn unindex_note(&mut self, path: &str) {
        let Some(entry) = self.notes.remove(path) else {
            return;
        };
        self.unlink(path, &entry);
        for key in link_keys(&entry.parsed.links) {
            if let Some(sources) = self.by_key.get_mut(&key) {
                sources.remove(path);
                if sources.is_empty() {
                    self.by_key.remove(&key);
                }
            }
        }
        for tag in tag_prefixes(&entry.parsed.tags) {
            let key = tag.to_lowercase();
            if let Some(count) = self.tags.get_mut(&key) {
                count.notes -= 1;
                if count.notes == 0 {
                    self.tags.remove(&key);
                }
            }
        }
    }

    /// Drops `path`'s resolved links from the backlinks.
    fn unlink(&mut self, path: &str, entry: &NoteEntry) {
        for target in entry.resolved.iter().flatten() {
            if let Some(sources) = self.backlinks.get_mut(target) {
                sources.remove(path);
                if sources.is_empty() {
                    self.backlinks.remove(target);
                }
            }
        }
    }

    /// Resolves again the links of every note that names a file with
    /// `key`, after such a file came or went. `except` was just indexed.
    fn reresolve_key(&mut self, key: &str, except: Option<&str>) {
        let Some(sources) = self.by_key.get(key) else {
            return;
        };
        let sources: Vec<String> = sources
            .iter()
            .filter(|source| Some(source.as_str()) != except)
            .cloned()
            .collect();
        for source in sources {
            let Some(entry) = self.notes.get(&source).cloned() else {
                continue;
            };
            self.unlink(&source, &entry);
            let resolved: Vec<Option<String>> = entry
                .parsed
                .links
                .iter()
                .map(|link| self.resolve(&source, link))
                .collect();
            for target in resolved.iter().flatten() {
                self.backlinks
                    .entry(target.clone())
                    .or_default()
                    .insert(source.clone());
            }
            if let Some(entry) = self.notes.get_mut(&source) {
                entry.resolved = resolved;
            }
        }
    }

    // ---- Resolving ----

    /// The file `link` in the note at `source` points to, if it exists.
    pub fn resolve(&self, source: &str, link: &Link) -> Option<String> {
        self.files.resolve(source, link)
    }

    // ---- Queries ----

    /// The notes linking to `target`, by path, each with its links there.
    pub fn backlinks(&self, target: &str) -> Vec<Backlink<'_>> {
        let Some(sources) = self.backlinks.get(target) else {
            return Vec::new();
        };
        sources
            .iter()
            .filter_map(|source| {
                let entry = self.notes.get(source)?;
                let links = entry
                    .parsed
                    .links
                    .iter()
                    .zip(&entry.resolved)
                    .filter(|(_, to)| to.as_deref() == Some(target))
                    .map(|(link, _)| link)
                    .collect();
                Some(Backlink { source, links })
            })
            .collect()
    }

    /// The notes linking to `target`.
    pub fn linking_to(&self, target: &str) -> Vec<String> {
        self.backlinks
            .get(target)
            .map(|sources| sources.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Links that go nowhere, by what they name, with the notes that have
    /// them. Embeds of missing files count too.
    pub fn unresolved(&self) -> BTreeMap<String, BTreeSet<String>> {
        let mut missing: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (path, entry) in &self.notes {
            for (link, to) in entry.parsed.links.iter().zip(&entry.resolved) {
                if to.is_none() {
                    missing
                        .entry(link.target.clone())
                        .or_default()
                        .insert(path.clone());
                }
            }
        }
        missing
    }

    /// The notes tagged `tag` or a tag nested under it, ignoring case,
    /// whether in their text or only in their frontmatter.
    pub fn notes_tagged(&self, tag: &str) -> HashSet<String> {
        let tag = tag.to_lowercase();
        let nested = format!("{tag}/");
        self.notes
            .iter()
            .filter(|(_, entry)| {
                entry.parsed.tags.iter().any(|written| {
                    let name = written.name.to_lowercase();
                    name == tag || name.starts_with(&nested)
                })
            })
            .map(|(path, _)| path.clone())
            .collect()
    }

    /// Every tag and parent tag with its note count, sorted by name.
    pub fn tags(&self) -> Vec<TagCount> {
        let mut tags: Vec<TagCount> = self.tags.values().cloned().collect();
        tags.sort_by_key(|tag| tag.name.to_lowercase());
        tags
    }
}

/// The distinct name keys of `links`.
fn link_keys(links: &[Link]) -> Vec<String> {
    let mut keys: Vec<String> = links.iter().map(|link| name_key(&link.target)).collect();
    keys.sort_unstable();
    keys.dedup();
    keys
}

/// Each item once, in first-seen order. Lists here are short, so a scan
/// beats hashing.
fn distinct<'a>(items: impl Iterator<Item = &'a String>) -> Vec<&'a String> {
    let mut seen: Vec<&String> = Vec::new();
    for item in items {
        if !seen.contains(&item) {
            seen.push(item);
        }
    }
    seen
}

/// A note's tags and all their parents (`a/b/c` gives `a`, `a/b` and
/// `a/b/c`), each once however often it's written.
fn tag_prefixes(tags: &[Tag]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for tag in tags {
        let mut end = 0;
        for part in tag.name.split('/') {
            end += part.len();
            let prefix = &tag.name[..end];
            if seen.insert(prefix.to_lowercase()) {
                out.push(prefix.to_string());
            }
            end += 1;
        }
    }
    out
}

/// A note read and parsed, with where each of its links goes.
pub type ResolvedNote = (String, Arc<str>, ParsedNote, Vec<Option<String>>);

/// Every file by lowercased path and lowercased file name, since Obsidian
/// matches links without regard to case. Links resolve against it.
#[derive(Clone, Debug, Default)]
pub struct FileNames {
    by_path: HashMap<String, String>,
    by_name: HashMap<String, Vec<String>>,
}

impl FileNames {
    pub fn new<'a>(files: impl IntoIterator<Item = &'a String>) -> FileNames {
        let mut names = FileNames::default();
        for file in files {
            names.insert(file);
        }
        names
    }

    /// Where each of `links` in the note at `source` goes. Notes link to
    /// the same few targets over and over, so each distinct one is
    /// resolved once.
    pub fn resolve_all(&self, source: &str, links: &[Link]) -> Vec<Option<String>> {
        let mut seen: Vec<(&Link, Option<String>)> = Vec::new();
        links
            .iter()
            .map(|link| {
                let known = seen.iter().find(|(known, _)| {
                    known.markdown == link.markdown && known.target == link.target
                });
                if let Some((_, found)) = known {
                    return found.clone();
                }
                let found = self.resolve(source, link);
                seen.push((link, found.clone()));
                found
            })
            .collect()
    }

    /// The file `link` in the note at `source` points to, if it exists.
    pub fn resolve(&self, source: &str, link: &Link) -> Option<String> {
        let dir = parent_dir(source);
        if link.markdown {
            self.resolve_destination(dir, &link.target)
        } else {
            self.resolve_linkpath(dir, &link.target)
        }
    }

    /// A wikilink path: relative paths from the note's folder, then exact
    /// vault paths, then the closest file whose path ends with it.
    fn resolve_linkpath(&self, dir: &str, target: &str) -> Option<String> {
        candidate_forms(target).into_iter().find_map(|form| {
            if form.starts_with("./") || form.starts_with("../") {
                return self.exact(&normalize(&join(dir, &form))?);
            }
            self.exact(&form).or_else(|| self.by_suffix(dir, &form))
        })
    }

    /// A Markdown destination: from the root when it starts with `/`,
    /// else relative to the note first, then as a linkpath.
    fn resolve_destination(&self, dir: &str, path: &str) -> Option<String> {
        if let Some(rooted) = path.strip_prefix('/') {
            return candidate_forms(rooted)
                .into_iter()
                .find_map(|form| self.exact(&normalize(&form)?));
        }
        candidate_forms(path)
            .into_iter()
            .find_map(|form| self.exact(&normalize(&join(dir, &form))?))
            .or_else(|| self.resolve_linkpath(dir, path))
    }

    /// Adds `path`; false when it was already there.
    pub fn insert(&mut self, path: &str) -> bool {
        let lower = path.to_lowercase();
        if self.by_path.contains_key(&lower) {
            return false;
        }
        self.by_path.insert(lower, path.to_string());
        self.by_name
            .entry(file_name(path).to_lowercase())
            .or_default()
            .push(path.to_string());
        true
    }

    fn remove(&mut self, path: &str) -> bool {
        if self.by_path.remove(&path.to_lowercase()).is_none() {
            return false;
        }
        let name = file_name(path).to_lowercase();
        if let Some(files) = self.by_name.get_mut(&name) {
            files.retain(|file| file != path);
            if files.is_empty() {
                self.by_name.remove(&name);
            }
        }
        true
    }

    fn all(&self) -> impl Iterator<Item = &str> {
        self.by_path.values().map(String::as_str)
    }

    fn exact(&self, path: &str) -> Option<String> {
        self.by_path.get(&path.to_lowercase()).cloned()
    }

    /// The file whose path ends with `suffix`, preferring the note's own
    /// folder, then the shortest path.
    fn by_suffix(&self, dir: &str, suffix: &str) -> Option<String> {
        let lower = suffix.to_lowercase();
        let ending = format!("/{lower}");
        self.by_name
            .get(file_name(&lower))?
            .iter()
            .filter(|file| {
                let file = file.to_lowercase();
                file == lower || file.ends_with(&ending)
            })
            .min_by_key(|file| (parent_dir(file) != dir, file.matches('/').count(), *file))
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(notes: &[(&str, &str)], others: &[&str]) -> LinkIndex {
        let files = others.iter().map(|f| f.to_string()).collect();
        let notes = notes
            .iter()
            .map(|(path, text)| (path.to_string(), Arc::from(*text), parse_note(text)))
            .collect();
        LinkIndex::build(files, notes)
    }

    fn sources(index: &LinkIndex, target: &str) -> Vec<String> {
        index.linking_to(target)
    }

    #[test]
    fn links_resolve_like_obsidian() {
        let index = index(
            &[
                (
                    "Plan.md",
                    "[[Roadmap]] [[Projects/Roadmap#Q1|road]] ![[chart.png]]",
                ),
                ("Projects/Roadmap.md", "[back](../Plan.md) [[plan]]"),
                ("Archive/Roadmap.md", "[[Roadmap]]"),
            ],
            &["Projects/images/chart.png"],
        );
        // Plain names prefer the linking note's folder, then the shortest
        // path, then the first by name.
        assert_eq!(
            sources(&index, "Archive/Roadmap.md"),
            ["Archive/Roadmap.md", "Plan.md"]
        );
        assert_eq!(sources(&index, "Projects/Roadmap.md"), ["Plan.md"]);
        assert_eq!(sources(&index, "Plan.md"), ["Projects/Roadmap.md"]);
        assert_eq!(sources(&index, "Projects/images/chart.png"), ["Plan.md"]);
        let backlinks = index.backlinks("Plan.md");
        assert_eq!(backlinks[0].links.len(), 2);
    }

    #[test]
    fn unresolved_links_resolve_when_the_note_appears() {
        let mut index = index(&[("A.md", "[[Missing]] and [[B]]"), ("B.md", "")], &[]);
        assert_eq!(
            index.unresolved().keys().collect::<Vec<_>>(),
            vec!["Missing"]
        );
        index.set_note("Folder/Missing.md", "hi");
        assert!(index.unresolved().is_empty());
        assert_eq!(sources(&index, "Folder/Missing.md"), ["A.md"]);
        index.remove("Folder");
        assert_eq!(index.unresolved().len(), 1);
        assert!(sources(&index, "Folder/Missing.md").is_empty());
    }

    #[test]
    fn editing_a_note_replaces_its_links_and_tags() {
        let mut index = index(&[("A.md", "[[B]] #one"), ("B.md", "#one #two/three")], &[]);
        let names = |index: &LinkIndex| -> Vec<(String, usize)> {
            index
                .tags()
                .into_iter()
                .map(|t| (t.name, t.notes))
                .collect()
        };
        assert_eq!(
            names(&index),
            [
                ("one".into(), 2),
                ("two".into(), 1),
                ("two/three".into(), 1)
            ]
        );
        index.set_note("A.md", "no links now #TWO/four");
        assert!(sources(&index, "B.md").is_empty());
        assert_eq!(
            names(&index),
            [
                ("one".into(), 1),
                ("two".into(), 2),
                ("two/four".into(), 1),
                ("two/three".into(), 1)
            ]
        );
    }

    #[test]
    fn renames_move_links_without_rereading() {
        let mut index = index(&[("A.md", "[[B]]"), ("Old/B.md", "[[A]]")], &[]);
        index.rename("Old", "New");
        assert_eq!(sources(&index, "New/B.md"), ["A.md"]);
        assert_eq!(sources(&index, "A.md"), ["New/B.md"]);
        assert!(index.note("Old/B.md").is_none());
        index.rename("A.md", "Z.md");
        // `[[A]]` in B no longer resolves; A's own link still does.
        assert_eq!(index.unresolved().keys().collect::<Vec<_>>(), vec!["A"]);
        assert_eq!(sources(&index, "New/B.md"), ["Z.md"]);
    }

    #[test]
    fn a_closer_file_takes_over_a_name() {
        let mut index = index(&[("Sub/A.md", "[[B]]"), ("Other/B.md", "")], &[]);
        assert_eq!(sources(&index, "Other/B.md"), ["Sub/A.md"]);
        index.set_note("Sub/B.md", "");
        assert_eq!(sources(&index, "Sub/B.md"), ["Sub/A.md"]);
        assert!(sources(&index, "Other/B.md").is_empty());
    }
}
