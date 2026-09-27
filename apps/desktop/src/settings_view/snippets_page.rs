//! The Snippets and replacements page: every snippet in the vault's
//! `.editor/snippets.txt` (or the built-in list) and every replacement in
//! `.editor/replacements.toml`, grouped as their files group them, each
//! with a switch. A snippet opens in place into an editor with a test box
//! that shows what typing gives as you type. Changes are written straight
//! away and reported as `snippets` or `replacements`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use editor_config::loader::CONFIG_DIR;
use editor_snippets::{
    FileLine, InputContext, ReplacementFire, Replacements, Scope, Snippet, SnippetFile,
    format_expansion, format_trigger,
};
use gpui::Context;

use super::model::words_match;
use super::snippet_look::SnippetLook;
use super::store;
use super::view::{ControlRow, PaneLayout, SettingsEvent, SettingsView};

pub const SNIPPETS_FILE: &str = "snippets.txt";
pub const REPLACEMENTS_FILE: &str = "replacements.toml";

/// The key a snippet change is reported and its errors kept under.
pub const SNIPPETS_KEY: &str = "snippets";
/// The key a replacement change is reported and its errors kept under.
pub const REPLACEMENTS_KEY: &str = "replacements";

/// The group snippets before any heading comment fall in.
const UNGROUPED: &str = "Snippets";

/// The heading new snippets go under.
const ADDED_GROUP: &str = "Added in settings";

fn snippets_path(vault_root: &Path) -> PathBuf {
    vault_root.join(CONFIG_DIR).join(SNIPPETS_FILE)
}

fn replacements_path(vault_root: &Path) -> PathBuf {
    vault_root.join(CONFIG_DIR).join(REPLACEMENTS_FILE)
}

/// The snippets and replacements the page lists.
#[derive(Clone, Debug, Default)]
pub struct TypingLists {
    pub snippets: SnippetFile,
    pub snippets_from_vault: bool,
    pub replacements: Replacements,
    pub replacements_from_vault: bool,
    /// Why a vault file couldn't be read. The page says so and doesn't
    /// write over the file.
    pub problem: Option<String>,
}

impl TypingLists {
    /// Reads the vault's files, or takes the built-in lists where there
    /// are none.
    pub fn load(vault_root: &Path) -> TypingLists {
        let mut lists = TypingLists {
            snippets: SnippetFile::builtin(),
            replacements: Replacements::builtin(),
            ..TypingLists::default()
        };
        match read_optional(&snippets_path(vault_root)) {
            Ok(Some(text)) => match SnippetFile::parse(&text) {
                Ok(file) => {
                    lists.snippets = file;
                    lists.snippets_from_vault = true;
                }
                Err(errors) => lists.problem = Some(format!("snippets.txt, {}", errors[0])),
            },
            Ok(None) => {}
            Err(error) => lists.problem = Some(error.to_string()),
        }
        match read_optional(&replacements_path(vault_root)) {
            Ok(Some(text)) => match Replacements::from_toml(&text) {
                Ok(table) => {
                    lists.replacements = table;
                    lists.replacements_from_vault = true;
                }
                Err(error) => lists.problem = Some(format!("replacements.toml: {error}")),
            },
            Ok(None) => {}
            Err(error) => lists.problem = Some(error.to_string()),
        }
        lists
    }
}

fn read_optional(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// One snippet as its row shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct SnippetRow {
    /// The snippet's line in the file.
    pub line: usize,
    pub trigger: String,
    pub expansion: String,
    /// Where and when it fires, in words.
    pub when: String,
    pub on: bool,
    /// What the row shows.
    pub look: SnippetLook,
}

impl SnippetRow {
    fn new(line: usize, snippet: &Snippet) -> SnippetRow {
        SnippetRow {
            line,
            trigger: format_trigger(&snippet.trigger),
            expansion: format_expansion(snippet),
            when: describe(snippet),
            on: !snippet.options.off,
            look: SnippetLook::of_snippet(snippet),
        }
    }
}

