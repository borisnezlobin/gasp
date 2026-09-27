//! Synthetic English: word lists and sentence templates. Nothing here comes from real notes.

use crate::rng::Rng;

const NOUNS: &[&str] = &[
    "argument",
    "estimate",
    "boundary",
    "lecture",
    "result",
    "approach",
    "sequence",
    "function",
    "experiment",
    "reading",
    "draft",
    "paragraph",
    "method",
    "model",
    "hypothesis",
    "proof",
    "example",
    "problem",
    "section",
    "theorem",
    "limit",
    "series",
    "derivation",
    "assumption",
    "observation",
    "measurement",
    "question",
    "summary",
    "outline",
    "diagram",
    "notebook",
    "exercise",
    "definition",
    "lemma",
    "corollary",
    "matrix",
    "vector",
    "operator",
    "field",
    "particle",
    "wave",
    "system",
    "spectrum",
    "integral",
    "variable",
    "constraint",
    "pattern",
    "essay",
    "chapter",
    "quote",
    "source",
    "margin",
    "schedule",
    "deadline",
    "habit",
    "routine",
    "sketch",
    "draft",
    "tutorial",
    "seminar",
    "office hour",
    "problem set",
    "midterm",
    "recipe",
    "project",
    "plugin",
    "shortcut",
    "workflow",
    "library",
    "idea",
    "detail",
    "trick",
];

const ADJECTIVES: &[&str] = &[
    "careful",
    "rough",
    "clean",
    "subtle",
    "obvious",
    "tricky",
    "useful",
    "strange",
    "elegant",
    "messy",
    "short",
    "long",
    "quiet",
    "sharp",
    "loose",
    "formal",
    "informal",
    "linear",
    "smooth",
    "bounded",
    "finite",
    "hidden",
    "simple",
    "general",
    "local",
    "global",
    "early",
    "late",
    "second",
    "final",
    "better",
    "weaker",
    "stronger",
    "cleaner",
    "honest",
    "brief",
    "dense",
    "sparse",
    "unexpected",
    "familiar",
    "classic",
    "modern",
    "odd",
    "narrow",
    "broad",
];

const VERBS: &[&str] = &[
    "shows",
    "suggests",
    "implies",
    "requires",
    "explains",
    "changes",
    "simplifies",
    "extends",
    "breaks",
    "fixes",
    "bounds",
    "describes",
    "ignores",
    "connects",
    "predicts",
    "clarifies",
    "complicates",
    "motivates",
    "justifies",
    "replaces",
    "covers",
    "hides",
    "reveals",
    "reuses",
    "tests",
    "follows",
    "mirrors",
    "undoes",
    "sharpens",
    "summarises",
];

const BASE_VERBS: &[&str] = &[
    "change",
    "explain",
    "need",
    "prove",
    "fix",
    "break",
    "cover",
    "matter for",
];

const INTRANSITIVE: &[&str] = &[
    "works",
    "holds",
    "fails",
    "converges",
    "diverges",
    "helps",
    "matters",
    "stalls",
    "improves",
    "vanishes",
    "breaks down",
    "falls apart",
    "checks out",
    "adds up",
    "stands",
];

const ADVERBS: &[&str] = &[
    "quickly",
    "clearly",
    "barely",
    "roughly",
    "eventually",
    "surprisingly",
    "probably",
    "almost",
    "still",
    "only",
    "mostly",
    "rarely",
    "quietly",
    "neatly",
    "slowly",
];

const PREPOSITIONS: &[&str] = &[
    "in", "on", "for", "with", "under", "after", "before", "near", "without", "across", "through",
    "about", "against", "beyond",
];

const CONJUNCTIONS: &[&str] = &[
    "and", "but", "so", "because", "while", "although", "since", "whereas", "yet",
];

const DETERMINERS: &[&str] = &[
    "the", "a", "this", "that", "every", "each", "our", "my", "one",
];

const SUBJECT_PRONOUNS: &[&str] = &[
    "it",
    "this",
    "that",
    "she",
    "he",
    "everyone",
    "nobody",
    "the class",
];

/// Synthetic surnames used for Dr., Prof. and St. abbreviations.
pub const NAMES: &[&str] = &[
    "Okafor",
    "Lindqvist",
    "Marchetti",
    "Haddad",
    "Novak",
    "Tanaka",
    "Ferreira",
    "Albright",
    "Castellan",
    "Moreau",
    "Ivanova",
    "Delacroix",
    "Whitcombe",
    "Oyelaran",
    "Quist",
];

const TITLES: &[&str] = &["Dr.", "Prof.", "Mr.", "Mrs.", "Ms."];

const SLUGS: &[&str] = &[
    "notes",
    "fourier-series",
    "linear-maps",
    "reading-list",
    "week-3",
    "problem-set-2",
    "harmonic-oscillator",
    "draft",
    "syllabus",
    "errata",
    "lecture-7",
    "archive",
    "faq",
];

