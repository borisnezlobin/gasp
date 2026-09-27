//! Paragraphs with inline features (math, links, footnotes, tags, HTML...) woven into prose.

use std::collections::{BTreeMap, BTreeSet};

use crate::math;
use crate::prose::{self, Sentence};
use crate::rng::Rng;

/// One inline feature that must appear in a paragraph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    Math,
    Link,
    Highlight,
    Tag,
    Comment,
    Wikilink,
    Br,
    Underline,
    Sup,
    Span,
    Kbd,
    /// A footnote reference: a new label, or a repeat of one already cited.
    Footnote,
}

const TAGS: &[&str] = &[
    "#physics",
    "#todo",
    "#review",
    "#idea",
    "#lecture/week-3",
    "#reading",
    "#draft",
    "#maths/analysis",
    "#question",
    "#follow-up",
];

/// Numbers footnote references in document order. Every label is cited
/// once in order, and the remaining references repeat labels already cited.
#[derive(Clone, Debug, Default)]
pub struct FootnoteState {
    cited: usize,
    total: usize,
    slots_left: usize,
    typos: BTreeSet<usize>,
    names: BTreeMap<usize, String>,
}

impl FootnoteState {
    /// `total` labels cited over `slots` references, of which `typos` are
    /// cited as `^[n]` and `names` have word labels.
    pub fn new(
        total: usize,
        slots: usize,
        typos: BTreeSet<usize>,
        names: BTreeMap<usize, String>,
    ) -> Self {
        Self {
            cited: 0,
            total,
            slots_left: slots,
            typos,
            names,
        }
    }

    /// The label text of footnote `number`.
    pub fn label(&self, number: usize) -> String {
        self.names
            .get(&number)
            .cloned()
            .unwrap_or_else(|| number.to_string())
    }

    fn reference(&mut self, rng: &mut Rng) -> String {
        let new_left = self.total - self.cited;
        let slots = self.slots_left.max(1);
        self.slots_left = self.slots_left.saturating_sub(1);
        let repeatable: Vec<usize> = (1..=self.cited)
            .filter(|n| !self.typos.contains(n))
            .collect();
        let cite_new = new_left > 0
            && (repeatable.is_empty() || new_left >= slots || rng.below(slots) < new_left);
        if cite_new {
            self.cited += 1;
            return self.render(self.cited);
        }
        let number = repeatable
            .get(rng.below(repeatable.len().max(1)))
            .copied()
            .unwrap_or(1);
        format!("[^{}]", self.label(number))
    }

    fn render(&self, number: usize) -> String {
        if self.typos.contains(&number) {
            return format!("^[{number}]");
        }
        format!("[^{}]", self.label(number))
    }
}

/// What inline rendering needs from the note being written.
pub struct InlineEnv<'a> {
    pub titles: &'a [String],
    pub self_index: usize,
    pub footnotes: &'a mut FootnoteState,
}

impl InlineEnv<'_> {
    /// A title of some other note.
    pub fn other_title(&self, rng: &mut Rng) -> String {
        let count = self.titles.len();
        if count < 2 {
            return self.titles.first().cloned().unwrap_or_default();
        }
        let offset = rng.range(1, count - 1);
        self.titles[(self.self_index + offset) % count].clone()
    }
}

#[derive(Default)]
struct Draft {
    sentence: Sentence,
    footnotes: usize,
    after: Vec<String>,
    breaks: usize,
}

impl Draft {
    fn new(sentence: Sentence) -> Self {
        Self {
            sentence,
            ..Self::default()
        }
    }

    fn render(&self, rng: &mut Rng, env: &mut InlineEnv) -> String {
        let mut text = self.sentence.render();
        let end_len = self.sentence.end.len();
        let mut marks = String::new();
        for _ in 0..self.footnotes {
            marks.push_str(&env.footnotes.reference(rng));
        }
        text.insert_str(text.len() - end_len, &marks);
        for extra in &self.after {
            text.push(' ');
            text.push_str(extra);
        }
        text.push_str(&"<br>".repeat(self.breaks));
        text
    }
}

