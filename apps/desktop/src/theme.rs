//! The editor's visual tokens, bridged from the config crate's resolved
//! theme (`crates/config/defaults/theme.toml` plus a vault's overrides) and
//! the `appearance.base-font-size` setting. Every visual value the view uses
//! comes from here.
//!
//! The base font size is in points, as in the settings file, and the view
//! works in GPUI's logical pixels, which follow the CSS convention of 96 per
//! inch. A point is 1/72 inch, so 12pt is 16px: the size Obsidian and most
//! browsers use for body text.

use editor_config::Config;
use editor_config::theme::{Theme as Tokens, TokenValue};
use editor_core::syntax::CalloutKind;
use gpui::{Font, FontStyle, FontWeight, Hsla, Pixels, Rgba, SharedString, font, hsla, px};

/// Logical pixels per typographic point.
pub const PIXELS_PER_POINT: f32 = 96. / 72.;

/// Sizes, fonts and colours for the editor view.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub body_font_family: SharedString,
    pub ui_font_family: SharedString,
    pub code_font_family: SharedString,
    pub regular_weight: FontWeight,
    pub medium_weight: FontWeight,
    pub bold_weight: FontWeight,
    pub body_font_size: Pixels,
    /// Font sizes for heading levels 1 to 6.
    pub heading_font_sizes: [Pixels; 6],
    pub small_font_size: Pixels,
    /// Code size as a multiple of the surrounding text size.
    pub code_scale: f32,
    /// Line height as a multiple of the font size.
    pub line_height_factor: f32,
    pub code_line_height_factor: f32,
    pub ui_line_height_factor: f32,
    /// The widest the text column gets with readable line length on.
    pub editor_max_width: Pixels,
    pub text_padding: Pixels,
    pub space_xs: Pixels,
    pub space_sm: Pixels,
    pub space_md: Pixels,
    pub space_lg: Pixels,
    pub space_xl: Pixels,
    pub radius_sm: Pixels,
    pub radius_md: Pixels,
    pub radius_lg: Pixels,
    pub icon_size: Pixels,
    pub image_height: Pixels,
    pub image_gap: Pixels,
    pub image_corner_radius: Pixels,
    pub cursor_width: Pixels,
    /// Extra width that shows a selected line break.
    pub newline_selection_width: Pixels,
    pub composition_underline_thickness: Pixels,
    pub rule_thickness: Pixels,
    pub quote_bar_width: Pixels,
    /// Horizontal room each quote or callout level takes.
    pub quote_indent: Pixels,
    /// Width of the slot a list bullet or checkbox is drawn in.
    pub list_marker_width: Pixels,
    pub bullet_size: Pixels,
    /// Columns a tab takes at the start of a line.
    pub tab_columns: usize,
    pub background: Hsla,
    pub surface: Hsla,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_faint: Hsla,
    pub heading_text: Hsla,
    pub markup_dimmed: Hsla,
    pub code_text: Hsla,
    pub code_background: Hsla,
    pub selection: Hsla,
    pub cursor: Hsla,
    pub accent: Hsla,
    pub composition_underline: Hsla,
    pub link: Hsla,
    pub tag_background: Hsla,
    pub highlight: Hsla,
    pub divider: Hsla,
    pub error: Hsla,
    pub shadow: Hsla,
    pub search_match: Hsla,
    pub active_search_match: Hsla,
    /// Opacity of a callout's tinted surface.
    pub callout_tint: f32,
    pub callout_colors: CalloutColors,
}

/// One colour per callout type.
#[derive(Clone, Debug, PartialEq)]
pub struct CalloutColors(Vec<(CalloutKind, Hsla)>);

/// Obsidian's callout colours, as `color.callout.<type>` tokens can override.
const CALLOUT_DEFAULTS: [(CalloutKind, &str, u32); 14] = [
    (CalloutKind::Note, "note", 0x086ddd),
    (CalloutKind::Abstract, "abstract", 0x00bfbc),
    (CalloutKind::Info, "info", 0x086ddd),
    (CalloutKind::Todo, "todo", 0x086ddd),
    (CalloutKind::Tip, "tip", 0x00bfbc),
    (CalloutKind::Success, "success", 0x08b94e),
    (CalloutKind::Question, "question", 0xec7500),
    (CalloutKind::Warning, "warning", 0xec7500),
    (CalloutKind::Failure, "failure", 0xe93147),
    (CalloutKind::Danger, "danger", 0xe93147),
    (CalloutKind::Bug, "bug", 0xe93147),
    (CalloutKind::Example, "example", 0x7852ee),
    (CalloutKind::Quote, "quote", 0x9e9e9e),
    (CalloutKind::Custom, "custom", 0x086ddd),
];