const DOMAINS: &[&str] = &[
    "example.org",
    "example.com",
    "example.net",
    "docs.example.org",
];

const SHORT_TEMPLATES: &[&str] = &[
    "this works",
    "it fails",
    "not quite",
    "fair enough",
    "good question",
    "that helps",
    "the %n %i",
    "%N %i",
    "exactly",
    "try again",
    "it %i %d",
    "no %n here",
    "much %a",
    "the %n is %a",
    "that is %a",
    "%a %n",
    "we move on",
    "back to the %n",
];

const MATH_TEMPLATES_ONE: &[&str] = &[
    "let @",
    "then @",
    "we know that @",
    "recall @ from last week",
    "note that @",
    "so @ and the %n %i",
    "the %n %i when @",
    "in particular @",
    "this gives @",
    "it follows that @",
    "we want @",
    "assume @",
    "check that @ holds",
    "the key step is @ — everything else is bookkeeping",
    "here @ is the %a %n",
];

const MATH_TEMPLATES_TWO: &[&str] = &[
    "if @, then @",
    "since @, we get @",
    "here @ and @",
    "substituting @ gives @",
    "compare @ with @",
    "from @ it follows that @",
    "take @ so that @",
    "the %n says @ whenever @",
];

const MATH_TEMPLATES_THREE: &[&str] = &[
    "given @ and @, it follows that @",
    "with @, @ and @ the %n %i",
    "combining @ with @ yields @",
];

/// One sentence as a list of words plus its closing punctuation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sentence {
    pub words: Vec<String>,
    pub end: &'static str,
}

impl Sentence {
    fn from_words(words: Vec<String>, end: &'static str) -> Self {
        Self { words, end }
    }

    /// Number of words.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.words.len()
    }

    /// True when there are no words.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// The sentence with its first letter capitalised and its punctuation.
    pub fn render(&self) -> String {
        let mut text = self.words.join(" ");
        capitalize_first(&mut text);
        text.push_str(self.end);
        text
    }
}

fn capitalize_first(text: &mut String) {
    let Some(first) = text.chars().next() else {
        return;
    };
    if first.is_ascii_lowercase() {
        text.replace_range(0..1, &first.to_ascii_uppercase().to_string());
    }
}

fn words_of(text: &str) -> Vec<String> {
    text.split(' ').map(str::to_string).collect()
}

/// A noun from the word list.
pub fn noun(rng: &mut Rng) -> &'static str {
    rng.pick::<&str>(NOUNS)
}

/// An adjective from the word list.
pub fn adjective(rng: &mut Rng) -> &'static str {
    rng.pick::<&str>(ADJECTIVES)
}

/// A synthetic surname.
pub fn name(rng: &mut Rng) -> &'static str {
    rng.pick::<&str>(NAMES)
}

/// Fills `%n` (noun), `%a` (adjective), `%i` (intransitive verb), `%v` (verb),
/// `%d` (adverb) and `%N` (titled name) slots.
pub fn fill(rng: &mut Rng, template: &str) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut chars = template.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let slot = chars.next().unwrap_or('%');
        out.push_str(&slot_value(rng, slot));
    }
    out
}

fn slot_value(rng: &mut Rng, slot: char) -> String {
    match slot {
        'n' => noun(rng).to_string(),
        'a' => adjective(rng).to_string(),
        'i' => rng.pick(INTRANSITIVE).to_string(),
        'v' => rng.pick(VERBS).to_string(),
        'd' => rng.pick(ADVERBS).to_string(),
        'N' => format!("{} {}", rng.pick(TITLES), name(rng)),
        other => other.to_string(),
    }
}

fn with_article(det: &str, next: &str) -> String {
    let vowel = next.starts_with(['a', 'e', 'i', 'o', 'u']);
    if det == "a" && vowel {
        "an".to_string()
    } else {
        det.to_string()
    }
}

fn noun_phrase(rng: &mut Rng) -> Vec<String> {
    let det = *rng.pick(DETERMINERS);
    let mut rest = Vec::new();
    if rng.chance(0.45) {
        rest.push(adjective(rng).to_string());
    }
    rest.extend(words_of(noun(rng)));
    let mut words = vec![with_article(det, &rest[0])];
    words.extend(rest);
    words
}

fn subject(rng: &mut Rng) -> Vec<String> {
    let roll = rng.below(10);
    if roll < 2 {
        return words_of(rng.pick::<&str>(SUBJECT_PRONOUNS));
    }
    if roll < 3 {
        return vec![rng.pick(TITLES).to_string(), name(rng).to_string()];
    }
    noun_phrase(rng)
}

