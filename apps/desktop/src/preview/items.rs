//! Turns a line plan into what the row builder places: visible text runs,
//! inline widgets, forced breaks and whole-row blocks, in source order.
//! Ranges become relative to the line.

use std::ops::Range;

use editor_core::render::{LinePlan, LineStyle, Placement, StyleKey, WidgetKind};

use crate::styling::LineTone;

/// Something the row builder places.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Text {
        range: Range<usize>,
        styles: Vec<StyleKey>,
    },
    Inline {
        range: Range<usize>,
        kind: WidgetKind,
    },
    /// A `<br>`: the next item starts a new row.
    Break { range: Range<usize> },
    Block {
        range: Range<usize>,
        kind: WidgetKind,
    },
}

impl Item {
    pub fn start(&self) -> usize {
        match self {
            Item::Text { range, .. }
            | Item::Inline { range, .. }
            | Item::Break { range }
            | Item::Block { range, .. } => range.start,
        }
    }
}

/// A widget placed above or below the line, with a line-relative range.
#[derive(Clone, Debug, PartialEq)]
pub struct Attached {
    pub range: Range<usize>,
    pub kind: WidgetKind,
}

/// Everything drawn for one line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LineItems {
    pub items: Vec<Item>,
    pub above: Vec<Attached>,
    pub below: Vec<Attached>,
}

/// Sorts a plan's runs and widgets into items.
pub fn line_items(plan: &LinePlan) -> LineItems {
    let line = &plan.range;
    let relative = |range: &Range<usize>| {
        range.start.clamp(line.start, line.end) - line.start
            ..range.end.clamp(line.start, line.end) - line.start
    };
    let mut result = LineItems::default();
    for run in &plan.runs {
        for visible in visible_parts(&run.range, &plan.hidden) {
            result.items.push(Item::Text {
                range: relative(&visible),
                styles: run.styles.clone(),
            });
        }
    }
    let has_checkbox = plan
        .widgets
        .iter()
        .any(|widget| matches!(widget.kind, WidgetKind::Checkbox { .. }));
    for widget in &plan.widgets {
        let range = relative(&widget.range);
        let kind = widget.kind.clone();
        match widget.placement {
            Placement::Above => result.above.push(Attached { range, kind }),
            Placement::Below => result.below.push(Attached { range, kind }),
            Placement::Replace => {
                let bullet_of_task = has_checkbox && matches!(kind, WidgetKind::ListBullet { .. });
                if !bullet_of_task {
                    result.items.push(replacement_item(range, kind));
                }
            }
        }
    }
    result.items.sort_by_key(Item::start);
    result
}

fn replacement_item(range: Range<usize>, kind: WidgetKind) -> Item {
    match kind {
        WidgetKind::LineBreak => Item::Break { range },
        WidgetKind::InlineMath { display: false, .. }
        | WidgetKind::Image { .. }
        | WidgetKind::Checkbox { .. }
        | WidgetKind::ListBullet { .. }
        | WidgetKind::FootnoteSuperscript { .. }
        | WidgetKind::ConflictLabel { .. }
        | WidgetKind::CalloutHeader { .. } => Item::Inline { range, kind },
        _ => Item::Block { range, kind },
    }
}

/// `range` minus the sorted, merged `hidden` ranges.
pub fn visible_parts(range: &Range<usize>, hidden: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut parts = Vec::new();
    let mut at = range.start;
    for gap in hidden {
        if gap.end <= at {
            continue;
        }
        if gap.start >= range.end {
            break;
        }
        if gap.start > at {
            parts.push(at..gap.start);
        }
        at = gap.end;
    }
    if at < range.end {
        parts.push(at..range.end);
    }
    parts
}

/// How the line's text is set, from its line styles.
pub fn line_tone(plan: &LinePlan) -> LineTone {
    plan.line_styles
        .iter()
        .fold(LineTone::PLAIN, |tone, style| match *style {
            LineStyle::Heading(level) => LineTone {
                heading_level: level,
                ..tone
            },
            LineStyle::CodeBlock { .. } => LineTone { code: true, ..tone },
            LineStyle::Frontmatter => LineTone {
                small: true,
                muted: true,
                ..tone
            },
            LineStyle::CalloutHeader { kind } => LineTone {
                callout: Some(kind),
                ..tone
            },
            _ => tone,
        })
}

/// Whether a run's styles make it part of a heading.
pub fn heading_level(styles: &[StyleKey]) -> Option<u8> {
    styles.iter().find_map(|style| match style {
        StyleKey::Heading(level) => Some(*level),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use editor_core::render::{RenderInput, RevealSettings, plan};
    use editor_core::syntax::parse;

    use super::*;

    fn items_at(text: &str, cursor: usize, line: usize) -> LineItems {
        let tree = parse(text);
        let settings = RevealSettings::default();
        let plan = plan(&RenderInput {
            text,
            tree: &tree,
            selections: std::slice::from_ref(&(cursor..cursor)),
            settings: &settings,
        });
        line_items(&plan.lines[line])
    }

    fn texts(items: &LineItems) -> Vec<Range<usize>> {
        items
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Text { range, .. } => Some(range.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn hidden_markup_splits_text() {
        let items = items_at("x\na **b** c", 0, 1);
        assert_eq!(texts(&items), vec![0..2, 4..5, 7..9]);
    }

    #[test]
    fn widgets_sit_between_text_in_order() {
        let items = items_at("x\nsee $y$ now", 0, 1);
        let kinds: Vec<&str> = items
            .items
            .iter()
            .map(|item| match item {
                Item::Text { .. } => "text",
                Item::Inline { .. } => "inline",
                Item::Break { .. } => "break",
                Item::Block { .. } => "block",
            })
            .collect();
        assert_eq!(kinds, vec!["text", "inline", "text"]);
    }

    #[test]
    fn tasks_show_a_checkbox_without_a_bullet() {
        let items = items_at("- [ ] todo\n\nx", 12, 0);
        assert!(matches!(
            items.items[0],
            Item::Inline {
                kind: WidgetKind::Checkbox { checked: false },
                ..
            }
        ));
        assert_eq!(texts(&items), vec![6..10]);
    }

    #[test]
    fn math_under_the_cursor_gets_a_preview_above() {
        let items = items_at("see $y$ now", 5, 0);
        assert_eq!(items.above.len(), 1);
        let texts = texts(&items);
        assert!(texts.windows(2).all(|pair| pair[0].end == pair[1].start));
        assert_eq!((texts[0].start, texts[texts.len() - 1].end), (0, 11));
    }

    #[test]
    fn visible_parts_skip_hidden_ranges() {
        assert_eq!(
            visible_parts(&(0..10), &[2..4, 6..7]),
            vec![0..2, 4..6, 7..10]
        );
        assert_eq!(
            visible_parts(&(3..5), std::slice::from_ref(&(0..4))),
            vec![4..5]
        );
        assert!(visible_parts(&(3..5), std::slice::from_ref(&(0..9))).is_empty());
    }
}