impl CalloutColors {
    pub fn get(&self, kind: CalloutKind) -> Hsla {
        self.0
            .iter()
            .find(|(candidate, _)| *candidate == kind)
            .map_or(hsla(0., 0., 0.5, 1.), |(_, color)| *color)
    }
}

impl Default for Theme {
    /// The built-in theme at the built-in base font size.
    fn default() -> Self {
        let config = Config::defaults();
        Self::from_config(&config)
    }
}

impl Theme {
    /// The theme a loaded config describes.
    pub fn from_config(config: &Config) -> Self {
        Self::from_tokens(&config.theme, config.settings.appearance.base_font_size)
    }

    /// Builds the theme from resolved tokens and a base size in points.
    /// Tokens that are missing or malformed fall back to the built-in value.
    pub fn from_tokens(tokens: &Tokens, base_font_points: u32) -> Self {
        let read = TokenReader { tokens };
        let base = px(base_font_points.max(1) as f32 * PIXELS_PER_POINT);
        let colors = read_colors(&read);
        let scale = |name: &str, default: f32| base * read.number(name, default);
        let space = |name: &str, default: f32| px(read.number(name, default));
        let heading = |level: usize, default: f32| scale(&format!("font.scale.h{level}"), default);
        Self {
            body_font_family: read.text("font.text", "Charter").into(),
            ui_font_family: read.text("font.ui", "Charter").into(),
            code_font_family: read.text("font.code", "Courier New").into(),
            regular_weight: FontWeight(read.number("font.weight.regular", 400.)),
            medium_weight: FontWeight(read.number("font.weight.medium", 500.)),
            bold_weight: FontWeight(read.number("font.weight.bold", 700.)),
            body_font_size: scale("font.scale.body", 1.),
            heading_font_sizes: [
                heading(1, 1.8),
                heading(2, 1.5),
                heading(3, 1.3),
                heading(4, 1.15),
                heading(5, 1.),
                heading(6, 1.),
            ],
            small_font_size: scale("font.scale.small", 0.875),
            code_scale: read.number("font.scale.code", 0.95),
            line_height_factor: read.number("font.line-height.body", 1.6),
            code_line_height_factor: read.number("font.line-height.code", 1.45),
            ui_line_height_factor: read.number("font.line-height.ui", 1.3),
            editor_max_width: space("size.editor-max-width", 720.),
            text_padding: space("space.xxl", 24.),
            space_xs: space("space.xs", 2.),
            space_sm: space("space.sm", 4.),
            space_md: space("space.md", 8.),
            space_lg: space("space.lg", 12.),
            space_xl: space("space.xl", 16.),
            radius_sm: space("radius.sm", 4.),
            radius_md: space("radius.md", 6.),
            radius_lg: space("radius.lg", 10.),
            icon_size: space("size.icon", 16.),
            image_height: px(72.),
            image_gap: space("space.sm", 4.),
            image_corner_radius: space("radius.sm", 4.),
            cursor_width: px(2.),
            newline_selection_width: space("space.sm", 4.) * 1.5,
            composition_underline_thickness: px(1.),
            rule_thickness: px(1.),
            quote_bar_width: px(3.),
            quote_indent: space("space.xl", 16.),
            list_marker_width: base * 1.25,
            bullet_size: base * 0.3,
            tab_columns: 4,
            callout_tint: 0.1,
            ..colors
        }
    }

    /// A copy with every size multiplied by `zoom`, for view zoom.
    pub fn scaled(&self, zoom: f32) -> Self {
        let mut scaled = self.clone();
        for size in scaled.sizes_mut() {
            *size *= zoom;
        }
        for size in &mut scaled.heading_font_sizes {
            *size *= zoom;
        }
        scaled
    }

    fn sizes_mut(&mut self) -> [&mut Pixels; 24] {
        [
            &mut self.body_font_size,
            &mut self.small_font_size,
            &mut self.editor_max_width,
            &mut self.text_padding,
            &mut self.space_xs,
            &mut self.space_sm,
            &mut self.space_md,
            &mut self.space_lg,
            &mut self.space_xl,
            &mut self.radius_sm,
            &mut self.radius_md,
            &mut self.radius_lg,
            &mut self.icon_size,
            &mut self.image_height,
            &mut self.image_gap,
            &mut self.image_corner_radius,
            &mut self.cursor_width,
            &mut self.newline_selection_width,
            &mut self.composition_underline_thickness,
            &mut self.rule_thickness,
            &mut self.quote_bar_width,
            &mut self.quote_indent,
            &mut self.list_marker_width,
            &mut self.bullet_size,
        ]
    }