/// Renders a paragraph that contains exactly the given tokens.
pub fn paragraph(rng: &mut Rng, env: &mut InlineEnv, tokens: &[Token]) -> String {
    let math_count = tokens.iter().filter(|t| **t == Token::Math).count();
    let mut drafts = math_drafts(rng, math_count);
    let plain = if math_count == 0 {
        rng.range(2, 5)
    } else {
        rng.range(0, 2)
    };
    for _ in 0..plain {
        let plain = prose::sentence(rng);
        drafts.push(Draft::new(decorated(rng, plain)));
    }
    if drafts.is_empty() {
        drafts.push(Draft::new(prose::sentence(rng)));
    }
    rng.shuffle(&mut drafts);
    for token in tokens.iter().filter(|t| **t != Token::Math) {
        let at = rng.below(drafts.len());
        apply(rng, env, &mut drafts[at], *token);
    }
    let mut out = String::new();
    for draft in &drafts {
        if !out.is_empty() {
            out.push(if out.ends_with("<br>") { '\n' } else { ' ' });
        }
        out.push_str(&draft.render(rng, env));
    }
    out
}

fn math_drafts(rng: &mut Rng, math_count: usize) -> Vec<Draft> {
    let mut drafts = Vec::new();
    let mut left = math_count;
    while left > 0 {
        let size = (rng.weighted(&[50, 35, 15]) + 1).min(left);
        let maths: Vec<String> = (0..size)
            .map(|_| format!("${}$", math::inline(rng)))
            .collect();
        drafts.push(Draft::new(prose::math_sentence(rng, &maths)));
        left -= size;
    }
    drafts
}

fn apply(rng: &mut Rng, env: &mut InlineEnv, draft: &mut Draft, token: Token) {
    let words = &mut draft.sentence.words;
    match token {
        Token::Link => add_link(rng, env, words),
        Token::Highlight => wrap_or_append(rng, words, "==", "=="),
        Token::Underline => wrap_or_append(rng, words, "<u>", "</u>"),
        Token::Span => wrap_or_append(rng, words, "<span style=\"color: #b5452c\">", "</span>"),
        Token::Sup => wrap_or_append(rng, words, "<sup>", "</sup>"),
        Token::Kbd => words.extend([
            "with".to_string(),
            format!("<kbd>{}</kbd>", rng.pick(&["Tab", "Esc", "Enter"])),
        ]),
        Token::Tag => draft.after.push(rng.pick(TAGS).to_string()),
        Token::Comment => add_comment(rng, draft),
        Token::Wikilink => add_wikilink(rng, env, words),
        Token::Br => draft.breaks += 1,
        Token::Footnote => draft.footnotes += 1,
        Token::Math => {}
    }
}

fn add_link(rng: &mut Rng, env: &InlineEnv, words: &mut Vec<String>) {
    let target = if rng.chance(0.2) {
        format!("{}.md", env.other_title(rng).replace(' ', "%20"))
    } else {
        prose::url(rng)
    };
    let close = format!("]({target})");
    if !wrap(rng, words, 3, "[", &close) {
        words.push(format!("(see [{}]({target}))", prose::phrase(rng)));
    }
}

fn add_comment(rng: &mut Rng, draft: &mut Draft) {
    let template = *rng.pick(&["todo: check the %n", "%a", "ask %N about this", "rewrite"]);
    let text = if rng.chance(0.3) {
        format!("%%{}%%", prose::fill(rng, template))
    } else {
        format!("%% {} %%", prose::fill(rng, template))
    };
    if rng.chance(0.5) || draft.sentence.words.len() < 3 {
        draft.after.push(text);
        return;
    }
    let at = rng.range(1, draft.sentence.words.len() - 1);
    draft.sentence.words.insert(at, text);
}

fn add_wikilink(rng: &mut Rng, env: &InlineEnv, words: &mut Vec<String>) {
    let title = env.other_title(rng);
    words.extend(["as".to_string(), "in".to_string()]);
    words.push(if rng.chance(0.5) {
        format!("[[{title}]]")
    } else {
        format!("[[{title}|{}]]", prose::phrase(rng))
    });
}

fn wrap_or_append(rng: &mut Rng, words: &mut Vec<String>, open: &str, close: &str) {
    if !wrap(rng, words, 3, open, close) {
        words.push(format!("{open}{}{close}", prose::phrase(rng)));
    }
}

