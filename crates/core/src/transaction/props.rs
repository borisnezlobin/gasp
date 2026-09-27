//! Property tests for change composition, inversion and mapping.

use proptest::prelude::*;

use super::*;

const TEXT: &str = "[ab é😀\n]{0,24}";
const INSERT: &str = "[xy ß\n]{0,4}";

/// Picks char boundaries in `text` from the random indices and keeps the
/// edits that don't overlap earlier ones.
fn changes_for(text: &str, picks: &[(usize, usize, String)]) -> ChangeSet {
    let boundaries: Vec<usize> = text
        .char_indices()
        .map(|(offset, _)| offset)
        .chain([text.len()])
        .collect();
    let mut ranges: Vec<TextEdit> = picks
        .iter()
        .map(|(a, b, insert)| {
            let a = boundaries[a % boundaries.len()];
            let b = boundaries[b % boundaries.len()];
            TextEdit::new(a.min(b)..a.max(b), insert.clone())
        })
        .collect();
    ranges.sort_by_key(|edit| edit.range.start);
    let mut kept: Vec<TextEdit> = Vec::new();
    for edit in ranges {
        let clashes = kept.last().is_some_and(|last| {
            edit.range.start < last.range.end || edit.range.start == last.range.start
        });
        if !clashes {
            kept.push(edit);
        }
    }
    ChangeSet::new(kept).unwrap()
}

fn picks() -> impl Strategy<Value = Vec<(usize, usize, String)>> {
    prop::collection::vec((0usize..64, 0usize..64, INSERT), 0..5)
}

fn apply(changes: &ChangeSet, text: &str) -> String {
    changes.apply_to_string(text).unwrap()
}

proptest! {
    #[test]
    fn invert_restores_original(text in TEXT, picks in picks()) {
        let changes = changes_for(&text, &picks);
        let edited = apply(&changes, &text);
        let inverse = changes.invert(&Document::from(text.as_str()));
        prop_assert_eq!(apply(&inverse, &edited), text.clone());
        let double = inverse.invert(&Document::from(edited.as_str()));
        prop_assert_eq!(apply(&double, &text), edited);
    }

    #[test]
    fn compose_matches_sequential_apply(
        text in TEXT,
        first_picks in picks(),
        second_picks in picks(),
    ) {
        let first = changes_for(&text, &first_picks);
        let middle = apply(&first, &text);
        let second = changes_for(&middle, &second_picks);
        let expected = apply(&second, &middle);
        let composed = first.compose(&second);
        prop_assert_eq!(apply(&composed, &text), expected);
    }

    #[test]
    fn compose_is_associative(
        text in TEXT,
        picks_a in picks(),
        picks_b in picks(),
        picks_c in picks(),
    ) {
        let a = changes_for(&text, &picks_a);
        let after_a = apply(&a, &text);
        let b = changes_for(&after_a, &picks_b);
        let c = changes_for(&apply(&b, &after_a), &picks_c);
        let left = a.compose(&b).compose(&c);
        let right = a.compose(&b.compose(&c));
        prop_assert_eq!(apply(&left, &text), apply(&right, &text));
    }

    #[test]
    fn compose_with_inverse_is_identity(text in TEXT, picks in picks()) {
        let changes = changes_for(&text, &picks);
        let inverse = changes.invert(&Document::from(text.as_str()));
        prop_assert_eq!(apply(&changes.compose(&inverse), &text), text);
    }

    #[test]
    fn mapped_offsets_are_monotone_boundaries(text in TEXT, picks in picks()) {
        let changes = changes_for(&text, &picks);
        let edited = Document::from(apply(&changes, &text).as_str());
        let (mut previous_before, mut previous_after) = (0, 0);
        for offset in (0..=text.len()).filter(|offset| text.is_char_boundary(*offset)) {
            let before = changes.map_offset(offset, Assoc::Before);
            let after = changes.map_offset(offset, Assoc::After);
            prop_assert!(previous_before <= before && previous_after <= after);
            prop_assert!(before <= after && after <= edited.len());
            prop_assert!(edited.is_char_boundary(before) && edited.is_char_boundary(after));
            (previous_before, previous_after) = (before, after);
        }
    }

    #[test]
    fn untouched_text_keeps_its_place(text in TEXT, picks in picks()) {
        let changes = changes_for(&text, &picks);
        let edited = apply(&changes, &text);
        let touched = |offset: usize| {
            changes.edits().iter().any(|edit| edit.range.start <= offset && offset < edit.range.end)
        };
        for (offset, ch) in text.char_indices().filter(|(offset, _)| !touched(*offset)) {
            let mapped = changes.map_offset(offset, Assoc::After);
            prop_assert_eq!(edited[mapped..].chars().next(), Some(ch));
        }
    }

    #[test]
    fn map_through_gives_applicable_changes(
        text in TEXT,
        local_picks in picks(),
        remote_picks in picks(),
    ) {
        let local = changes_for(&text, &local_picks);
        let remote = changes_for(&text, &remote_picks);
        let after_remote = Document::from(apply(&remote, &text).as_str());
        let rebased = local.map_through(&remote);
        prop_assert!(rebased.validate(&after_remote).is_ok());
    }
}
