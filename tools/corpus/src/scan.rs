//! Measures features by scanning note text. It does not trust the generator,
//! so tests and `manifest.json` report what the files really contain.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;

use crate::FootnoteProblem;

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("scanner regex is valid")
}

static FENCE: LazyLock<Regex> = LazyLock::new(|| re(r"^(`{3,}|~{3,})\s*([^\s`]*)(.*)$"));
static CALLOUT: LazyLock<Regex> = LazyLock::new(|| re(r"^\[!([A-Za-z-]+)\]([+-]?)\s*(.*)$"));
static FOOTNOTE_DEF: LazyLock<Regex> = LazyLock::new(|| re(r"^\[\^([^\]\s]+)\]:(.*)$"));
static HEADING: LazyLock<Regex> = LazyLock::new(|| re(r"^(#{1,6})\s+(.*)$"));
static TASK: LazyLock<Regex> = LazyLock::new(|| re(r"^(\s*)([-*+]|\d+[.)])\s\[([ xX])\]\s"));
static LIST: LazyLock<Regex> = LazyLock::new(|| re(r"^(\s*)([-*+]|\d+[.)])\s"));
static TABLE_SEPARATOR: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\|?(\s*:?-{3,}:?\s*\|)+\s*(:?-{3,}:?\s*)?$"));
static INLINE_CODE: LazyLock<Regex> = LazyLock::new(|| re(r"`[^`]+`"));
static EMBED: LazyLock<Regex> = LazyLock::new(|| re(r"!\[\[([^\]]+)\]\]"));
static WIKILINK: LazyLock<Regex> = LazyLock::new(|| re(r"(?:^|[^!])\[\[([^\]]+)\]\]"));
static LINK: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?:^|[^!\]])\[([^\[\]^][^\[\]]*)\]\(([^()\s]+)\)"));
static FOOTNOTE_REF: LazyLock<Regex> = LazyLock::new(|| re(r"\[\^([^\]\s]+)\]"));
static FOOTNOTE_TYPO: LazyLock<Regex> = LazyLock::new(|| re(r"\^\[(\d+)\]"));
static HIGHLIGHT: LazyLock<Regex> = LazyLock::new(|| re(r"==[^=\s][^=]*=="));
static HTML_TAG: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)<(br|hr|div|img|u|sup|sub|span|kbd|mark|center|details|summary|font|a|p)\b[^<>]*>")
});
static ANY_TAG: LazyLock<Regex> = LazyLock::new(|| re(r"</?[A-Za-z][^<>]*>"));
static IMG_SRC: LazyLock<Regex> = LazyLock::new(|| re(r#"<img\b[^>]*\bsrc="([^"]+)""#));
static TAG: LazyLock<Regex> = LazyLock::new(|| re(r"(?:^|\s)#([A-Za-z][\w/-]*)"));
static BOLD: LazyLock<Regex> = LazyLock::new(|| re(r"\*\*[^*\s][^*]*\*\*"));
static ITALIC: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?:^|[^*\w])(?:\*[^*\s][^*]*\*|_[^_\s][^_]*_)(?:[^*\w]|$)"));
static STRIKE: LazyLock<Regex> = LazyLock::new(|| re(r"~~[^~\s][^~]*~~"));
static URL: LazyLock<Regex> = LazyLock::new(|| re(r"(?:https?://|www\.)[^\s)<>\]]+"));
static ABBREVIATION: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?:^|[^A-Za-z.])(?:e\.g\.|i\.e\.|U\.S\.|Dr\.|St\.|Prof\.|Mr\.|Mrs\.|Ms\.|etc\.|vs\.)")
});
static INITIALS: LazyLock<Regex> = LazyLock::new(|| re(r"\b[A-Z]\. [A-Z]\. [A-Z][a-z]"));
static DECIMAL: LazyLock<Regex> = LazyLock::new(|| re(r"\b\d+\.\d+\b"));
static TIME: LazyLock<Regex> = LazyLock::new(|| re(r"\b\d{1,2}:\d{2}\b"));
static FRONTMATTER_KEY: LazyLock<Regex> = LazyLock::new(|| re(r"^([A-Za-z_][\w-]*):"));

