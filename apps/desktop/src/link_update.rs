//! Rewriting links when notes, attachments or folders move, so renaming a
//! file keeps every link to it working.
//!
//! Paths are vault-relative and `/`-separated, such as `Projects/Plan.md`.
//! Wikilinks (`[[Plan]]`, `[[Plan#Goals|the plan]]`, `![[chart.png]]`) and
//! Markdown links (`[the plan](Projects/Plan.md)`) are both rewritten, in the
//! same style they were written in. Links inside code are left alone.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;

const NOTE_EXTENSION: &str = ".md";

/// Every file under `from` moved to the same place under `to`, for a
/// folder rename. A plain file rename is just `[(from, to)]`.
pub fn expand_folder_move(files: &[String], from: &str, to: &str) -> Vec<(String, String)> {
    let prefix = format!("{}/", from.trim_end_matches('/'));
    let target = to.trim_end_matches('/');
    files
        .iter()
        .filter_map(|file| {
            let rest = file.strip_prefix(&prefix)?;
            Some((file.clone(), format!("{target}/{rest}")))
        })
        .collect()
}

/// Rewrites links for one set of moves, one note at a time.
pub struct LinkUpdater {
    moves: BTreeMap<String, String>,
    before: FileIndex,
    after: FileIndex,
    needles: Vec<String>,
}

impl LinkUpdater {
    /// `files` is every file in the vault before the moves, and `moves`
    /// pairs each moved file's old path with its new one.
    pub fn new(files: &[String], moves: &[(String, String)]) -> LinkUpdater {
        let moves: BTreeMap<String, String> = moves.iter().cloned().collect();
        let after: Vec<String> = files
            .iter()
            .map(|file| moves.get(file).unwrap_or(file).clone())
            .collect();
        let needles = moves.keys().flat_map(|old| needles_for(old)).collect();
        LinkUpdater {
            before: FileIndex::new(files),
            after: FileIndex::new(&after),
            moves,
            needles,
        }
    }

    /// The note's text with its links updated, or `None` when nothing
    /// changed. `note` is where the note was before the moves.
    pub fn rewrite(&self, note: &str, text: &str) -> Option<String> {
        if !self.moves.contains_key(note) && !self.may_link(text) {
            return None;
        }
        let context = NoteContext {
            old_dir: parent_dir(note).to_string(),
            new_dir: parent_dir(self.moves.get(note).map_or(note, String::as_str)).to_string(),
            moved: self.moves.contains_key(note),
        };
        let edits = self.edits(&context, text);
        (!edits.is_empty()).then(|| apply_edits(text, edits))
    }

    /// A cheap check that `text` might mention a moved file.
    fn may_link(&self, text: &str) -> bool {
        let lower = text.to_lowercase();
        self.needles.iter().any(|needle| lower.contains(needle))
    }

    fn edits(&self, context: &NoteContext, text: &str) -> Vec<(Range<usize>, String)> {
        let code = code_ranges(text);
        let in_code = |at: usize| code.iter().any(|range| range.contains(&at));
        let mut edits = Vec::new();
        for link in find_wikilinks(text)
            .into_iter()
            .filter(|l| !in_code(l.start))
        {
            if let Some(new) = self.new_wikilink_target(context, &text[link.clone()]) {
                edits.push((link, new));
            }
        }
        for dest in find_markdown_destinations(text)
            .into_iter()
            .filter(|d| !in_code(d.start))
        {
            if let Some(new) = self.new_markdown_destination(context, &text[dest.clone()]) {
                edits.push((dest, new));
            }
        }
        edits
    }

    /// The new text of a wikilink's target (the part before `#` or `|`).
    fn new_wikilink_target(&self, context: &NoteContext, target: &str) -> Option<String> {
        let target = target.trim();
        if target.is_empty() {
            return None;
        }
        let resolved = self.resolve_linkpath(&context.old_dir, target)?;
        let style = LinkStyle::of_linkpath(target);
        let new = self.retarget(
            context,
            &resolved,
            style,
            has_note_extension(target, &resolved),
        )?;
        (new != target).then_some(new)
    }