/// One replacement as its row shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct ReplacementRow {
    pub index: usize,
    pub from: String,
    pub to: String,
    pub when: String,
    pub on: bool,
    /// What the row shows.
    pub look: SnippetLook,
}

/// Where and when a snippet fires, as a sentence.
fn describe(snippet: &Snippet) -> String {
    let options = &snippet.options;
    let place = match options.scopes.as_slice() {
        [] => "Anywhere".to_string(),
        scopes => {
            let names: Vec<String> = scopes.iter().map(|scope| scope_name(*scope)).collect();
            format!("In {}", names.join(" or "))
        }
    };
    let when = match options.fire {
        editor_snippets::Fire::Instant => "as you type",
        editor_snippets::Fire::OnTab => "on Tab",
    };
    let mut extras = Vec::new();
    if options.whole_word {
        extras.push("as a whole word");
    }
    if options.after_space {
        extras.push("after a space");
    }
    if options.on_selection {
        extras.push("around a selection");
    }
    let extras = if extras.is_empty() {
        String::new()
    } else {
        format!(", {}", extras.join(", "))
    };
    format!("{place}, {when}{extras}.")
}

fn scope_name(scope: Scope) -> String {
    match scope {
        Scope::Context(context) => context.name().to_string(),
        Scope::InlineMath => "inline math".to_string(),
        Scope::BlockMath => "block math".to_string(),
    }
}

/// Where a snippet is tried in the test box: its first place.
pub(super) fn test_context(snippet: &Snippet) -> (InputContext, bool) {
    match snippet.options.scopes.first() {
        Some(Scope::Context(context)) => (*context, false),
        Some(Scope::InlineMath) => (InputContext::Math, false),
        Some(Scope::BlockMath) => (InputContext::Math, true),
        None => (InputContext::Text, false),
    }
}

/// The snippets grouped by the heading comments above them. A comment
/// starts a group when a blank line or the start of the file comes before
/// it; other comments are notes inside a group.
fn snippet_groups(file: &SnippetFile) -> Vec<(String, Vec<SnippetRow>)> {
    let mut groups: Vec<(String, Vec<SnippetRow>)> = Vec::new();
    let mut after_blank = true;
    for (line, entry) in file.lines.iter().enumerate() {
        match entry {
            FileLine::Comment(text) if after_blank => groups.push((text.clone(), Vec::new())),
            FileLine::Snippet(snippet) => {
                if groups.is_empty() {
                    groups.push((UNGROUPED.to_string(), Vec::new()));
                }
                let rows = &mut groups.last_mut().expect("a group was just made").1;
                rows.push(SnippetRow::new(line, snippet));
            }
            _ => {}
        }
        after_blank = matches!(entry, FileLine::Blank);
    }
    groups.retain(|(_, rows)| !rows.is_empty());
    groups
}

/// The replacements by group, in file order, leaving out curly quotes:
/// the smart quotes setting on the Editor page owns those.
fn replacement_groups(table: &Replacements) -> Vec<(String, Vec<ReplacementRow>)> {
    let mut groups: Vec<(String, Vec<ReplacementRow>)> = Vec::new();
    for (index, entry) in table.entries.iter().enumerate() {
        if entry.closing.is_some() {
            continue;
        }
        let group = capitalised(if entry.group.is_empty() {
            "Other"
        } else {
            &entry.group
        });
        let row = ReplacementRow {
            index,
            from: entry.from.clone(),
            to: entry.to.clone(),
            when: replacement_when(entry),
            on: entry.enabled,
            look: SnippetLook::of_replacement(entry),
        };
        match groups.iter_mut().find(|(name, _)| *name == group) {
            Some((_, rows)) => rows.push(row),
            None => groups.push((group, vec![row])),
        }
    }
    groups
}