/// Everything measured in one note.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NoteScan {
    pub counts: BTreeMap<String, usize>,
    pub footnote_refs: Vec<String>,
    /// Definitions as (label, is empty).
    pub footnote_defs: Vec<(String, bool)>,
    pub footnote_typos: Vec<String>,
    pub embeds: Vec<String>,
    pub img_sources: Vec<String>,
    pub wikilinks: Vec<String>,
    pub callout_types: Vec<String>,
    pub code_languages: Vec<String>,
    pub html_tags: Vec<String>,
}

impl NoteScan {
    /// The count for a feature key, zero when absent.
    pub fn count(&self, key: &str) -> usize {
        self.counts.get(key).copied().unwrap_or(0)
    }

    fn add(&mut self, key: &str, amount: usize) {
        if amount > 0 {
            *self.counts.entry(key.to_string()).or_insert(0) += amount;
        }
    }

    /// Footnote problems found in this note, as (label, problem), sorted.
    pub fn footnote_problems(&self) -> BTreeSet<(String, FootnoteProblem)> {
        let refs: BTreeSet<&String> = self.footnote_refs.iter().collect();
        let mut defs: BTreeMap<&String, usize> = BTreeMap::new();
        for (label, _) in &self.footnote_defs {
            *defs.entry(label).or_insert(0) += 1;
        }
        let mut problems = BTreeSet::new();
        for label in refs.iter().filter(|l| !defs.contains_key(**l)) {
            problems.insert(((*label).clone(), FootnoteProblem::Missing));
        }
        for (label, count) in &defs {
            if !refs.contains(label) {
                problems.insert(((*label).clone(), FootnoteProblem::Unused));
            }
            if *count > 1 {
                problems.insert(((*label).clone(), FootnoteProblem::Duplicate));
            }
        }
        for (label, empty) in &self.footnote_defs {
            if *empty {
                problems.insert((label.clone(), FootnoteProblem::Empty));
            }
        }
        for label in &self.footnote_typos {
            problems.insert((label.clone(), FootnoteProblem::Typo));
        }
        problems
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    Code { fence: u8, length: usize },
    Math,
    Comment,
}

struct Scanner {
    scan: NoteScan,
    mode: Mode,
}

/// Scans one note's text.
pub fn scan_note(text: &str) -> NoteScan {
    let mut scanner = Scanner {
        scan: NoteScan::default(),
        mode: Mode::Normal,
    };
    let body = scanner.frontmatter(text);
    for line in body.lines() {
        scanner.line(line);
    }
    let equations =
        scanner.scan.count("inline_math") + scanner.scan.count("block_math_delimiters") / 2;
    scanner.scan.add("equations", equations);
    scanner.scan
}

impl Scanner {
    fn frontmatter<'t>(&mut self, text: &'t str) -> &'t str {
        let Some(rest) = text.strip_prefix("---\n") else {
            return text;
        };
        let Some(end) = rest
            .find("\n---\n")
            .or_else(|| rest.strip_suffix("\n---").map(str::len))
        else {
            return text;
        };
        let keys: BTreeSet<&str> = rest[..end]
            .lines()
            .filter_map(|line| {
                FRONTMATTER_KEY
                    .captures(line)
                    .map(|c| c.get(1).map_or("", |m| m.as_str()))
            })
            .collect();
        self.scan.add("frontmatter", 1);
        let chronotyper = ["updated", "edited_seconds"];
        let has_chronotyper = chronotyper.iter().all(|k| keys.contains(k));
        let only_chronotyper = keys.iter().all(|k| chronotyper.contains(k));
        self.scan
            .add("frontmatter_chronotyper", usize::from(has_chronotyper));
        self.scan.add(
            "frontmatter_chronotyper_only",
            usize::from(has_chronotyper && only_chronotyper),
        );
        self.scan.add(
            "frontmatter_with_other_keys",
            usize::from(!only_chronotyper),
        );
        rest.get(end + 5..).unwrap_or("")
    }

    fn line(&mut self, raw: &str) {
        let (depth, content) = strip_quote(raw);
        match self.mode {
            Mode::Code { fence, length } => self.code_line(content, fence, length),
            Mode::Math => self.math_line(content),
            Mode::Comment => self.comment_line(content),
            Mode::Normal => self.normal_line(depth, content),
        }
    }

    fn code_line(&mut self, content: &str, fence: u8, length: usize) {
        let trimmed = content.trim();
        let closes = trimmed.len() >= length && trimmed.bytes().all(|b| b == fence);
        if closes {
            self.scan.add("code_fence_lines", 1);
            self.mode = Mode::Normal;
        } else {
            self.scan.add("code_lines", 1);
        }
    }

    fn math_line(&mut self, content: &str) {
        let delimiters = content.matches("$$").count();
        if delimiters > 0 {
            self.scan.add("block_math_delimiters", delimiters);
            self.mode = Mode::Normal;
        } else {
            self.scan.add("block_math_lines", 1);
        }
    }

    fn comment_line(&mut self, content: &str) {
        if content.contains("%%") {
            self.mode = Mode::Normal;
        }
    }

    fn normal_line(&mut self, depth: usize, content: &str) {
        let trimmed = content.trim_start();
        if self.opens_block(trimmed) {
            return;
        }
        let text = if depth > 0 {
            self.quote_line(depth, trimmed)
        } else {
            trimmed
        };
        let text = self.structure(content, text);
        self.inline(text);
    }

    /// Handles lines that open a code fence, display math or a comment block.
    fn opens_block(&mut self, trimmed: &str) -> bool {
        if let Some(caps) = FENCE.captures(trimmed) {
            let fence = &caps[1];
            self.scan.add("code_fence_lines", 1);
            self.scan.add("code_blocks", 1);
            self.scan
                .add("tilde_fences", usize::from(fence.starts_with('~')));
            self.scan
                .add("code_fence_titles", usize::from(caps[3].contains("title:")));
            self.scan.code_languages.push(caps[2].to_string());
            self.mode = Mode::Code {
                fence: fence.as_bytes()[0],
                length: fence.len(),
            };
            return true;
        }
        if trimmed.starts_with("$$") {
            let delimiters = trimmed.matches("$$").count();
            self.scan.add("block_math_delimiters", delimiters);
            if delimiters == 1 {
                self.mode = Mode::Math;
            }
            return true;
        }
        if trimmed == "%%" {
            self.scan.add("comments", 1);
            self.scan.add("comment_blocks", 1);
            self.mode = Mode::Comment;
            return true;
        }
        false
    }

    /// Records callout headers and quote lines; returns the text left to scan.
    fn quote_line<'t>(&mut self, depth: usize, trimmed: &'t str) -> &'t str {
        let Some(caps) = CALLOUT.captures(trimmed) else {
            self.scan.add("blockquote_lines", 1);
            return trimmed;
        };
        self.scan.add("callouts", 1);
        self.scan.add("nested_callouts", usize::from(depth > 1));
        self.scan
            .add("foldable_callouts", usize::from(!caps[2].is_empty()));
        self.scan
            .add("callout_titles", usize::from(!caps[3].trim().is_empty()));
        self.scan.callout_types.push(caps[1].to_lowercase());
        caps.get(3).map_or("", |m| m.as_str())
    }

    /// Records headings, lists, tasks, table rows and footnote definitions.
    fn structure<'t>(&mut self, content: &'t str, text: &'t str) -> &'t str {
        if let Some(caps) = FOOTNOTE_DEF.captures(text) {
            let rest = caps.get(2).map_or("", |m| m.as_str());
            self.scan.add("footnote_defs", 1);
            self.scan
                .footnote_defs
                .push((caps[1].to_string(), rest.trim().is_empty()));
            return rest;
        }
        if let Some(caps) = HEADING.captures(text) {
            self.scan.add(&format!("heading_h{}", caps[1].len()), 1);
            return caps.get(2).map_or("", |m| m.as_str());
        }
        if text.starts_with('|') {
            return self.table_row(text);
        }
        self.list_item(content);
        text
    }

    fn table_row<'t>(&mut self, text: &'t str) -> &'t str {
        if TABLE_SEPARATOR.is_match(text) {
            self.scan.add("table_separator_rows", 1);
            self.scan
                .add("table_alignment_rows", usize::from(text.contains(':')));
            return "";
        }
        self.scan.add("table_rows", 1);
        self.scan.add("escaped_pipes", text.matches("\\|").count());
        text
    }

    fn list_item(&mut self, content: &str) {
        let Some(caps) = LIST.captures(content) else {
            return;
        };
        let nested = !caps[1].is_empty();
        self.scan.add("list_items", 1);
        self.scan.add("nested_list_items", usize::from(nested));
        self.scan.add(
            "ordered_list_items",
            usize::from(caps[2].starts_with(|c: char| c.is_ascii_digit())),
        );
        if let Some(task) = TASK.captures(content) {
            self.scan.add("task_items", 1);
            self.scan.add("nested_task_items", usize::from(nested));
            self.scan
                .add("checked_task_items", usize::from(&task[3] != " "));
        }
    }

    fn inline(&mut self, text: &str) {
        let text = self.strip_inline_code(text);
        let text = self.strip_inline_comments(&text);
        let text = self.strip_inline_math(&text);
        self.links_and_references(&text);
        self.html(&text);
        let text = ANY_TAG.replace_all(&text, " ");
        self.prose_features(&text);
    }

    fn strip_inline_code(&mut self, text: &str) -> String {
        self.scan
            .add("inline_code", INLINE_CODE.find_iter(text).count());
        INLINE_CODE.replace_all(text, " ").into_owned()
    }

    fn strip_inline_comments(&mut self, text: &str) -> String {
        let pieces: Vec<&str> = text.split("%%").collect();
        let pairs = (pieces.len() - 1) / 2;
        self.scan.add("comments", pairs);
        self.scan.add("inline_comments", pairs);
        pieces
            .iter()
            .step_by(2)
            .copied()
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn strip_inline_math(&mut self, text: &str) -> String {
        let (kept, spans, escaped) = split_inline_math(text);
        self.scan.add("inline_math", spans);
        self.scan.add("escaped_dollars", escaped);
        kept
    }

    fn links_and_references(&mut self, text: &str) {
        for caps in EMBED.captures_iter(text) {
            let target = &caps[1];
            self.scan.add("embeds", 1);
            self.scan
                .add("embeds_with_size", usize::from(target.contains('|')));
            self.scan
                .embeds
                .push(target.split('|').next().unwrap_or("").to_string());
        }
        for caps in WIKILINK.captures_iter(text) {
            self.scan.add("wikilinks", 1);
            self.scan
                .wikilinks
                .push(caps[1].split('|').next().unwrap_or("").to_string());
        }
        self.scan
            .add("markdown_links", LINK.find_iter(text).count());
        for caps in FOOTNOTE_REF.captures_iter(text) {
            self.scan.add("footnote_refs", 1);
            self.scan.footnote_refs.push(caps[1].to_string());
        }
        for caps in FOOTNOTE_TYPO.captures_iter(text) {
            self.scan.add("footnote_typos", 1);
            self.scan.footnote_typos.push(caps[1].to_string());
        }
        self.scan
            .add("highlights", HIGHLIGHT.find_iter(text).count());
    }

    fn html(&mut self, text: &str) {
        for caps in HTML_TAG.captures_iter(text) {
            let name = caps[1].to_lowercase();
            self.scan.add("html_tags", 1);
            let key = match name.as_str() {
                "br" => "html_br",
                "hr" => "html_hr",
                _ => "html_other",
            };
            self.scan.add(key, 1);
            self.scan.html_tags.push(name);
        }
        for caps in IMG_SRC.captures_iter(text) {
            self.scan.img_sources.push(caps[1].to_string());
        }
    }

    fn prose_features(&mut self, text: &str) {
        self.scan.add("tags", TAG.find_iter(text).count());
        self.scan.add("bold", BOLD.find_iter(text).count());
        self.scan.add("italic", ITALIC.find_iter(text).count());
        self.scan
            .add("strikethrough", STRIKE.find_iter(text).count());
        self.scan.add("em_dashes", text.matches('—').count());
        self.scan.add("en_dashes", text.matches('–').count());
        self.scan.add(
            "ellipses",
            text.matches('…').count() + text.matches("...").count(),
        );
        self.scan.add("urls", URL.find_iter(text).count());
        let text = URL.replace_all(text, " ");
        self.scan
            .add("abbreviations", ABBREVIATION.find_iter(&text).count());
        self.scan.add("initials", INITIALS.find_iter(&text).count());
        self.scan.add("decimals", DECIMAL.find_iter(&text).count());
        self.scan.add("times", TIME.find_iter(&text).count());
    }
}