fn clause(rng: &mut Rng) -> Vec<String> {
    let mut words = subject(rng);
    if rng.chance(0.3) {
        words.push(rng.pick(ADVERBS).to_string());
    }
    if rng.chance(0.2) {
        words.extend(words_of(rng.pick::<&str>(INTRANSITIVE)));
        return words;
    }
    words.push(rng.pick(VERBS).to_string());
    words.extend(noun_phrase(rng));
    words
}

fn prepositional_phrase(rng: &mut Rng) -> Vec<String> {
    let mut words = vec![rng.pick(PREPOSITIONS).to_string()];
    words.extend(noun_phrase(rng));
    words
}

/// A sentence of two to six words.
pub fn short_sentence(rng: &mut Rng) -> Sentence {
    let template = *rng.pick(SHORT_TEMPLATES);
    let text = fill(rng, template);
    let end = *rng.pick(&[".", ".", ".", "!", "?"]);
    Sentence::from_words(words_of(&text), end)
}

fn medium_sentence(rng: &mut Rng) -> Sentence {
    let mut words = clause(rng);
    if rng.chance(0.6) {
        words.extend(prepositional_phrase(rng));
    }
    maybe_add_aside(rng, &mut words, 0.22);
    Sentence::from_words(words, ".")
}

/// A sentence of more than eighteen words.
pub fn long_sentence(rng: &mut Rng) -> Sentence {
    let mut words = clause(rng);
    let last = words.pop().unwrap_or_default();
    words.push(format!("{last},"));
    words.push(rng.pick(CONJUNCTIONS).to_string());
    words.extend(clause(rng));
    while words.len() < 19 {
        words.extend(prepositional_phrase(rng));
    }
    maybe_add_aside(rng, &mut words, 0.35);
    Sentence::from_words(words, ".")
}

/// A plain prose sentence: roughly a fifth short, a quarter long.
pub fn sentence(rng: &mut Rng) -> Sentence {
    match rng.weighted(&[20, 47, 25, 8]) {
        0 => short_sentence(rng),
        1 => medium_sentence(rng),
        2 => long_sentence(rng),
        _ => special_sentence(rng),
    }
}

/// A sentence carrying exactly `maths.len()` (1 to 3) inline math spans.
pub fn math_sentence(rng: &mut Rng, maths: &[String]) -> Sentence {
    let templates = match maths.len() {
        1 => MATH_TEMPLATES_ONE,
        2 => MATH_TEMPLATES_TWO,
        _ => MATH_TEMPLATES_THREE,
    };
    let template = *rng.pick(templates);
    let template = fill(rng, template);
    let mut pending = maths.iter();
    let words = template
        .split(' ')
        .map(|word| match word.find('@') {
            Some(_) => word.replacen('@', pending.next().map_or("", String::as_str), 1),
            None => word.to_string(),
        })
        .collect();
    Sentence::from_words(words, ".")
}

fn special_sentence(rng: &mut Rng) -> Sentence {
    let text = match rng.below(7) {
        0 => format!(
            "{} wrote, \"the {} is {}.\"",
            fill(rng, "%N"),
            noun(rng),
            adjective(rng)
        ),
        1 => format!("(see the {} {} above.)", adjective(rng), noun(rng)),
        2 => format!(
            "it seemed {}… until the {} {}",
            adjective(rng),
            noun(rng),
            fill(rng, "%i")
        ),
        3 => format!(
            "does the {} really {} the {}?",
            noun(rng),
            rng.pick(BASE_VERBS),
            noun(rng)
        ),
        4 => format!(
            "{} put it best: \"{} {}.\"",
            fill(rng, "%N"),
            fill(rng, "%a"),
            noun(rng)
        ),
        5 => format!("the {} kept going…", noun(rng)),
        _ => format!("well... the {} {} anyway", noun(rng), fill(rng, "%i")),
    };
    let end = if text.ends_with(['.', '?', ')', '"', '…']) {
        ""
    } else {
        "."
    };
    Sentence::from_words(words_of(&text), end)
}

fn maybe_add_aside(rng: &mut Rng, words: &mut Vec<String>, probability: f64) {
    if !rng.chance(probability) {
        return;
    }
    let aside = aside(rng);
    if aside.first().is_some_and(|w| w == "—") && words.len() > 3 {
        let at = rng.range(2, words.len() - 1);
        words.splice(at..at, aside);
    } else {
        words.extend(aside);
    }
}

type Aside = fn(&mut Rng) -> String;

