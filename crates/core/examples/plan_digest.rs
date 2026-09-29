//! Prints one digest per corpus note of everything the parser and planner
//! produce for it: the tree, the tree after typing into it edit by edit,
//! and plans for many cursor positions in each reveal mode. Two builds
//! that print the same digests produce the same output, which is how an
//! optimization shows it changed nothing.
//!
//! `cargo run --release -p gasp-core --example plan_digest > digests.txt`

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use gasp_bench::corpus::{corpus_notes, long_note};
use gasp_config::settings::{SymbolMode, SymbolSettings};
use gasp_core::render::folds::heading_sections;
use gasp_core::render::{RenderInput, plan, plan_lines, reveal_settings};
use gasp_core::syntax::{self, Edit, SyntaxTree};

const CURSOR_STEP: usize = 97;
const TYPED: &str = "a *b* $x^2$ [[c]] ";

fn main() {
    let mut notes = corpus_notes();
    notes.push(("long note".to_owned(), long_note(60 * 1024)));
    for (path, text) in &notes {
        let mut hasher = DefaultHasher::new();
        digest_note(text, &mut hasher);
        println!("{:016x}  {path}", hasher.finish());
    }
}

fn digest_note(text: &str, hasher: &mut DefaultHasher) {
    let tree = syntax::parse(text);
    format!("{tree:?}").hash(hasher);
    for mode in [
        SymbolMode::AroundCursor,
        SymbolMode::AlwaysShown,
        SymbolMode::AlwaysHidden,
    ] {
        let settings = reveal_settings(&SymbolSettings {
            mode,
            ..SymbolSettings::default()
        });
        for cursor in cursors(text) {
            let selection = cursor..cursor;
            let selections = [selection];
            let input = RenderInput {
                text,
                tree: &tree,
                selections: &selections,
                settings: &settings,
            };
            format!("{:?}", plan(&input)).hash(hasher);
            let line = tree.lines().line_of(cursor);
            let viewport = line.saturating_sub(10)..line + 10;
            format!("{:?}", plan_lines(&input, viewport)).hash(hasher);
        }
    }
    format!("{:?}", heading_sections(&tree)).hash(hasher);
    typed_trees(text, hasher);
}

fn cursors(text: &str) -> impl Iterator<Item = usize> + '_ {
    (0..=text.len())
        .step_by(CURSOR_STEP)
        .filter(|&at| text.is_char_boundary(at))
}

/// Types into the note at a few places, one character at a time, and
/// hashes each incrementally edited tree.
fn typed_trees(text: &str, hasher: &mut DefaultHasher) {
    for at in cursors(text).step_by(7) {
        let mut text = text.to_owned();
        let mut tree = syntax::parse(&text);
        let mut cursor = at;
        for key in TYPED.chars() {
            text.insert(cursor, key);
            let edit = Edit {
                old: cursor..cursor,
                new_len: key.len_utf8(),
            };
            tree.edit(&text, &edit);
            cursor += key.len_utf8();
            hash_tree(&tree, hasher);
        }
    }
}

fn hash_tree(tree: &SyntaxTree, hasher: &mut DefaultHasher) {
    format!("{tree:?}").hash(hasher);
}