    /// Picks the first family of each font that `available` has, falling
    /// back to common platform fonts. An empty list keeps the families.
    pub fn resolve_fonts(&mut self, available: &[String]) {
        if available.is_empty() {
            return;
        }
        let pick = |wanted: &SharedString, fallbacks: &[&str]| -> SharedString {
            std::iter::once(wanted.as_ref())
                .chain(fallbacks.iter().copied())
                .find(|family| available.iter().any(|name| name == family))
                .map_or_else(|| wanted.clone(), |family| family.to_owned().into())
        };
        self.body_font_family = pick(&self.body_font_family, SERIF_FALLBACKS);
        self.ui_font_family = pick(&self.ui_font_family, SERIF_FALLBACKS);
        self.code_font_family = pick(&self.code_font_family, MONO_FALLBACKS);
    }

    /// Font size for a line, by heading level (0 means body text).
    pub fn font_size(&self, heading_level: u8) -> Pixels {
        match heading_level {
            1..=6 => self.heading_font_sizes[usize::from(heading_level) - 1],
            _ => self.body_font_size,
        }
    }

    /// Height of one line of text at `font_size`.
    pub fn line_height(&self, font_size: Pixels) -> Pixels {
        font_size * self.line_height_factor
    }

    pub fn body_line_height(&self) -> Pixels {
        self.line_height(self.body_font_size)
    }

    pub fn body_font(&self) -> Font {
        let mut body = font(self.body_font_family.clone());
        body.weight = self.regular_weight;
        body
    }

    pub fn ui_font(&self) -> Font {
        font(self.ui_font_family.clone())
    }

    pub fn code_font(&self) -> Font {
        font(self.code_font_family.clone())
    }

    pub fn strong_font(&self) -> Font {
        let mut strong = self.body_font();
        strong.weight = self.bold_weight;
        strong
    }

    pub fn emphasis_font(&self) -> Font {
        let mut emphasis = self.body_font();
        emphasis.style = FontStyle::Italic;
        emphasis
    }

    pub fn heading_font(&self) -> Font {
        let mut heading = self.body_font();
        heading.weight = self.bold_weight;
        heading
    }

    pub fn callout_color(&self, kind: CalloutKind) -> Hsla {
        self.callout_colors.get(kind)
    }

    /// The tinted surface behind a callout.
    pub fn callout_surface(&self, kind: CalloutKind) -> Hsla {
        let mut color = self.callout_color(kind);
        color.a *= self.callout_tint;
        color
    }
}

#[cfg(target_os = "macos")]
const SERIF_FALLBACKS: &[&str] = &["Charter", "Iowan Old Style", "Georgia", "Times New Roman"];
#[cfg(target_os = "windows")]
const SERIF_FALLBACKS: &[&str] = &["Georgia", "Cambria", "Times New Roman", "Segoe UI"];
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const SERIF_FALLBACKS: &[&str] = &[
    "Charis SIL",
    "Bitstream Charter",
    "Noto Serif",
    "Liberation Serif",
    "DejaVu Serif",
    "Noto Sans",
    "DejaVu Sans",
];

#[cfg(target_os = "macos")]
const MONO_FALLBACKS: &[&str] = &["Menlo", "Monaco"];
#[cfg(target_os = "windows")]
const MONO_FALLBACKS: &[&str] = &["Consolas", "Cascadia Mono"];
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const MONO_FALLBACKS: &[&str] = &[
    "Liberation Mono",
    "Cousine",
    "DejaVu Sans Mono",
    "Noto Sans Mono",
];