/// Clauses that exercise the sentence segmenter: dashes, abbreviations,
/// initials, decimals, times, URLs and ellipses.
const ASIDES: &[Aside] = &[
    |rng| format!("— at least for {} {}s —", adjective(rng), noun(rng)),
    |rng| format!("(e.g. the {} or the {})", noun(rng), noun(rng)),
    |rng| format!("(i.e. the {} {})", adjective(rng), noun(rng)),
    |_| "in the U.S. and elsewhere".to_string(),
    |rng| format!("according to Dr. {}", name(rng)),
    |rng| format!("near St. {}'s", name(rng)),
    |rng| format!("at {}", clock_time(rng)),
    |rng| format!("about {} {}s", decimal(rng), noun(rng)),
    |rng| format!("(see {})", url(rng)),
    |rng| {
        format!(
            "in version {}.{}.{}",
            rng.range(1, 4),
            rng.range(0, 12),
            rng.range(0, 9)
        )
    },
    |rng| format!("and then… {}", adjective(rng)),
    |rng| format!("roughly {}% of the time", rng.range(5, 95)),
    |rng| format!("as {}. {}. {} noted", initial(rng), initial(rng), name(rng)),
    |rng| format!("vs. the {} {}", adjective(rng), noun(rng)),
    |rng| format!("for \\${}.{:02}", rng.range(2, 90), rng.below(100)),
    |rng| format!("on pages {}–{}", rng.range(3, 40), rng.range(41, 90)),
    |rng| format!("like {}s, {}s, etc. in general", noun(rng), noun(rng)),
];

fn aside(rng: &mut Rng) -> Vec<String> {
    let make = *rng.pick(ASIDES);
    words_of(&make(rng))
}

fn initial(rng: &mut Rng) -> char {
    (b'A' + rng.below(26) as u8) as char
}

/// A time like `10:30`.
pub fn clock_time(rng: &mut Rng) -> String {
    format!("{}:{:02}", rng.range(7, 22), rng.below(4) * 15)
}

/// A decimal number like `3.25`.
pub fn decimal(rng: &mut Rng) -> String {
    format!("{}.{}", rng.below(20), rng.range(1, 99))
}

/// A URL on one of the reserved example domains.
pub fn url(rng: &mut Rng) -> String {
    let domain = rng.pick(DOMAINS);
    let slug = rng.pick(SLUGS);
    match rng.below(4) {
        0 => format!("https://{domain}/{slug}"),
        1 => format!("https://{domain}/notes/{slug}?page={}", rng.range(1, 9)),
        2 => format!("http://{domain}/{slug}.html"),
        _ => format!("https://www.{domain}/{slug}#section-{}", rng.range(1, 6)),
    }
}

/// A short phrase for link text, list items and titles (one to three words).
pub fn phrase(rng: &mut Rng) -> String {
    match rng.below(3) {
        0 => noun(rng).to_string(),
        1 => format!("{} {}", adjective(rng), noun(rng)),
        _ => format!("{} {}", noun(rng), fill(rng, "%n")),
    }
}

/// A phrase in title case.
pub fn title_case(text: &str) -> String {
    text.split(' ')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A short imperative for a task item.
pub fn task_text(rng: &mut Rng) -> String {
    let verb = *rng.pick(&[
        "Review",
        "Finish",
        "Email",
        "Read",
        "Rewrite",
        "Check",
        "Print",
        "Outline",
        "Upload",
        "Ask about",
    ]);
    match rng.below(4) {
        0 => format!("{verb} the {}", phrase(rng)),
        1 => format!("{verb} the {} before {}", noun(rng), clock_time(rng)),
        2 => format!("{verb} Dr. {}'s {}", name(rng), noun(rng)),
        _ => format!("{verb} {} {}s", rng.range(2, 9), noun(rng)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_sentences_are_long() {
        let mut rng = Rng::new(1);
        for _ in 0..200 {
            assert!(long_sentence(&mut rng).len() > 18);
        }
    }

    #[test]
    fn short_sentences_are_short() {
        let mut rng = Rng::new(2);
        for _ in 0..200 {
            let s = short_sentence(&mut rng);
            assert!(!s.is_empty() && s.len() < 7, "{s:?}");
        }
    }

    #[test]
    fn mix_has_short_medium_and_long() {
        let mut rng = Rng::new(3);
        let lengths: Vec<usize> = (0..500).map(|_| sentence(&mut rng).len()).collect();
        assert!(lengths.iter().any(|&n| n < 7));
        assert!(lengths.iter().any(|&n| (7..=18).contains(&n)));
        assert!(lengths.iter().any(|&n| n > 18));
    }

    #[test]
    fn math_sentence_uses_every_span() {
        let mut rng = Rng::new(4);
        let maths = vec!["$a$".to_string(), "$b$".to_string()];
        let text = math_sentence(&mut rng, &maths).render();
        assert!(text.contains("$a$") && text.contains("$b$"), "{text}");
    }

    #[test]
    fn render_capitalises() {
        let s = Sentence::from_words(words_of("the proof works"), ".");
        assert_eq!(s.render(), "The proof works.");
    }

    #[test]
    fn article_agrees() {
        assert_eq!(with_article("a", "idea"), "an");
        assert_eq!(with_article("a", "proof"), "a");
    }
}
