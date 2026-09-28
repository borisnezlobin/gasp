//! Maps the render planner's semantic styles to fonts, colours and
//! decorations from the theme.

use editor_core::render::StyleKey;
use editor_core::syntax::CalloutKind;
use gpui::{Font, FontStyle, Hsla, Pixels, Rgba, StrikethroughStyle, TextRun, UnderlineStyle, px};

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

/// Whether a run is set in the code font: code, math source and
/// `<kbd>` keys.
fn uses_code_font(styles: &[StyleKey]) -> bool {
    is_code(styles) || styles.contains(&StyleKey::Kbd)
}

/// Room inside a run's fill at each end: inline code, keys and property
/// chips have some, so their text doesn't touch the fill's edge.
pub fn fill_padding(styles: &[StyleKey], theme: &Theme) -> Pixels {
    if uses_code_font(styles) {
        theme.inline_code_padding
    } else if styles.contains(&StyleKey::PropertyChip) {
        theme.property_chip_padding
    } else {
        px(0.)
    }
}

/// The font size of a run.
pub fn run_font_size(styles: &[StyleKey], tone: &LineTone, theme: &Theme) -> Pixels {
    let size = surrounding_size(styles, tone, theme);
    let size = if uses_code_font(styles) && !tone.code {
        size * theme.code_scale
    } else {
        size
    };
    if is_script(styles) {
        size * theme.script_scale
    } else {
        size
    }
}

/// How far a run's baseline sits above the line's: up for `<sup>`,
/// down (negative) for `<sub>`.
pub fn run_baseline_shift(styles: &[StyleKey], tone: &LineTone, theme: &Theme) -> Pixels {
    let size = surrounding_size(styles, tone, theme);
    if styles.contains(&StyleKey::Superscript) {
        size * theme.superscript_rise
    } else if styles.contains(&StyleKey::Subscript) {
        -(size * theme.subscript_drop)
    } else {
        px(0.)
    }
}

/// The size of the text a run sits in: the line's, scaled by the
/// innermost `font-size` a note's HTML asks for.
fn surrounding_size(styles: &[StyleKey], tone: &LineTone, theme: &Theme) -> Pixels {
    let scale = innermost(styles, |style| match style {
        StyleKey::FontScale { depth, percent } => Some((*depth, f32::from(*percent) / 100.)),
        _ => None,
    });
    tone.font_size(theme) * scale.unwrap_or(1.)
}

fn is_script(styles: &[StyleKey]) -> bool {
    styles
        .iter()
        .any(|style| matches!(style, StyleKey::Superscript | StyleKey::Subscript))
}

/// The value of the most deeply nested style `pick` finds, so an inner
/// element's colour or size wins over its parent's.
fn innermost<T>(styles: &[StyleKey], pick: impl Fn(&StyleKey) -> Option<(u8, T)>) -> Option<T> {
    styles
        .iter()
        .filter_map(pick)
        .max_by_key(|(depth, _)| *depth)
        .map(|(_, value)| value)
}

/// A colour written in a note, as `0xRRGGBBAA`.
fn note_rgba(rgba: u32) -> Hsla {
    let [r, g, b, a] = rgba.to_be_bytes();
    let unit = |byte: u8| f32::from(byte) / 255.;
    Rgba {
        r: unit(r),
        g: unit(g),
        b: unit(b),
        a: unit(a),
    }
    .into()
}

fn note_color(styles: &[StyleKey]) -> Option<Hsla> {
    innermost(styles, |style| match style {
        StyleKey::TextColor { depth, rgba } => Some((*depth, note_rgba(*rgba))),
        _ => None,
    })
}

fn note_background(styles: &[StyleKey]) -> Option<Hsla> {
    innermost(styles, |style| match style {
        StyleKey::TextBackground { depth, rgba } => Some((*depth, note_rgba(*rgba))),
        _ => None,
    })
}

