//! The render planner's output in UTF-16 offsets, for Swift to draw.

use gasp_core::render::{self, StyleKey};
use gasp_core::syntax::{Alignment, CalloutKind, ConflictSide};

use crate::offsets::{TextRange, Utf16Offsets};

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct NotePlan {
    pub lines: Vec<LinePlan>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct LinePlan {
    /// Zero-based line number.
    pub line: u32,
    /// The line's text, without its terminator.
    pub range: TextRange,
    pub decorations: Vec<LineDecoration>,
    /// Runs covering `range` exactly, in order.
    pub runs: Vec<StyledRun>,
    /// Text not drawn, sorted and merged.
    pub hidden: Vec<TextRange>,
    pub widgets: Vec<Widget>,
    /// The line takes no room, such as a table's delimiter row.
    pub collapsed: bool,
    pub table_row: Option<TableRow>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct StyledRun {
    pub range: TextRange,
    /// Empty means plain text.
    pub styles: Vec<InlineStyle>,
}

/// How a run of text is styled. Themes give each its look.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum InlineStyle {
    Strong,
    Emphasis,
    Strikethrough,
    Highlight,
    Underline,
    Code,
    CodeBlock,
    Heading {
        level: u8,
    },
    Link,
    Tag,
    Comment,
    FootnoteRef,
    MathSource,
    MathBracket {
        depth: u8,
    },
    Html,
    Frontmatter,
    FrontmatterKey,
    CalloutTitle,
    TaskDone,
    MarkupDimmed,
    PropertyChip,
    Superscript,
    Subscript,
    Kbd,
    /// A colour the note's HTML asks for, as `0xRRGGBBAA`.
    TextColor {
        rgba: u32,
    },
    TextBackground {
        rgba: u32,
    },
    /// A size the note's HTML asks for, as a percentage of the line's text.
    FontScale {
        percent: u16,
    },
}

const PLAIN_STYLES: &[(StyleKey, InlineStyle)] = &[
    (StyleKey::Strong, InlineStyle::Strong),
    (StyleKey::Emphasis, InlineStyle::Emphasis),
    (StyleKey::Strikethrough, InlineStyle::Strikethrough),
    (StyleKey::Highlight, InlineStyle::Highlight),
    (StyleKey::Underline, InlineStyle::Underline),
    (StyleKey::Code, InlineStyle::Code),
    (StyleKey::CodeBlock, InlineStyle::CodeBlock),
    (StyleKey::Link, InlineStyle::Link),
    (StyleKey::Tag, InlineStyle::Tag),
    (StyleKey::Comment, InlineStyle::Comment),
    (StyleKey::FootnoteRef, InlineStyle::FootnoteRef),
    (StyleKey::MathSource, InlineStyle::MathSource),
    (StyleKey::Html, InlineStyle::Html),
    (StyleKey::Frontmatter, InlineStyle::Frontmatter),
    (StyleKey::FrontmatterKey, InlineStyle::FrontmatterKey),
    (StyleKey::CalloutTitle, InlineStyle::CalloutTitle),
    (StyleKey::TaskDone, InlineStyle::TaskDone),
    (StyleKey::MarkupDimmed, InlineStyle::MarkupDimmed),
    (StyleKey::PropertyChip, InlineStyle::PropertyChip),
    (StyleKey::Superscript, InlineStyle::Superscript),
    (StyleKey::Subscript, InlineStyle::Subscript),
    (StyleKey::Kbd, InlineStyle::Kbd),
];

fn inline_style(key: StyleKey) -> InlineStyle {
    match key {
        StyleKey::Heading(level) => InlineStyle::Heading { level },
        StyleKey::MathBracket(depth) => InlineStyle::MathBracket { depth },
        StyleKey::TextColor { rgba, .. } => InlineStyle::TextColor { rgba },
        StyleKey::TextBackground { rgba, .. } => InlineStyle::TextBackground { rgba },
        StyleKey::FontScale { percent, .. } => InlineStyle::FontScale { percent },
        plain => PLAIN_STYLES
            .iter()
            .find(|(candidate, _)| *candidate == plain)
            .map_or(InlineStyle::Html, |(_, style)| *style),
    }
}

/// A decoration for a whole line, such as a quote's bar.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LineDecoration {
    Heading {
        level: u8,
    },
    /// One per enclosing quote; `depth` starts at 1.
    Quote {
        depth: u32,
    },
    /// `kind` names the callout's colour token, such as `warning`.
    Callout {
        kind: String,
        depth: u32,
    },
    CalloutHeader {
        kind: String,
    },
    /// `index` is the line within the code block, fences included.
    CodeBlock {
        index: u32,
    },
    MathBlock,
    Table,
    Frontmatter,
    Property {
        keyed: bool,
    },
    Comment,
    FootnoteDefinition,
    Conflict {
        this_device: bool,
    },
    Align {
        alignment: TextAlign,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TextAlign {
    Natural,
    Left,
    Center,
    Right,
}

fn text_alignment(alignment: Alignment) -> TextAlign {
    match alignment {
        Alignment::None => TextAlign::Natural,
        Alignment::Left => TextAlign::Left,
        Alignment::Center => TextAlign::Center,
        Alignment::Right => TextAlign::Right,
    }
}

/// The theme token that names a callout's colour, `color.callout.<kind>`.
fn callout_token(kind: CalloutKind) -> String {
    format!("{kind:?}").to_lowercase()
}

fn line_decoration(style: &render::LineStyle) -> LineDecoration {
    use render::LineStyle as Core;
    match *style {
        Core::Heading(level) => LineDecoration::Heading { level },
        Core::Quote { depth } => LineDecoration::Quote {
            depth: depth as u32,
        },
        Core::Callout { kind, depth } => LineDecoration::Callout {
            kind: callout_token(kind),
            depth: depth as u32,
        },
        Core::CalloutHeader { kind } => LineDecoration::CalloutHeader {
            kind: callout_token(kind),
        },
        Core::CodeBlock { index } => LineDecoration::CodeBlock {
            index: index as u32,
        },
        Core::MathBlock => LineDecoration::MathBlock,
        Core::Table => LineDecoration::Table,
        Core::Frontmatter => LineDecoration::Frontmatter,
        Core::Property { keyed } => LineDecoration::Property { keyed },
        Core::Comment => LineDecoration::Comment,
        Core::FootnoteDefinition => LineDecoration::FootnoteDefinition,
        Core::Conflict { side } => LineDecoration::Conflict {
            this_device: side == ConflictSide::ThisDevice,
        },
        Core::Align(alignment) => LineDecoration::Align {
            alignment: text_alignment(alignment),
        },
    }
}

/// Where a widget goes relative to its source range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Placement {
    /// Drawn instead of the range, which is also hidden.
    Replace,
    /// On its own row below the range's last line.
    Below,
    /// Above the range's first line.
    Above,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Widget {
    pub kind: WidgetKind,
    pub range: TextRange,
    pub placement: Placement,
}

/// Something drawn by the app rather than as text.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum WidgetKind {
    InlineMath {
        tex: String,
        display: bool,
    },
    MathBlock {
        tex: String,
    },
    MathPreview {
        tex: String,
        display: bool,
    },
    Image {
        target: String,
        alt: String,
        width: Option<u32>,
        height: Option<u32>,
        embed: bool,
    },
    HorizontalRule,
    Checkbox {
        checked: bool,
    },
    ListBullet {
        ordered: bool,
        number: Option<u64>,
        depth: u32,
    },
    CalloutHeader {
        kind: String,
        type_name: String,
        title: Option<TextRange>,
        default_title: String,
        folded: bool,
    },
    CodeBlock {
        language: Option<String>,
        title: Option<String>,
        content: TextRange,
    },
    FootnoteSuperscript {
        label: String,
    },
    ConflictLabel {
        this_device: bool,
    },
    LineBreak,
    SubpathSeparator,
    LinkCard {
        url: String,
        title: String,
    },
    PropertyList {
        items: Vec<String>,
        tags: bool,
    },
    EmptyTabStop,
}

fn widget(widget: &render::Widget, offsets: &Utf16Offsets) -> Widget {
    Widget {
        kind: widget_kind(&widget.kind, offsets),
        range: offsets.range(&widget.range),
        placement: match widget.placement {
            render::Placement::Replace => Placement::Replace,
            render::Placement::Below => Placement::Below,
            render::Placement::Above => Placement::Above,
        },
    }
}

fn widget_kind(kind: &render::WidgetKind, offsets: &Utf16Offsets) -> WidgetKind {
    use render::WidgetKind as Core;
    match kind {
        Core::CalloutHeader {
            kind,
            type_name,
            title,
            default_title,
            folded,
            ..
        } => WidgetKind::CalloutHeader {
            kind: callout_token(*kind),
            type_name: type_name.clone(),
            title: title.as_ref().map(|range| offsets.range(range)),
            default_title: default_title.clone(),
            folded: *folded,
        },
        Core::CodeBlock {
            language,
            title,
            content,
            ..
        } => WidgetKind::CodeBlock {
            language: language.clone(),
            title: title.clone(),
            content: offsets.range(content),
        },
        other => content_widget_kind(other),
    }
}

/// Widgets whose description carries no offsets.
fn content_widget_kind(kind: &render::WidgetKind) -> WidgetKind {
    use render::WidgetKind as Core;
    match kind {
        Core::InlineMath { tex, display } => WidgetKind::InlineMath {
            tex: tex.clone(),
            display: *display,
        },
        Core::MathBlock { tex } => WidgetKind::MathBlock { tex: tex.clone() },
        Core::MathPreview { tex, display } => WidgetKind::MathPreview {
            tex: tex.clone(),
            display: *display,
        },
        Core::Image {
            target,
            alt,
            width,
            height,
            embed,
        } => WidgetKind::Image {
            target: target.clone(),
            alt: alt.clone(),
            width: *width,
            height: *height,
            embed: *embed,
        },
        Core::ListBullet {
            ordered,
            number,
            depth,
        } => WidgetKind::ListBullet {
            ordered: *ordered,
            number: *number,
            depth: *depth as u32,
        },
        other => marker_widget_kind(other),
    }
}

/// Widgets drawn in place of a small piece of markup.
fn marker_widget_kind(kind: &render::WidgetKind) -> WidgetKind {
    use render::WidgetKind as Core;
    match kind {
        Core::Checkbox { checked } => WidgetKind::Checkbox { checked: *checked },
        Core::FootnoteSuperscript { label } => WidgetKind::FootnoteSuperscript {
            label: label.clone(),
        },
        Core::ConflictLabel { side } => WidgetKind::ConflictLabel {
            this_device: *side == ConflictSide::ThisDevice,
        },
        Core::LinkCard(card) => WidgetKind::LinkCard {
            url: card.url.clone(),
            title: card.title.clone(),
        },
        Core::PropertyList { items, tags } => WidgetKind::PropertyList {
            items: items.clone(),
            tags: *tags,
        },
        Core::HorizontalRule => WidgetKind::HorizontalRule,
        Core::LineBreak => WidgetKind::LineBreak,
        Core::SubpathSeparator => WidgetKind::SubpathSeparator,
        _ => WidgetKind::EmptyTabStop,
    }
}

/// A table row drawn as a row of the table's grid.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TableRow {
    /// 0 is the header. The delimiter row isn't a row.
    pub index: u32,
    pub count: u32,
    pub alignments: Vec<TextAlign>,
    /// Each cell's text.
    pub cells: Vec<TextRange>,
    /// Where the table starts, which tells tables apart.
    pub table_start: u32,
}

