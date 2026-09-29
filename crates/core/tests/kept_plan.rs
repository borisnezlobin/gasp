//! A kept plan that follows the cursor through every corpus note gives
//! the same lines as planning the whole note afresh at each stop.

use gasp_bench::corpus::corpus_notes;
use gasp_config::settings::SymbolSettings;
use gasp_core::render::{
    KeptPlan, RenderInput, RevealMode, RevealScope, RevealSettings, plan, reveal_settings,
};
use gasp_core::syntax::{Edit, Reparsed, parse};

const CURSOR_STEP: usize = 211;
const TYPING_STEP: usize = 1531;

/// Keys typed at a place, a `\u{8}` being a backspace.
const KEYS: &str = "a *b*\n$x^2$ [[c]]\u{8}\u{8} | d |\n\n- e\u{8}\u{8}\u{8}";

#[test]
fn a_kept_plan_follows_typing_through_the_corpus() {
    let settings = reveal_settings(&SymbolSettings::default());
    let (mut keys, mut by_blocks) = (0, 0);
    for (path, original) in corpus_notes() {
        let starts = (0..original.len())
            .step_by(TYPING_STEP)
            .filter(|&at| original.is_char_boundary(at));
        for start in starts {
            let mut text = original.clone();
            let mut tree = parse(&text);
            let mut kept = KeptPlan::default();
            let mut cursor = start;
            for key in KEYS.chars() {
                let edit = press(&mut text, &mut cursor, key);
                let reparsed = tree.edit(&text, &edit);
                keys += 1;
                by_blocks += usize::from(matches!(reparsed, Reparsed::Blocks { .. }));
                kept.edited(&edit, &reparsed, &tree);
                let caret = cursor..cursor;
                let selections = [caret];
                let input = RenderInput {
                    text: &text,
                    tree: &tree,
                    selections: &selections,
                    settings: &settings,
                };
                let fresh = plan(&input).lines;
                assert!(
                    kept.plan(&input) == &fresh[..],
                    "{path} from {start}, key {key:?}"
                );
            }
        }
    }
    assert!(
        by_blocks * 2 > keys,
        "{by_blocks} of {keys} keys reparsed blocks"
    );
}

/// Types `key` at the cursor, or deletes the character before it for a
/// backspace, and answers the edit.
fn press(text: &mut String, cursor: &mut usize, key: char) -> Edit {
    if key != '\u{8}' {
        text.insert(*cursor, key);
        *cursor += key.len_utf8();
        return Edit {
            old: *cursor - key.len_utf8()..*cursor - key.len_utf8(),
            new_len: key.len_utf8(),
        };
    }
    let before = text[..*cursor]
        .chars()
        .next_back()
        .map_or(0, char::len_utf8);
    let start = *cursor - before;
    text.replace_range(start..*cursor, "");
    let edit = Edit {
        old: start..*cursor,
        new_len: 0,
    };
    *cursor = start;
    edit
}

#[test]
fn a_kept_plan_follows_the_cursor_through_the_corpus() {
    let scopes = [RevealScope::Element, RevealScope::Line, RevealScope::Block];
    let mut every_settings: Vec<RevealSettings> = scopes
        .iter()
        .map(|&scope| RevealSettings::new(RevealMode::AroundCursor { scope }))
        .collect();
    every_settings.push(reveal_settings(&SymbolSettings::default()));
    for (path, text) in corpus_notes() {
        let tree = parse(&text);
        for settings in &every_settings {
            let mut kept = KeptPlan::default();
            let stops = (0..=text.len())
                .step_by(CURSOR_STEP)
                .filter(|&at| text.is_char_boundary(at));
            for at in stops {
                let width = (at / CURSOR_STEP % 2) * 40;
                let selection = at..(at + width).min(text.len());
                let selection = match text.is_char_boundary(selection.end) {
                    true => selection,
                    false => at..at,
                };
                let selections = [selection];
                let input = RenderInput {
                    text: &text,
                    tree: &tree,
                    selections: &selections,
                    settings,
                };
                let fresh = plan(&input).lines;
                assert!(
                    kept.plan(&input) == &fresh[..],
                    "{path} at {at}, {settings:?}"
                );
            }
        }
    }
}