fn replacement_when(entry: &editor_snippets::Replacement) -> String {
    let when = match entry.fire {
        ReplacementFire::Instant => "as you type",
        ReplacementFire::AfterSpace => "after a space",
    };
    match &entry.contexts {
        Some(contexts) => {
            let names: Vec<&str> = contexts.iter().map(|c| c.name()).collect();
            format!("In {}, {when}.", names.join(" or "))
        }
        None => format!("Outside code and math, {when}."),
    }
}

pub(super) fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

impl SettingsView {
    // ---- Rows ----

    /// The page's rows after its setting cards: the snippets and then the
    /// replacements, one card per group.
    pub(super) fn snippet_cards(&self, query: &str, layout: &mut PaneLayout) {
        let lists = &self.typing_lists;
        if query.is_empty() || words_match("add snippet new", query) {
            layout.push_card(None, vec![ControlRow::SnippetsFile]);
        }
        if self
            .snippet_editor
            .as_ref()
            .is_some_and(|e| e.line.is_none())
        {
            layout.push_card(None, vec![ControlRow::SnippetEditor]);
        }
        for (group, rows) in snippet_groups(&lists.snippets) {
            let mut shown = Vec::new();
            for row in rows {
                let haystack = format!(
                    "{} {} {} {group} {}",
                    row.trigger,
                    row.expansion,
                    row.when,
                    row.look.tooltip_words()
                );
                if !words_match(&haystack, query) {
                    continue;
                }
                let editing = self.editing_line() == Some(row.line);
                shown.push(ControlRow::Snippet(row));
                if editing {
                    shown.push(ControlRow::SnippetEditor);
                }
            }
            layout.push_card(Some(group), shown);
        }
        for (group, rows) in replacement_groups(&lists.replacements) {
            let shown = rows
                .into_iter()
                .filter(|row| {
                    let haystack = format!(
                        "{} {} {} {group} {}",
                        row.from,
                        row.to,
                        row.when,
                        row.look.tooltip_words()
                    );
                    words_match(&haystack, query)
                })
                .map(ControlRow::Replacement)
                .collect();
            layout.push_card(Some(group), shown);
        }
    }

    pub(super) fn editing_line(&self) -> Option<usize> {
        self.snippet_editor.as_ref().and_then(|editor| editor.line)
    }

    /// What the row at the top of the snippets says about their file.
    pub(super) fn snippets_file_description(&self) -> String {
        if let Some(problem) = &self.typing_lists.problem {
            return format!("The file can’t be read, so nothing here changes it: {problem}");
        }
        if self.typing_lists.snippets_from_vault {
            "Saved in .editor/snippets.txt, so they sync with your notes.".to_string()
        } else {
            "Built in. Your first change saves a copy to .editor/snippets.txt.".to_string()
        }
    }

    // ---- Changes ----

    pub(super) fn toggle_snippet(&mut self, line: usize, cx: &mut Context<Self>) {
        let mut file = self.typing_lists.snippets.clone();
        if let Some(FileLine::Snippet(snippet)) = file.lines.get_mut(line) {
            snippet.options.off = !snippet.options.off;
            self.save_snippets(file, cx);
        }
    }

    pub(super) fn toggle_replacement(&mut self, index: usize, cx: &mut Context<Self>) {
        let mut table = self.typing_lists.replacements.clone();
        if let Some(entry) = table.entries.get_mut(index) {
            entry.enabled = !entry.enabled;
            self.save_replacements(table, cx);
        }
    }

    fn can_write(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        if let Some(problem) = &self.typing_lists.problem {
            self.error = Some((key.to_string(), format!("Fix the file first: {problem}")));
            cx.notify();
            return false;
        }
        true
    }

    pub(super) fn save_snippets(&mut self, file: SnippetFile, cx: &mut Context<Self>) -> bool {
        if !self.can_write(SNIPPETS_KEY, cx) {
            return false;
        }
        let written = store::save(&snippets_path(&self.vault_root), &file.to_string());
        self.finish_write(SNIPPETS_KEY, written, cx, |lists| {
            lists.snippets = file;
            lists.snippets_from_vault = true;
        })
    }

