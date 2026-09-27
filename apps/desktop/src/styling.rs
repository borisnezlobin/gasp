//! Maps the render planner's semantic styles to fonts, colours and
//! decorations from the theme.

use editor_core::render::StyleKey;
use editor_core::syntax::CalloutKind;
use gpui::{Font, FontStyle, Hsla, Pixels, StrikethroughStyle, TextRun, UnderlineStyle};

use crate::theme::Theme;

/// What a line's text looks like before its runs' own styles apply.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineTone {
    /// 1 to 6 for headings, 0 for body text.
    pub heading_level: u8,
    /// Code block lines use the code font throughout.
    pub code: bool,
    /// Frontmatter and other compact blocks use the small size.
    pub small: bool,
    pub muted: bool,
    /// The callout whose header this line is, for the title colour.
    pub callout: Option<CalloutKind>,
}

impl LineTone {
    pub const PLAIN: LineTone = LineTone {
        heading_level: 0,
        code: false,
        small: false,
        muted: false,
        callout: None,
    };

    /// The size of the line's text without inline code.
    pub fn font_size(&self, theme: &Theme) -> Pixels {
        match (self.code, self.small) {
            (true, _) => theme.body_font_size * theme.code_scale,
            (false, true) => theme.small_font_size,
            _ => theme.font_size(self.heading_level),
        }
    }

    pub fn line_height_factor(&self, theme: &Theme) -> f32 {
        if self.code {
            theme.code_line_height_factor
        } else if self.heading_level > 0 {
            theme.heading_line_height_factor
        } else {
            theme.line_height_factor
        }
    }
}

/// Whether a run with these styles is set in the code font at code size.
pub fn is_code(styles: &[StyleKey]) -> bool {
    styles
        .iter()
        .any(|style| matches!(style, StyleKey::Code | StyleKey::MathSource))
}

/// The font size of a run.
pub fn run_font_size(styles: &[StyleKey], tone: &LineTone, theme: &Theme) -> Pixels {
    let size = tone.font_size(theme);
    if is_code(styles) && !tone.code {
        size * theme.code_scale
    } else {
        size
    }
}

/// A text run of `len` bytes with the given styles.
pub fn text_run(
    len: usize,
    styles: &[StyleKey],
    tone: &LineTone,
    marked: bool,
    theme: &Theme,
) -> TextRun {
    let color = run_color(styles, tone, theme);
    TextRun {
        len,
        font: run_font(styles, tone, theme),
        color,
        background_color: run_background(styles, theme),
        underline: run_underline(styles, marked, color, theme),
        strikethrough: run_strikethrough(styles, color, theme),
    }
}

fn run_font(styles: &[StyleKey], tone: &LineTone, theme: &Theme) -> Font {
    let has = |key: StyleKey| styles.contains(&key);
    let mut font = if tone.code || is_code(styles) {
        theme.code_font()
    } else {
        theme.body_font()
    };
    let bold = has(StyleKey::Strong) || tone.heading_level > 0 || is_heading(styles);
    if bold {
        font.weight = theme.bold_weight;
    } else if has(StyleKey::CalloutTitle) {
        font.weight = theme.medium_weight;
    }
    if has(StyleKey::Emphasis) {
        font.style = FontStyle::Italic;
    }
    font
}

fn is_heading(styles: &[StyleKey]) -> bool {
    styles
        .iter()
        .any(|style| matches!(style, StyleKey::Heading(_)))
}

/// Colour by the first style that sets one, in order of precedence.
const COLOR_STYLES: [StyleKey; 8] = [
    StyleKey::MarkupDimmed,
    StyleKey::Comment,
    StyleKey::Html,
    StyleKey::TaskDone,
    StyleKey::Link,
    StyleKey::FootnoteRef,
    StyleKey::Tag,
    StyleKey::FrontmatterKey,
];

fn run_color(styles: &[StyleKey], tone: &LineTone, theme: &Theme) -> Hsla {
    let keyed = COLOR_STYLES
        .iter()
        .find(|key| styles.contains(key))
        .map(|key| style_color(*key, theme));
    if let Some(color) = keyed {
        return color;
    }
    if let Some(depth) = bracket_depth(styles) {
        return theme.math_brackets[usize::from(depth % 3)];
    }
    match tone.callout {
        Some(kind) if styles.contains(&StyleKey::CalloutTitle) => theme.callout_color(kind),
        _ if tone.muted => theme.text_muted,
        _ if tone.heading_level > 0 || is_heading(styles) => theme.heading_text,
        _ if is_code(styles) || tone.code => theme.code_text,
        _ => theme.text,
    }
}

fn bracket_depth(styles: &[StyleKey]) -> Option<u8> {
    styles.iter().find_map(|style| match style {
        StyleKey::MathBracket(depth) => Some(*depth),
        _ => None,
    })
}