/// Colours read from tokens, with every size left at zero for
/// [`Theme::from_tokens`] to fill in.
fn read_colors(read: &TokenReader<'_>) -> Theme {
    let text = read.color("color.text", 0x27272a);
    let highlight = read.color("color.highlight", 0xfff59d);
    let mut search_match = highlight;
    search_match.a *= 0.55;
    Theme {
        background: read.color("color.background", 0xffffff),
        surface: read.color("color.surface", 0xfafafa),
        text,
        text_muted: read.color("color.text-muted", 0x52525b),
        text_faint: read.color("color.text-faint", 0xa1a1aa),
        heading_text: text,
        markup_dimmed: read.color("color.text-faint", 0xa1a1aa),
        code_text: text,
        code_background: read.color("color.code-background", 0xf4f4f5),
        selection: read.color("color.selection", 0xe4e4e7),
        cursor: read.color("color.accent", 0x000000),
        accent: read.color("color.accent", 0x000000),
        composition_underline: text,
        link: read.color("color.link", 0x000000),
        tag_background: read.color("color.hover", 0xf4f4f5),
        highlight,
        divider: read.color("color.divider", 0xe4e4e7),
        error: read.color("color.conflict", 0xc62828),
        shadow: read.color("color.shadow", 0x0000001f),
        search_match,
        active_search_match: read.color("color.highlight", 0xfff59d),
        callout_colors: CalloutColors(
            CALLOUT_DEFAULTS
                .iter()
                .map(|(kind, name, rgb)| {
                    (*kind, read.color(&format!("color.callout.{name}"), *rgb))
                })
                .collect(),
        ),
        ..zero_sizes()
    }
}

fn zero_sizes() -> Theme {
    let zero = px(0.);
    let black = hsla(0., 0., 0., 1.);
    Theme {
        body_font_family: SharedString::default(),
        ui_font_family: SharedString::default(),
        code_font_family: SharedString::default(),
        regular_weight: FontWeight::NORMAL,
        medium_weight: FontWeight::MEDIUM,
        bold_weight: FontWeight::BOLD,
        body_font_size: zero,
        heading_font_sizes: [zero; 6],
        small_font_size: zero,
        code_scale: 1.,
        line_height_factor: 1.,
        code_line_height_factor: 1.,
        ui_line_height_factor: 1.,
        editor_max_width: zero,
        text_padding: zero,
        space_xs: zero,
        space_sm: zero,
        space_md: zero,
        space_lg: zero,
        space_xl: zero,
        radius_sm: zero,
        radius_md: zero,
        radius_lg: zero,
        icon_size: zero,
        image_height: zero,
        image_gap: zero,
        image_corner_radius: zero,
        cursor_width: zero,
        newline_selection_width: zero,
        composition_underline_thickness: zero,
        rule_thickness: zero,
        quote_bar_width: zero,
        quote_indent: zero,
        list_marker_width: zero,
        bullet_size: zero,
        tab_columns: 4,
        background: black,
        surface: black,
        text: black,
        text_muted: black,
        text_faint: black,
        heading_text: black,
        markup_dimmed: black,
        code_text: black,
        code_background: black,
        selection: black,
        cursor: black,
        accent: black,
        composition_underline: black,
        link: black,
        tag_background: black,
        highlight: black,
        divider: black,
        error: black,
        shadow: black,
        search_match: black,
        active_search_match: black,
        callout_tint: 0.,
        callout_colors: CalloutColors(Vec::new()),
    }
}

struct TokenReader<'a> {
    tokens: &'a Tokens,
}

impl TokenReader<'_> {
    fn text(&self, name: &str, default: &str) -> String {
        self.tokens.text(name).unwrap_or(default).to_owned()
    }

    fn number(&self, name: &str, default: f32) -> f32 {
        self.tokens
            .get(name)
            .and_then(TokenValue::as_f64)
            .map_or(default, |value| value as f32)
    }

    /// A colour token; `default` is `0xrrggbb`, or `0xrrggbbaa` when it
    /// needs more than six hex digits.
    fn color(&self, name: &str, default: u32) -> Hsla {
        self.tokens
            .text(name)
            .and_then(parse_color)
            .unwrap_or_else(|| hex_color(default))
    }
}

fn hex_color(value: u32) -> Hsla {
    let rgba = if value > 0xff_ffff {
        value
    } else {
        (value << 8) | 0xff
    };
    gpui::rgba(rgba).into()
}

/// Parses `#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb(r, g, b)` and
/// `rgba(r, g, b, a)`.
pub fn parse_color(text: &str) -> Option<Hsla> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix('#') {
        return parse_hex(hex);
    }
    let inner = text
        .strip_prefix("rgba(")
        .or_else(|| text.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let parts: Vec<f32> = inner
        .split(',')
        .map(|part| part.trim().parse::<f32>())
        .collect::<Result<_, _>>()
        .ok()?;
    let (channels, alpha) = match parts.as_slice() {
        [r, g, b] => ([*r, *g, *b], 1.),
        [r, g, b, a] => ([*r, *g, *b], *a),
        _ => return None,
    };
    let [r, g, b] = channels.map(|channel| (channel / 255.).clamp(0., 1.));
    Some(
        Rgba {
            r,
            g,
            b,
            a: alpha.clamp(0., 1.),
        }
        .into(),
    )
}