    fn save_replacements(&mut self, table: Replacements, cx: &mut Context<Self>) -> bool {
        if !self.can_write(REPLACEMENTS_KEY, cx) {
            return false;
        }
        let text = format!(
            "# Replacements for typing, which the settings screen keeps.\n\n{}",
            table.to_toml()
        );
        let written = store::save(&replacements_path(&self.vault_root), &text);
        self.finish_write(REPLACEMENTS_KEY, written, cx, |lists| {
            lists.replacements = table;
            lists.replacements_from_vault = true;
        })
    }

    fn finish_write(
        &mut self,
        key: &str,
        written: io::Result<()>,
        cx: &mut Context<Self>,
        take: impl FnOnce(&mut TypingLists),
    ) -> bool {
        let saved = match written {
            Ok(()) => {
                take(&mut self.typing_lists);
                self.error = None;
                self.invalidate_layouts();
                cx.emit(SettingsEvent::Changed(key.to_string()));
                true
            }
            Err(error) => {
                self.error = Some((key.to_string(), error.to_string()));
                false
            }
        };
        cx.notify();
        saved
    }
}

/// Adds `snippet` at the end of the group new snippets go in, starting
/// the group when there isn't one yet.
pub(super) fn add_snippet(file: &mut SnippetFile, snippet: Snippet) {
    let heading = FileLine::Comment(ADDED_GROUP.to_string());
    if let Some(at) = file.lines.iter().position(|line| *line == heading) {
        let end = file.lines[at + 1..]
            .iter()
            .position(|line| !matches!(line, FileLine::Snippet(_)))
            .map_or(file.lines.len(), |offset| at + 1 + offset);
        file.lines.insert(end, FileLine::Snippet(snippet));
        return;
    }
    if !matches!(file.lines.last(), None | Some(FileLine::Blank)) {
        file.push_blank();
    }
    file.lines.push(heading);
    file.push_snippet(snippet);
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_snippets::parse_snippet;

    const FILE: &str = "\
# Migrated from Latex Suite.
# A note under the heading.

# Math mode
mk → $●$  text, instant
# Auto letter subscript
{letter}{digit} → {letter}_{{digit}}  math, instant, off

# Empty group

# Greek
@a → \\alpha  math, instant
";

    #[test]
    fn headings_follow_blank_lines_and_empty_groups_drop() {
        let file = SnippetFile::parse(FILE).unwrap();
        let groups = snippet_groups(&file);
        let names: Vec<&str> = groups.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["Math mode", "Greek"]);
        assert_eq!(groups[0].1.len(), 2);
        assert!(!groups[0].1[1].on);
        assert_eq!(groups[0].1[0].when, "In text, as you type.");
    }

    #[test]
    fn new_snippets_gather_under_one_heading() {
        let mut file = SnippetFile::parse(FILE).unwrap();
        add_snippet(&mut file, parse_snippet("zz → z  anywhere").unwrap());
        add_snippet(&mut file, parse_snippet("yy → y  anywhere").unwrap());
        let tail: Vec<String> = file.lines[file.lines.len() - 4..]
            .iter()
            .map(|line| match line {
                FileLine::Snippet(snippet) => format_trigger(&snippet.trigger),
                FileLine::Comment(text) => format!("# {text}"),
                FileLine::Blank => String::new(),
            })
            .collect();
        assert_eq!(tail, ["", "# Added in settings", "zz", "yy"]);
    }

    #[test]
    fn quotes_stay_off_the_replacements_list() {
        let mut quote = editor_snippets::Replacement::new("\"", "“", "Quotes");
        quote.closing = Some("”".to_string());
        let table = Replacements::new(vec![
            quote,
            editor_snippets::Replacement::new("--", "—", "dashes"),
        ]);
        let groups = replacement_groups(&table);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, "Dashes");
        assert_eq!(groups[0].1[0].when, "Outside code and math, as you type.");
    }
}
