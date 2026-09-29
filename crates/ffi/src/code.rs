//! Syntax colours for a note's fenced code blocks, as the desktop draws
//! them: each stretch of code named by what it is, for the phone to colour
//! with the theme's `color.code.*` tokens. Blocks are highlighted once per
//! text, and the note is only locked while its blocks are copied out.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Mutex, PoisonError};

use gasp_core::syntax::NodeKind;
use gasp_highlight::{CodeKind, Spans, highlight_block};

use crate::document::NoteDocument;
use crate::offsets::{TextRange, Utf16Offsets};

/// Blocks whose colours are kept before the cache starts over.
const CACHE_LIMIT: usize = 256;

static HIGHLIGHTED: Mutex<Option<HashMap<u64, Vec<Spans>>>> = Mutex::new(None);

/// What a stretch of code is, which picks its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum CodeColor {
    Comment,
    String,
    Number,
    Constant,
    Keyword,
    Function,
    Type,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CodeSpan {
    pub range: TextRange,
    pub color: CodeColor,
}

/// A fenced block's code copied out of the note.
struct Block {
    language: String,
    code: String,
    /// Where the code starts in the note, in UTF-16.
    start: u32,
}

#[uniffi::export]
impl NoteDocument {
    /// The coloured stretches of every fenced code block that names a
    /// language with a grammar. The grammars load on the first call, which
    /// takes a moment: call it off the main thread.
    pub fn code_spans(&self) -> Vec<CodeSpan> {
        code_blocks(self).iter().flat_map(block_spans).collect()
    }
}

/// The theme token that colours `color`, such as `color.code.keyword`.
#[uniffi::export]
pub fn code_color_token(color: CodeColor) -> String {
    code_kind(color).token().to_owned()
}

fn code_blocks(document: &NoteDocument) -> Vec<Block> {
    let parsed = document.lock();
    let tree = &parsed.tree;
    tree.preorder()
        .into_iter()
        .filter_map(|id| {
            let node = tree.node(id);
            let NodeKind::CodeBlock(info) = &node.kind else {
                return None;
            };
            let language = info.language.clone().filter(|_| info.fenced)?;
            let open = node.markup.first()?;
            let start = (open.range.end + 1).min(node.range.end);
            let end = node
                .markup
                .get(1)
                .map_or(node.range.end, |close| close.range.start)
                .max(start);
            Some(Block {
                language,
                code: parsed.text[start..end].to_owned(),
                start: parsed.offsets.utf16(start),
            })
        })
        .collect()
}

fn block_spans(block: &Block) -> Vec<CodeSpan> {
    let Some(lines) = highlighted(block) else {
        return Vec::new();
    };
    let offsets = Utf16Offsets::new(&block.code);
    let mut line_start = 0;
    let mut spans = Vec::new();
    for (line, line_spans) in block.code.split('\n').zip(lines) {
        for (range, kind) in line_spans {
            let range = offsets.range(&(line_start + range.start..line_start + range.end));
            spans.push(CodeSpan {
                range: TextRange {
                    start: block.start + range.start,
                    end: block.start + range.end,
                },
                color: code_color(kind),
            });
        }
        line_start += line.len() + 1;
    }
    spans
}

/// The block's colours, from the cache when its code hasn't changed.
fn highlighted(block: &Block) -> Option<Vec<Spans>> {
    let mut hasher = DefaultHasher::new();
    (&block.language, &block.code).hash(&mut hasher);
    let key = hasher.finish();
    let cached = HIGHLIGHTED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .and_then(|cache| cache.get(&key).cloned());
    if cached.is_some() {
        return cached;
    }
    let lines = highlight_block(&block.language, block.code.split('\n'))?;
    let mut cache = HIGHLIGHTED.lock().unwrap_or_else(PoisonError::into_inner);
    let cache = cache.get_or_insert_with(HashMap::new);
    if cache.len() > CACHE_LIMIT {
        cache.clear();
    }
    cache.insert(key, lines.clone());
    Some(lines)
}

const COLORS: [(CodeKind, CodeColor); 7] = [
    (CodeKind::Comment, CodeColor::Comment),
    (CodeKind::String, CodeColor::String),
    (CodeKind::Number, CodeColor::Number),
    (CodeKind::Constant, CodeColor::Constant),
    (CodeKind::Keyword, CodeColor::Keyword),
    (CodeKind::Function, CodeColor::Function),
    (CodeKind::Type, CodeColor::Type),
];

fn code_color(kind: CodeKind) -> CodeColor {
    COLORS
        .iter()
        .find(|(candidate, _)| *candidate == kind)
        .map_or(CodeColor::Constant, |(_, color)| *color)
}

fn code_kind(color: CodeColor) -> CodeKind {
    COLORS
        .iter()
        .find(|(_, candidate)| *candidate == color)
        .map_or(CodeKind::Constant, |(kind, _)| *kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rust_block_gets_its_keywords_and_strings_in_utf16() {
        let text = "é\n\n```rust\nfn main() { let s = \"hi\"; }\n```\n\n```\nplain\n```\n";
        let document = NoteDocument::new(text.into());
        let spans = document.code_spans();
        let code_start = text
            .encode_utf16()
            .position(|unit| unit == u16::from(b'f'))
            .unwrap() as u32;
        let keyword = spans
            .iter()
            .find(|span| span.color == CodeColor::Keyword)
            .unwrap();
        assert_eq!(keyword.range.start, code_start);
        assert!(spans.iter().any(|span| span.color == CodeColor::String));
        assert_eq!(code_color_token(CodeColor::Keyword), "color.code.keyword");
    }
}