    /// The new destination of a Markdown link, or `None` to keep it.
    fn new_markdown_destination(&self, context: &NoteContext, raw: &str) -> Option<String> {
        let dest = Destination::parse(raw)?;
        let (resolved, style) = self.resolve_destination(&context.old_dir, &dest.path)?;
        let with_extension = has_note_extension(&dest.path, &resolved);
        let new_path = self.retarget(context, &resolved, style, with_extension)?;
        if new_path == dest.path {
            return None;
        }
        Some(dest.render(&new_path))
    }

    /// Where a link to `resolved` should point after the moves, or `None`
    /// when neither the note nor the target moved in a way that matters.
    fn retarget(
        &self,
        context: &NoteContext,
        resolved: &str,
        style: LinkStyle,
        with_extension: bool,
    ) -> Option<String> {
        let target_moved = self.moves.get(resolved);
        if target_moved.is_none() && !(context.moved && style == LinkStyle::Relative) {
            return None;
        }
        let new_target = target_moved.map_or(resolved, String::as_str);
        let written = match style {
            LinkStyle::Relative => relative_path(&context.new_dir, new_target),
            LinkStyle::Absolute => format!("/{new_target}"),
            LinkStyle::Path => new_target.to_string(),
            LinkStyle::Name => self.shortest_linkpath(new_target),
        };
        Some(if with_extension {
            written
        } else {
            strip_note_extension(&written).to_string()
        })
    }

    /// The file name when it's unique after the moves, else the full path.
    fn shortest_linkpath(&self, path: &str) -> String {
        let name = file_name(path);
        if self.after.by_name(name).len() > 1 {
            path.to_string()
        } else {
            name.to_string()
        }
    }

    /// Resolves a wikilink-style path the way Obsidian does: relative
    /// paths from the note's folder, then exact vault paths, then the
    /// closest file whose path ends with it.
    fn resolve_linkpath(&self, note_dir: &str, target: &str) -> Option<String> {
        candidate_forms(target).into_iter().find_map(|form| {
            if form.starts_with("./") || form.starts_with("../") {
                let joined = normalize(&join(note_dir, &form))?;
                return self.before.exact(&joined);
            }
            self.before
                .exact(&form)
                .or_else(|| self.before.by_suffix(note_dir, &form))
        })
    }

    /// Resolves a Markdown link destination: relative to the note first,
    /// then as a linkpath.
    fn resolve_destination(&self, note_dir: &str, path: &str) -> Option<(String, LinkStyle)> {
        if let Some(rooted) = path.strip_prefix('/') {
            let found = candidate_forms(rooted)
                .into_iter()
                .find_map(|form| self.before.exact(&normalize(&form)?))?;
            return Some((found, LinkStyle::Absolute));
        }
        let relative = candidate_forms(path).into_iter().find_map(|form| {
            let joined = normalize(&join(note_dir, &form))?;
            self.before.exact(&joined)
        });
        if let Some(found) = relative {
            return Some((found, LinkStyle::Relative));
        }
        let found = self.resolve_linkpath(note_dir, path)?;
        Some((found, LinkStyle::of_linkpath(path)))
    }
}

/// Where the note being rewritten lives, before and after the moves.
struct NoteContext {
    old_dir: String,
    new_dir: String,
    moved: bool,
}

/// How a link names its target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinkStyle {
    /// `./Plan.md` or `../Plan.md`, from the note's folder.
    Relative,
    /// `/Projects/Plan.md`, from the vault root.
    Absolute,
    /// `Projects/Plan`, a vault path.
    Path,
    /// `Plan`, just the name.
    Name,
}

impl LinkStyle {
    fn of_linkpath(target: &str) -> LinkStyle {
        if target.starts_with("./") || target.starts_with("../") {
            LinkStyle::Relative
        } else if target.contains('/') {
            LinkStyle::Path
        } else {
            LinkStyle::Name
        }
    }
}

/// Files by lowercased path and lowercased file name, since Obsidian
/// matches links without regard to case.
struct FileIndex {
    by_path: HashMap<String, String>,
    by_name: HashMap<String, Vec<String>>,
}

