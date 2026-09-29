//! UniFFI surface for the iPhone app: a vault folder and everything done
//! with its notes, one open note's render plan and editing commands, the
//! command registry, the theme, and sync with the notes repository. Offsets are in UTF-16 code units, as
//! UIKit counts them.

mod commands;
mod display;
mod document;
mod edits;
mod export;
mod knowledge;
mod notes;
mod offsets;
mod plan;
mod recovery;
mod settings;
mod sync;
mod sync_conflicts;
mod sync_setup;
#[cfg(test)]
mod sync_tests;
mod theme;
mod vault;

pub use commands::{CommandInfo, KeyBinding};
pub use display::SymbolVisibility;
pub use document::{NoteDocument, OutlineHeading, SentenceLength, SentenceTint};
pub use edits::{CommandOutcome, TextReplacement};
pub use export::ExportedFile;
pub use knowledge::{SearchHit, SearchResult, TagInfo};
pub use notes::LinkDestination;
pub use offsets::TextRange;
pub use plan::{
    InlineStyle, LineDecoration, LinePlan, NotePlan, Placement, StyledRun, TableRow, TextAlign,
    Widget, WidgetKind,
};
pub use recovery::{SnapshotInfo, use_data_folder};
pub use settings::{SettingControl, SettingItem, SettingValue};
pub use sync::{SyncOutcome, SyncOverview, SyncPhaseKind, SyncRunSummary, VaultSync};
pub use sync_conflicts::{ConflictNote, ConflictPlace, PlaceChoice, merge_note_edits};
pub use sync_setup::{SyncSetup, repository_url, set_up_sync};
pub use theme::{Palette, Spacing, ThemeColor, ThemeTokens, Typography, built_in_theme};
pub use vault::{NoteSummary, OpenTabs, VaultError, VaultFolder};

uniffi::setup_scaffolding!();
