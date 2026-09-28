//! UniFFI surface for the iPhone app: a vault folder's notes, one open
//! note's render plan, and the theme. Offsets are in UTF-16 code units, as
//! UIKit counts them.

mod document;
mod offsets;
mod plan;
mod theme;
mod vault;

pub use document::NoteDocument;
pub use offsets::TextRange;
pub use plan::{
    InlineStyle, LineDecoration, LinePlan, NotePlan, Placement, StyledRun, TableRow, TextAlign,
    Widget, WidgetKind,
};
pub use theme::{Palette, Spacing, ThemeColor, ThemeTokens, Typography, built_in_theme};
pub use vault::{NoteSummary, VaultError, VaultFolder};

uniffi::setup_scaffolding!();
