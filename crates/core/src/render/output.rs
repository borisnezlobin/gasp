//! What the render planner hands to the apps.

use std::ops::Range;

use crate::syntax::{Alignment, CalloutKind, ConflictSide, Fold};

/// A semantic style for a run of text. Themes map these to concrete values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StyleKey {
    Strong,
    Emphasis,
    Strikethrough,
    Highlight,
    Underline,
    Code,
    CodeBlock,
    Heading(u8),
    Link,
    Tag,
    Comment,
    FootnoteRef,
    MathSource,
    Html,
    Frontmatter,
    /// A property's name in the frontmatter.
    FrontmatterKey,
    CalloutTitle,
    TaskDone,
    /// Markdown symbols that are shown, drawn fainter than the text.
    MarkupDimmed,
}

const HEADING_NAMES: [&str; 6] = [
    "heading-1",
    "heading-2",
    "heading-3",
    "heading-4",
    "heading-5",
    "heading-6",
];

impl StyleKey {
    /// The theme token name for this style.
    pub fn name(self) -> &'static str {
        match self {
            Self::Heading(level) => HEADING_NAMES[usize::from(level.clamp(1, 6)) - 1],
            other => other.plain_name(),
        }
    }

    fn plain_name(self) -> &'static str {
        STYLE_NAMES
            .iter()
            .find(|(key, _)| *key == self)
            .map_or("text", |(_, name)| name)
    }
}

const STYLE_NAMES: &[(StyleKey, &str)] = &[
    (StyleKey::Strong, "strong"),
    (StyleKey::Emphasis, "emphasis"),
    (StyleKey::Strikethrough, "strikethrough"),
    (StyleKey::Highlight, "highlight"),
    (StyleKey::Underline, "underline"),
    (StyleKey::Code, "code"),
    (StyleKey::CodeBlock, "code-block"),
    (StyleKey::Link, "link"),
    (StyleKey::Tag, "tag"),
    (StyleKey::Comment, "comment"),
    (StyleKey::FootnoteRef, "footnote-ref"),
    (StyleKey::MathSource, "math-source"),
    (StyleKey::Html, "html"),
    (StyleKey::Frontmatter, "frontmatter"),
    (StyleKey::FrontmatterKey, "frontmatter-key"),
    (StyleKey::CalloutTitle, "callout-title"),
    (StyleKey::TaskDone, "task-done"),
    (StyleKey::MarkupDimmed, "markup-dimmed"),
];

/// A decoration for a whole line, such as the bar beside a quote.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LineStyle {
    Heading(u8),
    /// One per enclosing quote; `depth` starts at 1.
    Quote {
        depth: usize,
    },
    Callout {
        kind: CalloutKind,
        depth: usize,
    },
    CalloutHeader {
        kind: CalloutKind,
    },
    /// `index` is the zero-based line within the code block, fences included.
    CodeBlock {
        index: usize,
    },
    MathBlock,
    Table,
    Frontmatter,
    /// A frontmatter line shown as a property while the cursor is outside
    /// the frontmatter: its key, if `keyed`, then its value in a column.
    Property {
        keyed: bool,
    },
    Comment,
    FootnoteDefinition,
    /// A line of one version of a sync conflict, markers included.
    Conflict {
        side: ConflictSide,
    },
}

/// A run of source text sharing one set of styles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyledRun {
    pub range: Range<usize>,
    /// Sorted and without duplicates. Empty means plain text.
    pub styles: Vec<StyleKey>,
}

/// Where a widget goes relative to its source range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// Drawn instead of the range, which is also listed as hidden.
    Replace,
    /// Drawn after the range, on its own row below its last line.
    Below,
    /// Drawn above the range's first line, like the live math preview.
    Above,
}

/// Something drawn by the app rather than as text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Widget {
    pub kind: WidgetKind,
    pub range: Range<usize>,
    pub placement: Placement,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WidgetKind {
    /// Rendered inline math. `tex` is the source between the delimiters.
    InlineMath {
        tex: String,
        display: bool,
    },
    /// A rendered `$$` block, with quote markers removed from `tex`.
    MathBlock {
        tex: String,
    },
    /// The live preview shown while the cursor is in math.
    MathPreview {
        tex: String,
        display: bool,
    },
    /// A Markdown image, `![[embed]]` or `<img>`.
    Image {
        target: String,
        alt: String,
        width: Option<u32>,
        height: Option<u32>,
        embed: bool,
    },
    HorizontalRule,
    /// A task checkbox; toggling it rewrites the marker at the widget's range.
    Checkbox {
        checked: bool,
    },
    /// A bullet or number drawn instead of a list marker.
    ListBullet {
        ordered: bool,
        number: Option<u64>,
        depth: usize,
    },
    CalloutHeader {
        kind: CalloutKind,
        type_name: String,
        /// The title text's range, when the header has one; it keeps its
        /// own styled runs.
        title: Option<Range<usize>>,
        default_title: String,
        fold: Option<Fold>,
        /// Whether the body is collapsed right now.
        folded: bool,
    },
    Table {
        alignments: Vec<Alignment>,
        /// Cell content ranges, header row first.
        rows: Vec<Vec<Range<usize>>>,
    },
    CodeBlock {
        language: Option<String>,
        title: Option<String>,
        line_numbers: Option<bool>,
        highlighted_lines: Vec<(u32, u32)>,
        /// The code between the fences.
        content: Range<usize>,
    },
    FootnoteSuperscript {
        label: String,
    },
    /// Names the version of a sync conflict that starts on this line,
    /// drawn instead of its marker.
    ConflictLabel {
        side: ConflictSide,
    },
    LineBreak,
    /// The `#` between a wikilink's note and its heading or block, drawn
    /// as `›` so `[[Waves#Questions]]` reads "Waves › Questions".
    SubpathSeparator,
    /// A Link Embed `embed` block drawn as a card.
    LinkCard(crate::link_card::LinkCard),
}

/// The plan for one source line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinePlan {
    /// Zero-based line number.
    pub line: usize,
    /// The line's text, without its terminator.
    pub range: Range<usize>,
    pub line_styles: Vec<LineStyle>,
    /// Runs covering `range` exactly, in order.
    pub runs: Vec<StyledRun>,
    /// Ranges not drawn as text, sorted and merged.
    pub hidden: Vec<Range<usize>>,
    /// Widgets anchored on this line.
    pub widgets: Vec<Widget>,
    /// The line takes no space: a widget on an earlier line replaces it, or
    /// it is inside a folded callout.
    pub collapsed: bool,
}

/// The plan for a range of lines.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderPlan {
    pub lines: Vec<LinePlan>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_names_are_theme_tokens() {
        assert_eq!(StyleKey::Heading(2).name(), "heading-2");
        assert_eq!(StyleKey::FootnoteRef.name(), "footnote-ref");
        assert_eq!(StyleKey::MarkupDimmed.name(), "markup-dimmed");
        assert_eq!(STYLE_NAMES.len(), 18);
    }
}