impl FileIndex {
    fn new(files: &[String]) -> FileIndex {
        let mut by_name: HashMap<String, Vec<String>> = HashMap::new();
        for file in files {
            by_name
                .entry(file_name(file).to_lowercase())
                .or_default()
                .push(file.clone());
        }
        FileIndex {
            by_path: files
                .iter()
                .map(|f| (f.to_lowercase(), f.clone()))
                .collect(),
            by_name,
        }
    }

    fn exact(&self, path: &str) -> Option<String> {
        self.by_path.get(&path.to_lowercase()).cloned()
    }

    fn by_name(&self, name: &str) -> &[String] {
        self.by_name
            .get(&name.to_lowercase())
            .map_or(&[], Vec::as_slice)
    }

    /// The file whose path ends with `suffix`, preferring the note's own
    /// folder, then the shortest path.
    fn by_suffix(&self, note_dir: &str, suffix: &str) -> Option<String> {
        let lower = suffix.to_lowercase();
        let ending = format!("/{lower}");
        let mut matches: Vec<&String> = self
            .by_name(file_name(suffix))
            .iter()
            .filter(|file| {
                let file = file.to_lowercase();
                file == lower || file.ends_with(&ending)
            })
            .collect();
        matches.sort_by_key(|file| {
            (
                parent_dir(file) != note_dir,
                file.matches('/').count(),
                *file,
            )
        });
        matches.first().map(|file| (*file).clone())
    }
}

/// The paths a link could mean: as written, and with `.md` added.
pub(crate) fn candidate_forms(target: &str) -> Vec<String> {
    let mut forms = vec![target.to_string()];
    if !target.to_lowercase().ends_with(NOTE_EXTENSION) {
        forms.push(format!("{target}{NOTE_EXTENSION}"));
    }
    forms
}

/// Whether the link spelled out `.md`, or points at a file that isn't a note.
fn has_note_extension(written: &str, resolved: &str) -> bool {
    written.to_lowercase().ends_with(NOTE_EXTENSION)
        || !resolved.to_lowercase().ends_with(NOTE_EXTENSION)
}

pub(crate) fn strip_note_extension(path: &str) -> &str {
    let cut = path.len().saturating_sub(NOTE_EXTENSION.len());
    match path.get(cut..) {
        Some(tail) if tail.eq_ignore_ascii_case(NOTE_EXTENSION) => &path[..cut],
        _ => path,
    }
}

/// Lowercased strings whose presence means a note may link to `path`.
fn needles_for(path: &str) -> Vec<String> {
    let stem = strip_note_extension(file_name(path));
    let mut needles = vec![
        stem.to_lowercase(),
        percent_encode(stem, false).to_lowercase(),
        percent_encode(stem, true).to_lowercase(),
    ];
    needles.dedup();
    needles
}

pub(crate) fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub(crate) fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

pub(crate) fn join(dir: &str, path: &str) -> String {
    if dir.is_empty() {
        path.to_string()
    } else {
        format!("{dir}/{path}")
    }
}

/// Resolves `.` and `..`. `None` when the path climbs out of the vault.
pub(crate) fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

/// The path from folder `from` to `to`, such as `../Archive/Plan.md`.
fn relative_path(from: &str, to: &str) -> String {
    let from_parts: Vec<&str> = from.split('/').filter(|p| !p.is_empty()).collect();
    let to_parts: Vec<&str> = to.split('/').collect();
    let shared = from_parts
        .iter()
        .zip(&to_parts)
        .take_while(|(a, b)| a == b)
        .count()
        .min(to_parts.len().saturating_sub(1));
    let ups = std::iter::repeat_n("..", from_parts.len() - shared);
    let downs = to_parts[shared..].iter().copied();
    ups.chain(downs).collect::<Vec<_>>().join("/")
}

/// A Markdown link destination split into the parts a rewrite keeps.
pub(crate) struct Destination {
    /// The decoded path, without the fragment.
    pub(crate) path: String,
    pub(crate) fragment: String,
    angle_brackets: bool,
    /// Whether characters other than spaces were percent-encoded.
    fully_encoded: bool,
}

