//! Footnote problems. Missing, unused and duplicate footnotes are structural
//! and block renumbering; empty definitions and `^[n]` typos are advisory.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use super::parse::{ParsedFootnotes, parse_footnotes};

/// What is wrong with a footnote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FootnoteProblemKind {
    /// A reference with no definition.
    Missing,
    /// A definition nothing references.
    Unused,
    /// A `^[n]` typo for `[^n]`.
    InlineTypo,
    /// A label defined more than once.
    Duplicate,
    /// A definition with no text.
    Empty,
}

impl FootnoteProblemKind {
    /// Stable identifier, matching the original plugin's kind names.
    pub fn id(self) -> &'static str {
        match self {
            Self::Missing => "dangling-ref",
            Self::Unused => "orphan-def",
            Self::InlineTypo => "inline-typo",
            Self::Duplicate => "duplicate-def",
            Self::Empty => "empty-def",
        }
    }

    /// Whether this problem stops renumbering.
    pub fn is_blocking(self) -> bool {
        matches!(self, Self::Missing | Self::Unused | Self::Duplicate)
    }

    /// Lower is more important when two problems cover the same span.
    pub fn priority(self) -> u8 {
        match self {
            Self::Missing => 0,
            Self::Duplicate => 1,
            Self::Unused => 2,
            Self::Empty => 3,
            Self::InlineTypo => 4,
        }
    }
}

/// One problem, with the range to highlight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FootnoteProblem {
    pub kind: FootnoteProblemKind,
    pub range: Range<usize>,
    pub label: String,
    pub message: String,
}

fn message(kind: FootnoteProblemKind, label: &str) -> String {
    match kind {
        FootnoteProblemKind::Missing => format!("Footnote [^{label}] has no definition."),
        FootnoteProblemKind::Unused => format!("Footnote [^{label}] is defined but never used."),
        FootnoteProblemKind::Duplicate => {
            format!("Footnote [^{label}] is defined more than once.")
        }
        FootnoteProblemKind::Empty => format!("Footnote [^{label}] has no text yet."),
        FootnoteProblemKind::InlineTypo => format!(
            "\"^[{label}]\" is an inline footnote containing \"{label}\". Did you mean [^{label}]?"
        ),
    }
}

fn problem(kind: FootnoteProblemKind, range: Range<usize>, label: &str) -> FootnoteProblem {
    FootnoteProblem {
        kind,
        range,
        label: label.to_string(),
        message: message(kind, label),
    }
}

/// Every problem in already-parsed footnotes, in the plugin's order.
pub fn classify_problems(parsed: &ParsedFootnotes) -> Vec<FootnoteProblem> {
    let mut problems = Vec::new();
    let referenced: HashSet<&str> = parsed.refs.iter().map(|r| r.label.as_str()).collect();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for def in &parsed.defs {
        *counts.entry(def.label.as_str()).or_default() += 1;
    }
    let def_count = |label: &str| counts.get(label).copied().unwrap_or(0);

    for reference in &parsed.refs {
        if def_count(&reference.label) == 0 {
            let range = reference.range.clone();
            problems.push(problem(
                FootnoteProblemKind::Missing,
                range,
                &reference.label,
            ));
        }
    }

    for def in &parsed.defs {
        let label = def.label.as_str();
        let checks = [
            (FootnoteProblemKind::Unused, !referenced.contains(label)),
            (FootnoteProblemKind::Duplicate, def_count(label) > 1),
            (FootnoteProblemKind::Empty, def.body.trim().is_empty()),
        ];
        for (kind, found) in checks {
            if found {
                problems.push(problem(kind, def.head_range(), label));
            }
        }
    }

    for typo in &parsed.inline_typos {
        let range = typo.range.clone();
        problems.push(problem(FootnoteProblemKind::InlineTypo, range, &typo.label));
    }
    problems
}

/// Every footnote problem in `text`.
pub fn find_problems(text: &str) -> Vec<FootnoteProblem> {
    classify_problems(&parse_footnotes(text))
}

/// Problems to highlight: sorted, non-empty, and only the most important one
/// when several cover exactly the same span.
pub fn highlight_problems(text: &str) -> Vec<FootnoteProblem> {
    let mut problems: Vec<FootnoteProblem> = find_problems(text)
        .into_iter()
        .filter(|p| !p.range.is_empty())
        .collect();
    problems.sort_by_key(|p| (p.range.start, p.range.end, p.kind.priority()));
    problems.dedup_by(|later, earlier| later.range == earlier.range);
    problems
}

/// Whether any problem blocks renumbering.
pub fn has_blocking_problems(problems: &[FootnoteProblem]) -> bool {
    problems.iter().any(|p| p.kind.is_blocking())
}

/// Distinct `[^label]` strings for blocking problems, in first-seen order.
pub fn blocking_labels(problems: &[FootnoteProblem]) -> Vec<String> {
    let mut labels: Vec<String> = Vec::new();
    for p in problems.iter().filter(|p| p.kind.is_blocking()) {
        let label = format!("[^{}]", p.label);
        if !labels.contains(&label) {
            labels.push(label);
        }
    }
    labels
}