fn is_plain(word: &str) -> bool {
    !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphabetic() || c == '\'' || c == '-')
}

/// Wraps a run of up to `max_len` plain words (never the first) in markup.
fn wrap(rng: &mut Rng, words: &mut Vec<String>, max_len: usize, open: &str, close: &str) -> bool {
    let candidates: Vec<usize> = (1..words.len()).filter(|&i| is_plain(&words[i])).collect();
    if candidates.is_empty() {
        return false;
    }
    let start = *rng.pick(&candidates);
    let want = rng.range(1, max_len);
    let mut end = start;
    while end + 1 < words.len() && end + 1 - start < want && is_plain(&words[end + 1]) {
        end += 1;
    }
    let joined = format!("{open}{}{close}", words[start..=end].join(" "));
    words.splice(start..=end, [joined]);
    true
}

const DECORATIONS: &[(&str, &str, f64)] = &[
    ("**", "**", 0.08),
    ("*", "*", 0.06),
    ("_", "_", 0.03),
    ("~~", "~~", 0.025),
    ("***", "***", 0.01),
    ("`", "`", 0.03),
];

/// Adds random emphasis, strikethrough and inline code to a plain sentence.
pub fn decorated(rng: &mut Rng, mut sentence: Sentence) -> Sentence {
    for (open, close, probability) in DECORATIONS {
        if rng.chance(*probability) {
            wrap(rng, &mut sentence.words, 2, open, close);
        }
    }
    sentence
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_with<'a>(titles: &'a [String], footnotes: &'a mut FootnoteState) -> InlineEnv<'a> {
        InlineEnv {
            titles,
            self_index: 0,
            footnotes,
        }
    }

    #[test]
    fn paragraph_contains_every_token() {
        let titles = vec!["One".to_string(), "Two".to_string()];
        let mut state = FootnoteState::new(2, 2, BTreeSet::new(), BTreeMap::new());
        let mut env = env_with(&titles, &mut state);
        let mut rng = Rng::new(5);
        let tokens = [
            Token::Math,
            Token::Math,
            Token::Link,
            Token::Highlight,
            Token::Footnote,
            Token::Footnote,
            Token::Br,
            Token::Wikilink,
        ];
        let text = paragraph(&mut rng, &mut env, &tokens);
        assert_eq!(text.matches('$').count(), 4, "{text}");
        assert!(text.contains("]("), "{text}");
        assert!(text.contains("=="), "{text}");
        assert!(text.contains("[^1]") && text.contains("[^2]"), "{text}");
        assert!(text.contains("<br>"), "{text}");
        assert!(text.contains("[[Two"), "{text}");
    }

    #[test]
    fn typo_labels_render_as_typos() {
        let mut rng = Rng::new(1);
        let mut state = FootnoteState::new(2, 2, BTreeSet::from([2]), BTreeMap::new());
        assert_eq!(state.reference(&mut rng), "[^1]");
        assert_eq!(state.reference(&mut rng), "^[2]");
    }

    #[test]
    fn labels_are_cited_in_order_and_repeats_look_back() {
        for seed in 0..50 {
            let mut rng = Rng::new(seed);
            let mut state = FootnoteState::new(4, 7, BTreeSet::from([3]), BTreeMap::new());
            let refs: Vec<String> = (0..7).map(|_| state.reference(&mut rng)).collect();
            let mut highest = 0;
            for r in &refs {
                let number: usize = r
                    .trim_matches(|c: char| !c.is_ascii_digit())
                    .parse()
                    .unwrap();
                assert!(number <= highest + 1, "{refs:?}");
                highest = highest.max(number);
            }
            assert_eq!(highest, 4);
            assert_eq!(
                refs.iter().filter(|r| r.starts_with('^')).count(),
                1,
                "{refs:?}"
            );
        }
    }

    #[test]
    fn wrap_skips_first_word() {
        let mut rng = Rng::new(1);
        let mut words = vec!["the".to_string(), "proof".to_string()];
        assert!(wrap(&mut rng, &mut words, 1, "**", "**"));
        assert_eq!(words, ["the", "**proof**"]);
    }
}