/// The relative luminance of a colour, as WCAG defines it.
fn luminance(color: Hsla) -> f32 {
    let rgb = color.to_rgb();
    let linear = |channel: f32| {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(rgb.r) + 0.7152 * linear(rgb.g) + 0.0722 * linear(rgb.b)
}

fn contrast(a: Hsla, b: Hsla) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Steps a note's colour takes toward legibility, as lightness.
const LEGIBILITY_STEP: f32 = 0.05;

/// `color`, lightened on a dark backdrop or darkened on a light one until
/// it has `theme.note_color_contrast` against `backdrop`, so a note's
/// navy stays readable in dark mode and its yellow in light mode. A
/// colour that already reads is kept as written.
pub fn legible(color: Hsla, backdrop: Hsla, theme: &Theme) -> Hsla {
    let white = Hsla::white();
    let black = Hsla::black();
    let step = if contrast(backdrop, white) > contrast(backdrop, black) {
        LEGIBILITY_STEP
    } else {
        -LEGIBILITY_STEP
    };
    let mut adjusted = color;
    while contrast(adjusted, backdrop) < theme.note_color_contrast
        && (0. ..=1.).contains(&(adjusted.l + step))
    {
        adjusted.l += step;
    }
    adjusted
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
    let mut font = if tone.code || uses_code_font(styles) {
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
/// Symbols come first; then a colour the note's HTML asks for; then the
/// rest.
const MARKUP_COLOR_STYLES: [StyleKey; 3] =
    [StyleKey::MarkupDimmed, StyleKey::Comment, StyleKey::Html];
const COLOR_STYLES: [StyleKey; 6] = [
    StyleKey::TaskDone,
    StyleKey::Link,
    StyleKey::FootnoteRef,
    StyleKey::Tag,
    StyleKey::FrontmatterKey,
    StyleKey::Kbd,
];

fn keyed_color(keys: &[StyleKey], styles: &[StyleKey], theme: &Theme) -> Option<Hsla> {
    keys.iter()
        .find(|key| styles.contains(key))
        .map(|key| style_color(*key, theme))
}

fn run_color(styles: &[StyleKey], tone: &LineTone, theme: &Theme) -> Hsla {
    if let Some(color) = keyed_color(&MARKUP_COLOR_STYLES, styles, theme) {
        return color;
    }
    let color = note_color(styles)
        .or_else(|| keyed_color(&COLOR_STYLES, styles, theme))
        .unwrap_or_else(|| tone_color(styles, tone, theme));
    let explicit = note_color(styles).is_some() || note_background(styles).is_some();
    match explicit {
        true => legible(
            color,
            note_background(styles).unwrap_or(theme.background),
            theme,
        ),
        false => color,
    }
}

/// The colour of plain text in a line of this tone.
fn tone_color(styles: &[StyleKey], tone: &LineTone, theme: &Theme) -> Hsla {
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
        StyleKey::Kbd => theme.keycap_text,
        _ => theme.text_muted,
    }
}

fn run_background(styles: &[StyleKey], theme: &Theme) -> Option<Hsla> {
    if let Some(color) = note_background(styles) {
        return Some(color);
    }
    if styles.contains(&StyleKey::Highlight) {
        return Some(theme.highlight);
    }
    if styles.contains(&StyleKey::Code) {
        return Some(theme.code_background);
    }
    if styles.contains(&StyleKey::Kbd) {
        return Some(theme.keycap_fill);
    }
    let chip = styles.contains(&StyleKey::Tag) || styles.contains(&StyleKey::PropertyChip);
    chip.then_some(theme.tag_background)
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
    fn a_notes_html_styles_its_runs() {
        let theme = Theme::default();
        let red = run(&[StyleKey::TextColor {
            depth: 0,
            rgba: 0xff0000ff,
        }]);
        assert_eq!(
            red.color,
            gpui::rgb(0xff0000).into(),
            "red reads as written"
        );
        let nested = run(&[
            StyleKey::Strong,
            StyleKey::TextColor {
                depth: 0,
                rgba: 0xff0000ff,
            },
            StyleKey::TextColor {
                depth: 1,
                rgba: 0x0000ffff,
            },
        ]);
        assert_eq!(
            nested.color,
            gpui::rgb(0x0000ff).into(),
            "the inner colour wins"
        );
        assert_eq!(
            nested.font.weight, theme.bold_weight,
            "and composes with bold"
        );
        let dimmed = run(&[
            StyleKey::MarkupDimmed,
            StyleKey::TextColor {
                depth: 0,
                rgba: 0xff0000ff,
            },
        ]);
        assert_eq!(dimmed.color, theme.markup_dimmed, "shown tags stay dimmed");
        let key = run(&[StyleKey::Kbd]);
        assert_eq!(key.color, theme.keycap_text);
        assert_eq!(key.background_color, Some(theme.keycap_fill));
        assert_eq!(key.font.family, theme.code_font_family);
    }

    #[test]
    fn html_sizes_scale_and_scripts_shrink_and_shift() {
        let theme = Theme::default();
        let body = theme.body_font_size;
        let scaled = [StyleKey::FontScale {
            depth: 0,
            percent: 150,
        }];
        assert_eq!(run_font_size(&scaled, &LineTone::PLAIN, &theme), body * 1.5);
        let up = [StyleKey::Superscript];
        assert_eq!(
            run_font_size(&up, &LineTone::PLAIN, &theme),
            body * theme.script_scale
        );
        assert!(run_baseline_shift(&up, &LineTone::PLAIN, &theme) > px(0.));
        let down = [StyleKey::Subscript];
        assert!(run_baseline_shift(&down, &LineTone::PLAIN, &theme) < px(0.));
        assert_eq!(run_baseline_shift(&[], &LineTone::PLAIN, &theme), px(0.));
    }

    #[test]
    fn a_notes_colours_stay_legible_on_the_page() {
        let light = Theme::default();
        let dark = Theme {
            background: gpui::rgb(0x1e1e1e).into(),
            ..Theme::default()
        };
        let navy: Hsla = gpui::rgb(0x000080).into();
        let red: Hsla = gpui::rgb(0xff0000).into();
        assert_eq!(legible(red, light.background, &light), red);
        assert_eq!(legible(red, dark.background, &dark), red);
        assert_eq!(legible(navy, light.background, &light), navy);
        let lifted = legible(navy, dark.background, &dark);
        assert!(lifted.l > navy.l, "navy lightens on a dark page");
        assert!(contrast(lifted, dark.background) >= dark.note_color_contrast);
        let yellow: Hsla = gpui::rgb(0xffff00).into();
        assert!(legible(yellow, light.background, &light).l < yellow.l);
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