fn table_row(row: &render::TableRowPlan, offsets: &Utf16Offsets) -> TableRow {
    TableRow {
        index: row.index as u32,
        count: row.count as u32,
        alignments: row.alignments.iter().copied().map(text_alignment).collect(),
        cells: offsets.ranges(&row.cells),
        table_start: offsets.utf16(row.table_start),
    }
}

fn line_plan(line: &render::LinePlan, offsets: &Utf16Offsets) -> LinePlan {
    LinePlan {
        line: line.line as u32,
        range: offsets.range(&line.range),
        decorations: line.line_styles.iter().map(line_decoration).collect(),
        runs: line
            .runs
            .iter()
            .map(|run| StyledRun {
                range: offsets.range(&run.range),
                styles: run.styles.iter().copied().map(inline_style).collect(),
            })
            .collect(),
        hidden: offsets.ranges(&line.hidden),
        widgets: line
            .widgets
            .iter()
            .map(|each| widget(each, offsets))
            .collect(),
        collapsed: line.collapsed,
        table_row: line.table_row.as_ref().map(|row| table_row(row, offsets)),
    }
}

pub(crate) fn note_plan(plan: &render::RenderPlan, offsets: &Utf16Offsets) -> NotePlan {
    NotePlan {
        lines: plan
            .lines
            .iter()
            .map(|line| line_plan(line, offsets))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::NoteDocument;

    fn plan_at(text: &str, cursor: u32) -> NotePlan {
        NoteDocument::new(text.into()).plan(TextRange {
            start: cursor,
            end: cursor,
        })
    }

    #[test]
    fn every_plain_style_has_its_own_swift_case() {
        let mut styles: Vec<InlineStyle> = PLAIN_STYLES.iter().map(|(_, style)| *style).collect();
        styles.dedup();
        assert_eq!(styles.len(), PLAIN_STYLES.len());
        assert_eq!(inline_style(StyleKey::Kbd), InlineStyle::Kbd);
    }

    #[test]
    fn a_heading_away_from_the_cursor_hides_its_marker() {
        let plan = plan_at("# Café\n\nbody", 12);
        let heading = &plan.lines[0];
        assert_eq!(
            heading.decorations,
            vec![LineDecoration::Heading { level: 1 }]
        );
        assert_eq!(heading.hidden, vec![TextRange { start: 0, end: 2 }]);
        assert_eq!(heading.range, TextRange { start: 0, end: 6 });
    }

    #[test]
    fn markup_shows_again_when_the_cursor_is_in_it() {
        let text = "a **bold** word";
        let away = plan_at(text, 0);
        let inside = plan_at(text, 5);
        assert_eq!(
            away.lines[0].hidden,
            vec![
                TextRange { start: 2, end: 4 },
                TextRange { start: 8, end: 10 }
            ]
        );
        assert!(inside.lines[0].hidden.is_empty());
        let bold = away.lines[0]
            .runs
            .iter()
            .find(|run| run.styles.contains(&InlineStyle::Strong))
            .unwrap();
        assert_eq!(bold.range, TextRange { start: 2, end: 10 });
    }

    #[test]
    fn offsets_after_wide_characters_are_in_utf16() {
        let text = "𝜋 *it*";
        let plan = plan_at(text, 0);
        let italic = plan.lines[0]
            .runs
            .iter()
            .find(|run| run.styles.contains(&InlineStyle::Emphasis))
            .unwrap();
        assert_eq!(italic.range, TextRange { start: 3, end: 7 });
        assert_eq!(plan.lines[0].hidden[0], TextRange { start: 3, end: 4 });
    }

    #[test]
    fn task_markers_become_checkboxes() {
        let plan = plan_at("- [x] done\n\nnext", 14);
        let kinds: Vec<&WidgetKind> = plan.lines[0].widgets.iter().map(|w| &w.kind).collect();
        assert!(kinds.contains(&&WidgetKind::Checkbox { checked: true }));
    }
}
