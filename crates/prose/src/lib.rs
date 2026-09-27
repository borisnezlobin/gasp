//! Reading a note as prose: which blocks hold sentences, where each
//! sentence ends and how long it is, and the first two layers of the
//! grammar checker (mechanical checks and spelling that learns from the
//! vault).
//!
//! Everything works one unit (a paragraph, list item or heading) at a
//! time, so an editor can cache results per paragraph and redo only the
//! ones that change.

pub mod grammar;
pub mod markdown;
pub mod projection;
pub mod segment;
pub mod vocabulary;

pub use grammar::{CheckOptions, Checker, English, Flag, FlagKind};
pub use markdown::{Purpose, Unit, sentence_lengths, units};
pub use segment::{Length, Sentence, Thresholds, sentences, word_count};