fn parse_hex(hex: &str) -> Option<Hsla> {
    let expanded: String = match hex.len() {
        3 => hex.chars().flat_map(|c| [c, c]).collect(),
        6 | 8 => hex.to_owned(),
        _ => return None,
    };
    let value = u32::from_str_radix(&expanded, 16).ok()?;
    Some(if expanded.len() == 8 {
        gpui::rgba(value).into()
    } else {
        hex_color(value)
    })
}

#[cfg(test)]
mod tests {
    use editor_config::loader::build_theme;

    use super::*;

    fn themed(user: &str, base: u32) -> Theme {
        let (tokens, _) = build_theme("theme.toml", Some(user)).unwrap();
        Theme::from_tokens(&tokens, base)
    }

    #[test]
    fn headings_are_larger_than_body_text() {
        let theme = Theme::default();
        for level in 1..=4 {
            assert!(theme.font_size(level) > theme.body_font_size);
        }
        assert_eq!(theme.font_size(0), theme.body_font_size);
        assert_eq!(theme.font_size(9), theme.body_font_size);
    }

    #[test]
    fn base_size_is_in_points() {
        let theme = Theme::default();
        assert_eq!(theme.body_font_size, px(16.));
        assert_eq!(theme.heading_font_sizes[0], px(16. * 1.8));
        assert_eq!(themed("", 15).body_font_size, px(20.));
    }

    #[test]
    fn fonts_and_colours_come_from_tokens() {
        let theme = themed(
            "[font]\ntext = \"Iowan\"\n[color]\ntext = \"#ff0000\"\n",
            12,
        );
        assert_eq!(theme.body_font_family.as_ref(), "Iowan");
        assert_eq!(theme.code_font_family.as_ref(), "Courier New");
        assert_eq!(theme.text, parse_color("#f00").unwrap());
        assert_eq!(theme.heading_text, theme.text);
        assert_eq!(theme.line_height_factor, 1.6);
    }

    #[test]
    fn callout_colours_can_be_overridden() {
        let theme = themed("[color.callout]\nnote = \"#00ff00\"\n", 12);
        assert_eq!(
            theme.callout_color(CalloutKind::Note),
            parse_color("#00ff00").unwrap()
        );
        assert!(theme.callout_surface(CalloutKind::Bug).a < 0.2);
    }

    #[test]
    fn parses_css_colours() {
        let shadow = parse_color("rgba(0, 0, 0, 0.12)").unwrap();
        assert!((shadow.a - 0.12).abs() < 1e-6);
        assert_eq!(parse_color("#000000ff"), parse_color("#000"));
        assert_eq!(parse_color("nope"), None);
        assert_eq!(parse_color("#12345"), None);
    }

    #[test]
    fn zoom_scales_every_size() {
        let theme = Theme::default();
        let zoomed = theme.scaled(1.5);
        assert_eq!(zoomed.body_font_size, theme.body_font_size * 1.5);
        assert_eq!(zoomed.font_size(1), theme.font_size(1) * 1.5);
        assert_eq!(zoomed.editor_max_width, theme.editor_max_width * 1.5);
        assert_eq!(zoomed.text, theme.text);
    }

    #[test]
    fn missing_fonts_fall_back() {
        let mut theme = Theme::default();
        theme.resolve_fonts(&["DejaVu Serif".into(), "DejaVu Sans Mono".into()]);
        let mut with_italics = Theme::default();
        with_italics.resolve_fonts(&["DejaVu Serif".into(), "Liberation Serif".into()]);
        if cfg!(target_os = "linux") {
            assert_eq!(with_italics.body_font_family.as_ref(), "Liberation Serif");
        }
        if cfg!(target_os = "linux") {
            assert_eq!(theme.body_font_family.as_ref(), "DejaVu Serif");
            assert_eq!(theme.code_font_family.as_ref(), "DejaVu Sans Mono");
        }
        let mut untouched = Theme::default();
        untouched.resolve_fonts(&[]);
        assert_eq!(untouched.body_font_family.as_ref(), "Charter");
    }
}
