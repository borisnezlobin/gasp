//! Numbered footnotes are numbered 1..N by first appearance, with the
//! definitions listed in the same order. Named footnotes are left alone, and
//! nothing is renumbered while the document has structural problems:
//! rearranging an inconsistent document is how footnotes get jumbled.

use std::collections::HashMap;
use std::ops::Range;

use super::edits::{FootnoteEdit, apply_edits, line_ending};
use super::lint::{FootnoteProblem, classify_problems, has_blocking_problems};
use super::parse::{FootnoteDef, ParsedFootnotes, is_numeric, parse_footnotes};

/// What renumbering would do to a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenumberResult {
    pub changed: bool,
    /// Edits against the input text.
    pub edits: Vec<FootnoteEdit>,
    pub new_text: String,
    /// The definition block that was rebuilt, in input offsets.
    pub reordered_block: Option<Range<usize>>,
    pub problems: Vec<FootnoteProblem>,
}

fn defs_are_contiguous(text: &str, defs: &[FootnoteDef]) -> bool {
    defs.windows(2)
        .all(|pair| text[pair[0].end..pair[1].line_start].trim().is_empty())
}

/// Old numeric label to new number, by first appearance among references.
fn numbering(parsed: &ParsedFootnotes) -> HashMap<&str, usize> {
    let mut map = HashMap::new();
    for reference in parsed.refs.iter().filter(|r| is_numeric(&r.label)) {
        let next = map.len() + 1;
        map.entry(reference.label.as_str()).or_insert(next);
    }
    map
}

fn reference_edits(parsed: &ParsedFootnotes, map: &HashMap<&str, usize>) -> Vec<FootnoteEdit> {
    parsed
        .refs
        .iter()
        .filter_map(|r| {
            let new = map.get(r.label.as_str())?.to_string();
            (new != r.label).then(|| FootnoteEdit::new(r.range.clone(), format!("[^{new}]")))
        })
        .collect()
}

fn rebuilt_block(text: &str, defs: &[FootnoteDef], map: &HashMap<&str, usize>) -> String {
    let mut sorted: Vec<&FootnoteDef> = defs.iter().collect();
    sorted.sort_by_key(|def| map.get(def.label.as_str()).copied().unwrap_or(usize::MAX));
    let lines: Vec<String> = sorted
        .iter()
        .map(|def| {
            let new = map
                .get(def.label.as_str())
                .map_or(def.label.clone(), usize::to_string);
            format!("{}[^{new}]:{}", def.indent, def.body)
        })
        .collect();
    lines.join(line_ending(text))
}

fn relabel_in_place(parsed: &ParsedFootnotes, map: &HashMap<&str, usize>) -> Vec<FootnoteEdit> {
    parsed
        .defs
        .iter()
        .filter_map(|def| {
            let new = map.get(def.label.as_str())?.to_string();
            let head = def.head_range();
            let range = head.start..head.end - 1;
            (new != def.label).then(|| FootnoteEdit::new(range, format!("[^{new}]")))
        })
        .collect()
}

/// Computes the edits that bring numbered footnotes into reading order.
pub fn compute_renumber(text: &str) -> RenumberResult {
    let parsed = parse_footnotes(text);
    let problems = classify_problems(&parsed);
    if has_blocking_problems(&problems) {
        return RenumberResult {
            changed: false,
            edits: Vec::new(),
            new_text: text.to_string(),
            reordered_block: None,
            problems,
        };
    }

    let map = numbering(&parsed);
    let mut edits = reference_edits(&parsed, &map);
    let defs = &parsed.defs;
    let can_reorder = !defs.is_empty()
        && defs.iter().all(|def| is_numeric(&def.label))
        && defs_are_contiguous(text, defs);

    let mut reordered_block = None;
    if can_reorder {
        let block = defs[0].line_start..defs[defs.len() - 1].end;
        let rebuilt = rebuilt_block(text, defs, &map);
        if rebuilt != text[block.clone()] {
            edits.push(FootnoteEdit::new(block.clone(), rebuilt));
        }
        reordered_block = Some(block);
    } else {
        edits.extend(relabel_in_place(&parsed, &map));
    }

    let new_text = apply_edits(text, &edits);
    RenumberResult {
        changed: new_text != text,
        edits,
        new_text,
        reordered_block,
        problems,
    }
}