impl Destination {
    pub(crate) fn parse(raw: &str) -> Option<Destination> {
        let angle_brackets = raw.starts_with('<') && raw.ends_with('>') && raw.len() >= 2;
        let inner = if angle_brackets {
            &raw[1..raw.len() - 1]
        } else {
            raw
        };
        if is_external(inner) {
            return None;
        }
        let (path, fragment) = match inner.find('#') {
            Some(at) => (&inner[..at], &inner[at..]),
            None => (inner, ""),
        };
        let decoded = percent_decode(path)?;
        Some(Destination {
            fully_encoded: path.replace("%20", " ").contains('%'),
            path: decoded,
            fragment: fragment.to_string(),
            angle_brackets,
        })
    }

    fn render(&self, path: &str) -> String {
        if self.angle_brackets {
            return format!("<{path}{}>", self.fragment);
        }
        format!(
            "{}{}",
            percent_encode(path, self.fully_encoded),
            self.fragment
        )
    }
}

const EXTERNAL_PREFIXES: [&str; 3] = ["mailto:", "obsidian:", "data:"];

fn is_external(dest: &str) -> bool {
    dest.is_empty()
        || dest.starts_with('#')
        || dest.contains("://")
        || EXTERNAL_PREFIXES.iter().any(|p| dest.starts_with(p))
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = (bytes[at] == b'%')
            .then(|| text.get(at + 1..at + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match hex {
            Some(byte) => {
                out.push(byte);
                at += 3;
            }
            None => {
                out.push(bytes[at]);
                at += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// Encodes spaces (and, when `fully`, everything but unreserved ASCII and `/`).
fn percent_encode(path: &str, fully: bool) -> String {
    let mut out = String::with_capacity(path.len());
    for ch in path.chars() {
        let plain = ch != ' ' && (!fully || ch.is_ascii_alphanumeric() || "-._~/".contains(ch));
        if plain {
            out.push(ch);
            continue;
        }
        let mut buffer = [0; 4];
        for byte in ch.encode_utf8(&mut buffer).bytes() {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The byte ranges of every wikilink's target, the part before `#` or `|`.
fn find_wikilinks(text: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(open) = text[from..].find("[[").map(|at| at + from) {
        let body_start = open + 2;
        let Some(close) = text[body_start..].find("]]").map(|at| at + body_start) else {
            break;
        };
        let body = &text[body_start..close];
        if !body.contains('\n') && !body.contains("[[") {
            found.push(body_start..body_start + wikilink_target_len(body));
            from = close + 2;
        } else {
            from = body_start;
        }
    }
    found
}

/// The length of a wikilink body's target, stopping at `#`, `|` or `\|`.
fn wikilink_target_len(body: &str) -> usize {
    let end = body.find(['#', '|']).unwrap_or(body.len());
    let target = &body[..end];
    target.strip_suffix('\\').map_or(end, str::len)
}

/// The byte ranges of every Markdown link destination, after `](`.
pub(crate) fn find_markdown_destinations(text: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = text[from..].find("](").map(|at| at + from) {
        let start = at + 2;
        if let Some(len) = destination_len(&text[start..]) {
            found.push(start..start + len);
        }
        from = start;
    }
    found
}

/// The length of a link destination at the start of `rest`: `<…>`, or up
/// to whitespace or the closing parenthesis, keeping balanced parentheses.
fn destination_len(rest: &str) -> Option<usize> {
    if rest.starts_with('<') {
        return rest
            .find(['>', '\n'])
            .filter(|end| rest.as_bytes()[*end] == b'>')
            .map(|end| end + 1);
    }
    let mut depth = 0usize;
    for (at, ch) in rest.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' if depth == 0 => return (at > 0).then_some(at),
            ')' => depth -= 1,
            c if c.is_whitespace() => return (at > 0).then_some(at),
            _ => {}
        }
    }
    None
}

/// Byte ranges of fenced code blocks and inline code spans.
pub(crate) fn code_ranges(text: &str) -> Vec<Range<usize>> {
    let fences = fenced_blocks(text);
    let mut ranges = fences.clone();
    let mut start = 0;
    for fence in fences
        .iter()
        .chain(std::iter::once(&(text.len()..text.len())))
    {
        ranges.extend(
            inline_code_spans(&text[start..fence.start])
                .into_iter()
                .map(|span| span.start + start..span.end + start),
        );
        start = fence.end;
    }
    ranges
}

/// A fence opener: the fence character and how many.
fn fence_of(line: &str) -> Option<(char, usize)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let trimmed = &line[indent..];
    let ch = trimmed.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let count = trimmed.len() - trimmed.trim_start_matches(ch).len();
    (indent <= 3 && count >= 3).then_some((ch, count))
}

fn fenced_blocks(text: &str) -> Vec<Range<usize>> {
    let mut blocks = Vec::new();
    let mut open: Option<(usize, char, usize)> = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let fence = fence_of(line.trim_end_matches(['\n', '\r']));
        match (open, fence) {
            (None, Some((ch, count))) => open = Some((offset, ch, count)),
            (Some((start, ch, count)), Some((close_ch, close_count)))
                if close_ch == ch && close_count >= count =>
            {
                blocks.push(start..offset + line.len());
                open = None;
            }
            _ => {}
        }
        offset += line.len();
    }
    if let Some((start, ..)) = open {
        blocks.push(start..text.len());
    }
    blocks
}

/// Inline code spans: a run of backticks closed by a run of the same length.
fn inline_code_spans(text: &str) -> Vec<Range<usize>> {
    let runs = backtick_runs(text);
    let mut spans = Vec::new();
    let mut used = HashSet::new();
    for (index, open) in runs.iter().enumerate() {
        if used.contains(&index) {
            continue;
        }
        let close = runs
            .iter()
            .enumerate()
            .skip(index + 1)
            .find(|(_, run)| run.len() == open.len());
        if let Some((close_index, close)) = close {
            spans.push(open.start..close.end);
            used.extend(index..=close_index);
        }
    }
    spans
}

fn backtick_runs(text: &str) -> Vec<Range<usize>> {
    let mut runs: Vec<Range<usize>> = Vec::new();
    for (at, ch) in text.char_indices().filter(|(_, ch)| *ch == '`') {
        match runs.last_mut() {
            Some(run) if run.end == at => run.end = at + ch.len_utf8(),
            _ => runs.push(at..at + 1),
        }
    }
    runs
}

fn apply_edits(text: &str, mut edits: Vec<(Range<usize>, String)>) -> String {
    edits.sort_by_key(|(range, _)| range.start);
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    for (range, replacement) in edits {
        if range.start < copied {
            continue;
        }
        out.push_str(&text[copied..range.start]);
        out.push_str(&replacement);
        copied = range.end;
    }
    out.push_str(&text[copied..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const VAULT: &[&str] = &[
        "Plan.md",
        "Projects/Roadmap.md",
        "Projects/Notes on design.md",
        "Projects/images/chart.png",
        "Archive/Old idea.md",
        "Daily/2024-01-01.md",
        "Daily/Roadmap.md",
        "images/photo one.jpg",
        "Reading/Café.md",
    ];

    fn files() -> Vec<String> {
        VAULT.iter().map(|s| s.to_string()).collect()
    }

    fn rename(from: &str, to: &str) -> LinkUpdater {
        LinkUpdater::new(&files(), &[(from.to_string(), to.to_string())])
    }

    fn rewritten(updater: &LinkUpdater, note: &str, text: &str) -> String {
        updater
            .rewrite(note, text)
            .unwrap_or_else(|| text.to_string())
    }

    #[test]
    fn plain_wikilinks_follow_a_rename() {
        let updater = rename("Plan.md", "Master plan.md");
        assert_eq!(
            rewritten(&updater, "Daily/2024-01-01.md", "See [[Plan]] today."),
            "See [[Master plan]] today."
        );
    }

    #[test]
    fn aliases_headings_and_blocks_are_kept() {
        let updater = rename("Plan.md", "Master plan.md");
        let text = "[[Plan|the plan]] [[Plan#Goals]] [[Plan#Goals|goals]] [[Plan#^abc123]]";
        assert_eq!(
            rewritten(&updater, "Daily/2024-01-01.md", text),
            "[[Master plan|the plan]] [[Master plan#Goals]] [[Master plan#Goals|goals]] [[Master plan#^abc123]]"
        );
    }

    #[test]
    fn escaped_pipes_in_tables_are_kept() {
        let updater = rename("Plan.md", "Master plan.md");
        assert_eq!(
            rewritten(&updater, "Daily/2024-01-01.md", "| [[Plan\\|plan]] |"),
            "| [[Master plan\\|plan]] |"
        );
    }

    #[test]
    fn embeds_of_attachments_keep_their_extension() {
        let updater = rename(
            "Projects/images/chart.png",
            "Projects/images/sales chart.png",
        );
        let text = "![[chart.png]] and ![[chart.png|300]] and ![[Projects/images/chart.png]]";
        assert_eq!(
            rewritten(&updater, "Projects/Roadmap.md", text),
            "![[sales chart.png]] and ![[sales chart.png|300]] and ![[Projects/images/sales chart.png]]"
        );
    }

    #[test]
    fn written_extensions_are_kept() {
        let updater = rename("Plan.md", "Master plan.md");
        assert_eq!(
            rewritten(&updater, "Daily/2024-01-01.md", "[[Plan.md]]"),
            "[[Master plan.md]]"
        );
    }

    #[test]
    fn path_links_get_the_new_path() {
        let updater = rename("Archive/Old idea.md", "Projects/Old idea.md");
        assert_eq!(
            rewritten(&updater, "Plan.md", "[[Archive/Old idea]] and [[Old idea]]"),
            "[[Projects/Old idea]] and [[Old idea]]"
        );
    }

    #[test]
    fn links_match_without_regard_to_case() {
        let updater = rename("Plan.md", "Master plan.md");
        assert_eq!(
            rewritten(&updater, "Daily/2024-01-01.md", "[[plan]]"),
            "[[Master plan]]"
        );
    }

    #[test]
    fn ambiguous_names_resolve_to_the_nearest_file() {
        // Daily/Roadmap.md is closer to a note in Daily.
        let updater = rename("Projects/Roadmap.md", "Projects/Roadmap 2025.md");
        assert_eq!(
            rewritten(&updater, "Daily/2024-01-01.md", "[[Roadmap]]"),
            "[[Roadmap]]"
        );
        assert_eq!(
            rewritten(&updater, "Projects/Notes on design.md", "[[Roadmap]]"),
            "[[Roadmap 2025]]"
        );
    }

    #[test]
    fn a_name_that_becomes_ambiguous_uses_the_full_path() {
        let updater = rename("Archive/Old idea.md", "Archive/Roadmap.md");
        assert_eq!(
            rewritten(&updater, "Plan.md", "[[Old idea]]"),
            "[[Archive/Roadmap]]"
        );
    }

    #[test]
    fn links_to_other_notes_are_untouched() {
        let updater = rename("Plan.md", "Master plan.md");
        assert_eq!(
            updater.rewrite("Daily/2024-01-01.md", "[[Roadmap]] [[Planning]] [[Plan B]]"),
            None
        );
    }

    #[test]
    fn self_heading_links_are_untouched() {
        let updater = rename("Plan.md", "Master plan.md");
        assert_eq!(updater.rewrite("Plan.md", "[[#Goals]]"), None);
    }

    #[test]
    fn markdown_links_are_relative_to_the_note() {
        let updater = rename("Projects/Roadmap.md", "Projects/Roadmap 2025.md");
        assert_eq!(
            rewritten(&updater, "Plan.md", "[road](Projects/Roadmap.md)"),
            "[road](Projects/Roadmap%202025.md)"
        );
        assert_eq!(
            rewritten(
                &updater,
                "Archive/Old idea.md",
                "[road](../Projects/Roadmap.md#Q1)"
            ),
            "[road](../Projects/Roadmap%202025.md#Q1)"
        );
    }

    #[test]
    fn markdown_links_keep_their_encoding_style() {
        let updater = rename("Projects/Notes on design.md", "Projects/Design notes.md");
        assert_eq!(
            rewritten(
                &updater,
                "Projects/Roadmap.md",
                "[d](Notes%20on%20design.md)"
            ),
            "[d](Design%20notes.md)"
        );
        assert_eq!(
            rewritten(&updater, "Projects/Roadmap.md", "[d](<Notes on design.md>)"),
            "[d](<Design notes.md>)"
        );
    }

    #[test]
    fn markdown_links_with_titles_keep_the_title() {
        let updater = rename("Plan.md", "Master plan.md");
        assert_eq!(
            rewritten(&updater, "Daily/Roadmap.md", "[p](../Plan.md \"The plan\")"),
            "[p](../Master%20plan.md \"The plan\")"
        );
    }

    #[test]
    fn markdown_image_links_are_rewritten() {
        let updater = rename("images/photo one.jpg", "images/photo two.jpg");
        assert_eq!(
            rewritten(&updater, "Plan.md", "![a photo](images/photo%20one.jpg)"),
            "![a photo](images/photo%20two.jpg)"
        );
    }

    #[test]
    fn non_ascii_names_stay_readable_unless_fully_encoded() {
        let updater = rename("Reading/Café.md", "Reading/Café crème.md");
        assert_eq!(
            rewritten(&updater, "Plan.md", "[c](Reading/Café.md)"),
            "[c](Reading/Café%20crème.md)"
        );
        assert_eq!(
            rewritten(&updater, "Plan.md", "[c](Reading/Caf%C3%A9.md)"),
            "[c](Reading/Caf%C3%A9%20cr%C3%A8me.md)"
        );
    }

    #[test]
    fn root_links_stay_rooted() {
        let updater = rename("Plan.md", "Plans/Plan.md");
        assert_eq!(
            rewritten(&updater, "Daily/Roadmap.md", "[p](/Plan.md)"),
            "[p](/Plans/Plan.md)"
        );
    }

    #[test]
    fn external_links_are_ignored() {
        let updater = rename("Plan.md", "Master plan.md");
        let text = "[a](https://example.com/Plan.md) [b](mailto:plan@example.com) [c](#Plan)";
        assert_eq!(updater.rewrite("Daily/Roadmap.md", text), None);
    }

    #[test]
    fn a_moved_note_fixes_its_own_relative_links() {
        let updater = rename("Plan.md", "Archive/Plan.md");
        let text = "[r](Projects/Roadmap.md) [[Roadmap]] [[./Daily/2024-01-01]]";
        assert_eq!(
            rewritten(&updater, "Plan.md", text),
            "[r](../Projects/Roadmap.md) [[Roadmap]] [[../Daily/2024-01-01]]"
        );
    }

    #[test]
    fn folder_moves_update_paths_inside_them() {
        let moves = expand_folder_move(&files(), "Projects", "Work");
        assert!(moves.contains(&(
            "Projects/images/chart.png".to_string(),
            "Work/images/chart.png".to_string()
        )));
        assert_eq!(moves.len(), 3);
        let updater = LinkUpdater::new(&files(), &moves);
        let text = "[[Projects/Roadmap]] [[Notes on design]] ![[Projects/images/chart.png]] [r](Projects/Roadmap.md)";
        assert_eq!(
            rewritten(&updater, "Plan.md", text),
            "[[Work/Roadmap]] [[Notes on design]] ![[Work/images/chart.png]] [r](Work/Roadmap.md)"
        );
    }

    #[test]
    fn notes_moving_together_keep_links_between_them() {
        let moves = expand_folder_move(&files(), "Projects", "Work");
        let updater = LinkUpdater::new(&files(), &moves);
        let text = "[d](Notes%20on%20design.md) ![c](images/chart.png)";
        assert_eq!(updater.rewrite("Projects/Roadmap.md", text), None);
    }

    #[test]
    fn folder_move_prefix_needs_a_whole_segment() {
        let files = vec!["Pro/a.md".to_string(), "Projects/b.md".to_string()];
        assert_eq!(
            expand_folder_move(&files, "Pro", "Amateur"),
            vec![("Pro/a.md".to_string(), "Amateur/a.md".to_string())]
        );
    }

    #[test]
    fn code_is_left_alone() {
        let updater = rename("Plan.md", "Master plan.md");
        let text = "`[[Plan]]` and\n```\n[[Plan]]\n```\n~~~md\n[x](Plan.md)\n~~~\nbut [[Plan]]";
        assert_eq!(
            rewritten(&updater, "Daily/Roadmap.md", text),
            "`[[Plan]]` and\n```\n[[Plan]]\n```\n~~~md\n[x](Plan.md)\n~~~\nbut [[Master plan]]"
        );
    }

    #[test]
    fn double_backtick_spans_can_hold_single_backticks() {
        let updater = rename("Plan.md", "Master plan.md");
        assert_eq!(
            rewritten(&updater, "Daily/Roadmap.md", "``a ` [[Plan]]`` [[Plan]]"),
            "``a ` [[Plan]]`` [[Master plan]]"
        );
    }

    #[test]
    fn an_unclosed_fence_runs_to_the_end() {
        let updater = rename("Plan.md", "Master plan.md");
        assert_eq!(updater.rewrite("Daily/Roadmap.md", "```\n[[Plan]]\n"), None);
    }

    #[test]
    fn many_links_on_one_line_all_change() {
        let updater = rename("Plan.md", "P.md");
        assert_eq!(
            rewritten(
                &updater,
                "Daily/Roadmap.md",
                "[[Plan]][[Plan]] ![[Plan]] [x](../Plan.md)"
            ),
            "[[P]][[P]] ![[P]] [x](../P.md)"
        );
    }

    #[test]
    fn frontmatter_links_are_updated() {
        let updater = rename("Plan.md", "Master plan.md");
        let text = "---\nup: \"[[Plan]]\"\n---\nbody";
        assert_eq!(
            rewritten(&updater, "Daily/Roadmap.md", text),
            "---\nup: \"[[Master plan]]\"\n---\nbody"
        );
    }

    #[test]
    fn broken_and_unterminated_links_do_not_panic() {
        let updater = rename("Plan.md", "Master plan.md");
        for text in [
            "[[Plan",
            "[x](Plan.md",
            "[[\n]]",
            "[x](<Plan.md",
            "]()",
            "%E2%28(Plan.md)",
            "[[]]",
        ] {
            let _ = updater.rewrite("Daily/Roadmap.md", text);
        }
        assert_eq!(
            rewritten(&updater, "Daily/Roadmap.md", "[[Plan [[Plan]]"),
            "[[Plan [[Master plan]]"
        );
    }

    #[test]
    fn links_without_the_note_extension_stay_without_it() {
        let updater = rename("Plan.md", "Master plan.md");
        assert_eq!(
            rewritten(&updater, "Daily/Roadmap.md", "[p](../Plan)"),
            "[p](../Master%20plan)"
        );
    }

    #[test]
    fn markdown_linkpaths_resolve_like_wikilinks() {
        let updater = rename("Archive/Old idea.md", "Archive/New idea.md");
        assert_eq!(
            rewritten(&updater, "Plan.md", "[i](Old%20idea.md)"),
            "[i](New%20idea.md)"
        );
    }

    #[test]
    fn relative_paths_climb_and_descend() {
        assert_eq!(relative_path("", "a/b.md"), "a/b.md");
        assert_eq!(relative_path("a", "a/b.md"), "b.md");
        assert_eq!(relative_path("a/c", "a/b.md"), "../b.md");
        assert_eq!(relative_path("x/y", "a/b.md"), "../../a/b.md");
        assert_eq!(relative_path("a", "a"), "../a");
    }

    #[test]
    fn normalize_rejects_escaping_the_vault() {
        assert_eq!(normalize("a/../b/./c.md"), Some("b/c.md".to_string()));
        assert_eq!(normalize("../c.md"), None);
    }

    #[test]
    fn untouched_notes_are_skipped_quickly() {
        let updater = rename("Plan.md", "Master plan.md");
        assert!(!updater.may_link("nothing relevant here"));
        assert!(updater.may_link("a [[PLAN]]"));
    }
}
