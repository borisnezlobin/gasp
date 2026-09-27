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
mod replacements;
mod snippet;

pub use context::{InputContext, TriggerKey};
pub use engine::{CompileError, Request, SnippetEdit, SnippetEngine, TabStop};
pub use file::{FileLine, SnippetFile};
pub use format::{format_expansion, format_options, format_trigger};
pub use parse::{ParseError, decode_glyphs, parse_line, parse_snippet};
pub use pattern::{GREEK_NAMES, NamedPattern, SYMBOL_NAMES, expand_patterns_in_regex};
pub use replacements::{
    Replacement, ReplacementEdit, ReplacementFire, Replacements, ReplacementsError,
};
pub use snippet::{
    ARROW, CaptureRef, Expansion, ExpansionPart, Fire, NEWLINE_GLYPH, Options, REGEX_PREFIX,
    SPACE_GLYPH, STOP_GLYPH, Scope, Snippet, StopMark, TAB_GLYPH, Trigger, TriggerPart,
};
