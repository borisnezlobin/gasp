//! Snippet engine, the readable snippet format, and the typing replacements table.
//!
//! A snippet line reads `trigger → expansion  options`, for example
//! `{letter}{digit} → {letter}_{digit}  math, instant`. See [`parse_snippet`] for the
//! format and [`SnippetEngine`] for matching. [`Replacements`] is the merged Smart
//! Typography and Symbols Prettifier table.

mod context;
mod engine;
mod file;
mod format;
mod parse;
mod pattern;
mod preview;
mod replacements;
mod snippet;

pub use context::{InputContext, TriggerKey};
pub use engine::{CompileError, Request, SnippetEdit, SnippetEngine, TabStop};
pub use file::{FileLine, SnippetFile};
pub use format::{format_expansion, format_options, format_trigger};
pub use parse::{ParseError, decode_glyphs, parse_line, parse_snippet};
pub use pattern::{GREEK_NAMES, NamedPattern, SYMBOL_NAMES, expand_patterns_in_regex};
pub use preview::{Preview, preview};
pub use replacements::{
    Replacement, ReplacementEdit, ReplacementFire, Replacements, ReplacementsError,
};
pub use snippet::{
    ARROW, CaptureRef, Expansion, ExpansionPart, Fire, NEWLINE_GLYPH, Options, REGEX_PREFIX,
    SPACE_GLYPH, STOP_GLYPH, Scope, Snippet, StopMark, TAB_GLYPH, Trigger, TriggerPart,
};

/// The built-in snippets, used until a vault has its own `snippets.txt`.
pub const DEFAULT_SNIPPETS: &str = include_str!("../defaults/snippets.txt");

/// The built-in replacements, used until a vault has its own
/// `replacements.toml`.
pub const DEFAULT_REPLACEMENTS: &str = include_str!("../defaults/replacements.toml");

impl SnippetFile {
    /// The built-in snippets.
    pub fn builtin() -> SnippetFile {
        SnippetFile::parse(DEFAULT_SNIPPETS).unwrap_or_default()
    }
}

impl Replacements {
    /// The built-in replacements.
    pub fn builtin() -> Replacements {
        Replacements::from_toml(DEFAULT_REPLACEMENTS).unwrap_or_default()
    }
}

impl SnippetEngine {
    /// Compiles every snippet in a file, in file order.
    pub fn from_file(file: &SnippetFile) -> Result<SnippetEngine, CompileError> {
        SnippetEngine::new(file.snippets().cloned().collect())
    }
}

#[cfg(test)]
mod defaults_tests {
    use super::*;

    #[test]
    fn the_built_in_files_parse_and_compile() {
        let file = SnippetFile::parse(DEFAULT_SNIPPETS).expect("default snippets parse");
        assert!(file.snippets().count() > 200);
        SnippetEngine::from_file(&file).expect("default snippets compile");
        let table =
            Replacements::from_toml(DEFAULT_REPLACEMENTS).expect("default replacements parse");
        assert!(table.entries.iter().all(|entry| entry.closing.is_none()));
    }
}