/// Removes `$…$` spans. Returns the remaining text, the number of spans and
/// the number of escaped `\$`.
fn split_inline_math(text: &str) -> (String, usize, usize) {
    let bytes = text.as_bytes();
    let mut kept = String::with_capacity(text.len());
    let (mut spans, mut escaped) = (0, 0);
    let mut i = 0;
    let mut copied_from = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                escaped += usize::from(bytes.get(i + 1) == Some(&b'$'));
                i += 2;
            }
            b'$' => {
                let Some(close) = closing_dollar(bytes, i + 1) else {
                    break;
                };
                kept.push_str(&text[copied_from..i]);
                kept.push(' ');
                spans += 1;
                i = close + 1;
                copied_from = i;
            }
            _ => i += 1,
        }
    }
    kept.push_str(text.get(copied_from..).unwrap_or(""));
    (kept, spans, escaped)
}

fn closing_dollar(bytes: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'$' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

/// Splits leading `>` markers off a line; returns the quote depth and the rest.
fn strip_quote(line: &str) -> (usize, &str) {
    let mut depth = 0;
    let mut rest = line;
    while let Some(after) = rest.trim_start_matches(' ').strip_prefix('>') {
        depth += 1;
        rest = after.strip_prefix(' ').unwrap_or(after);
    }
    (depth, rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_inline_math_and_skips_escapes() {
        let scan = scan_note("Let $x$ and $y^2$ cost \\$5.\n");
        assert_eq!(scan.count("inline_math"), 2);
        assert_eq!(scan.count("escaped_dollars"), 1);
    }

    #[test]
    fn code_blocks_hide_markdown() {
        let scan =
            scan_note("```markdown title:\"a.md\"\n$x$ [[a]] #tag\n```\n~~~\n- [ ] no\n~~~\n");
        assert_eq!(scan.count("inline_math"), 0);
        assert_eq!(scan.count("wikilinks"), 0);
        assert_eq!(scan.count("task_items"), 0);
        assert_eq!(scan.count("code_fence_lines"), 4);
        assert_eq!(scan.count("code_fence_titles"), 1);
        assert_eq!(scan.count("tilde_fences"), 1);
    }

    #[test]
    fn display_math_in_callouts() {
        let text = "> [!tip]- Title\n> text $a$\n> $$\n> x^2\n> $$\n> > [!note]\n> > inner\n";
        let scan = scan_note(text);
        assert_eq!(scan.count("callouts"), 2);
        assert_eq!(scan.count("nested_callouts"), 1);
        assert_eq!(scan.count("foldable_callouts"), 1);
        assert_eq!(scan.count("block_math_delimiters"), 2);
        assert_eq!(scan.count("equations"), 2);
        assert_eq!(scan.callout_types, ["tip", "note"]);
    }

    #[test]
    fn footnote_problems_are_found() {
        let text =
            "A[^1] b[^2] c ^[3] d[^5].\n\n[^1]: one\n[^1]: again\n[^3]: three\n[^4]:\n[^5]:\n";
        let problems = scan_note(text).footnote_problems();
        let has = |label: &str, p| problems.contains(&(label.to_string(), p));
        assert!(has("2", FootnoteProblem::Missing));
        assert!(has("1", FootnoteProblem::Duplicate));
        assert!(has("3", FootnoteProblem::Typo));
        assert!(has("4", FootnoteProblem::Unused));
        assert!(has("5", FootnoteProblem::Empty));
    }

    #[test]
    fn frontmatter_kinds() {
        let only = scan_note("---\nupdated: 2024-01-01\nedited_seconds: 5\n---\nText\n");
        assert_eq!(only.count("frontmatter_chronotyper_only"), 1);
        let other = scan_note("---\ntags:\n  - a\nupdated: x\nedited_seconds: 5\n---\n");
        assert_eq!(other.count("frontmatter_with_other_keys"), 1);
    }

    #[test]
    fn tables_tasks_and_lists() {
        let text = "| a | b |\n| :--- | ---: |\n| x \\| y | $z$ |\n\n- [x] done\n\t- [ ] sub\n1. one\n    - nested\n";
        let scan = scan_note(text);
        assert_eq!(scan.count("table_rows"), 2);
        assert_eq!(scan.count("table_alignment_rows"), 1);
        assert_eq!(scan.count("escaped_pipes"), 1);
        assert_eq!(scan.count("task_items"), 2);
        assert_eq!(scan.count("nested_task_items"), 1);
        assert_eq!(scan.count("list_items"), 4);
        assert_eq!(scan.count("nested_list_items"), 2);
    }

    #[test]
    fn links_embeds_html_and_tags() {
        let text = "See [a](https://example.org/x#y) and ![[p.png|300]] <br> <hr/> <u>u</u> <span style=\"color: #abc\">s</span> #tag [[Note]] ==hi== %% c %%\n";
        let scan = scan_note(text);
        assert_eq!(scan.count("markdown_links"), 1);
        assert_eq!(scan.count("embeds"), 1);
        assert_eq!(scan.count("embeds_with_size"), 1);
        assert_eq!(scan.count("html_tags"), 4);
        assert_eq!(scan.count("tags"), 1);
        assert_eq!(scan.count("wikilinks"), 1);
        assert_eq!(scan.count("highlights"), 1);
        assert_eq!(scan.count("comments"), 1);
        assert_eq!(scan.embeds, ["p.png"]);
    }

    #[test]
    fn prose_features() {
        let text = "At 10:30 Dr. Novak — e.g. in the U.S. — paid 3.5 units… see www.example.com/a.b and J. R. Quist.\n";
        let scan = scan_note(text);
        assert_eq!(scan.count("times"), 1);
        assert_eq!(scan.count("abbreviations"), 3);
        assert_eq!(scan.count("em_dashes"), 2);
        assert_eq!(scan.count("decimals"), 1);
        assert_eq!(scan.count("ellipses"), 1);
        assert_eq!(scan.count("urls"), 1);
        assert_eq!(scan.count("initials"), 1);
    }

    #[test]
    fn headings() {
        let scan = scan_note("# A\n###### F\n####### not\n");
        assert_eq!(scan.count("heading_h1"), 1);
        assert_eq!(scan.count("heading_h6"), 1);
    }
}