fn style_color(key: StyleKey, theme: &Theme) -> Hsla {
    match key {
        StyleKey::MarkupDimmed | StyleKey::Html => theme.markup_dimmed,
        StyleKey::Comment | StyleKey::TaskDone => theme.text_faint,
        StyleKey::Link | StyleKey::FootnoteRef | StyleKey::Tag => theme.link,
        _ => theme.text_muted,
    }
}

fn run_background(styles: &[StyleKey], theme: &Theme) -> Option<Hsla> {
    if styles.contains(&StyleKey::Highlight) {
        return Some(theme.highlight);
    }
    if styles.contains(&StyleKey::Code) {
        return Some(theme.code_background);
    }
    styles
        .contains(&StyleKey::Tag)
        .then_some(theme.tag_background)
}

fn run_underline(
    styles: &[StyleKey],
    marked: bool,
    color: Hsla,
    theme: &Theme,
) -> Option<UnderlineStyle> {
    let underlined = marked
        || styles.contains(&StyleKey::Underline)
        || styles.contains(&StyleKey::Link) && !styles.contains(&StyleKey::MarkupDimmed);
    let color = if marked {
        theme.composition_underline
    } else if styles.contains(&StyleKey::Link) {
        theme.link_underline
    } else {
        color
    };
    underlined.then_some(UnderlineStyle {
        thickness: theme.composition_underline_thickness,
        color: Some(color),
        wavy: false,
    })
}

fn run_strikethrough(
    styles: &[StyleKey],
    color: Hsla,
    theme: &Theme,
) -> Option<StrikethroughStyle> {
    let struck = styles.contains(&StyleKey::Strikethrough) || styles.contains(&StyleKey::TaskDone);
    struck.then_some(StrikethroughStyle {
        thickness: theme.rule_thickness,
        color: Some(color),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(styles: &[StyleKey]) -> TextRun {
        text_run(3, styles, &LineTone::PLAIN, false, &Theme::default())
    }

    #[test]
    fn strong_and_emphasis_combine() {
        let theme = Theme::default();
        let both = run(&[StyleKey::Strong, StyleKey::Emphasis]);
        assert_eq!(both.font.weight, theme.bold_weight);
        assert_eq!(both.font.style, FontStyle::Italic);
        assert_eq!(both.color, theme.text);
    }

    #[test]
    fn markup_is_dimmed_even_inside_links() {
        let theme = Theme::default();
        let bracket = run(&[StyleKey::Link, StyleKey::MarkupDimmed]);
        assert_eq!(bracket.color, theme.markup_dimmed);
        assert!(bracket.underline.is_none());
        let link = run(&[StyleKey::Link]);
        assert_eq!(link.color, theme.link);
        assert!(link.underline.is_some());
    }

    #[test]
    fn code_uses_the_code_font_size_and_background() {
        let theme = Theme::default();
        let code = run(&[StyleKey::Code]);
        assert_eq!(code.font.family, theme.code_font_family);
        assert_eq!(code.background_color, Some(theme.code_background));
        assert_eq!(
            run_font_size(&[StyleKey::Code], &LineTone::PLAIN, &theme),
            theme.body_font_size * theme.code_scale
        );
    }

    #[test]
    fn decorations_follow_their_styles() {
        let theme = Theme::default();
        assert!(run(&[StyleKey::Strikethrough]).strikethrough.is_some());
        assert!(run(&[StyleKey::Underline]).underline.is_some());
        assert_eq!(
            run(&[StyleKey::Highlight]).background_color,
            Some(theme.highlight)
        );
        let done = run(&[StyleKey::TaskDone]);
        assert_eq!(done.color, theme.text_faint);
        assert!(done.strikethrough.is_some());
        assert_eq!(run(&[StyleKey::Comment]).color, theme.text_faint);
        let marked = text_run(1, &[], &LineTone::PLAIN, true, &theme);
        assert!(marked.underline.is_some());
    }

    #[test]
    fn headings_and_callout_titles_take_the_line_tone() {
        let theme = Theme::default();
        let heading = LineTone {
            heading_level: 2,
            ..LineTone::PLAIN
        };
        let run = text_run(1, &[StyleKey::Heading(2)], &heading, false, &theme);
        assert_eq!(run.font.weight, theme.bold_weight);
        assert_eq!(heading.font_size(&theme), theme.font_size(2));
        let callout = LineTone {
            callout: Some(CalloutKind::Warning),
            ..LineTone::PLAIN
        };
        let title = text_run(1, &[StyleKey::CalloutTitle], &callout, false, &theme);
        assert_eq!(title.color, theme.callout_color(CalloutKind::Warning));
    }
}
