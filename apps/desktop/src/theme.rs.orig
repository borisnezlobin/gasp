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
use gpui::{
    BoxShadow, Font, FontStyle, FontWeight, Hsla, Pixels, Rgba, SharedString, font, hsla, point,
    px, rgb,
};

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
    /// Find bar, vault search panel and export dialog.
    pub find_ui: FindUiTheme,
    /// The workspace shell around the editor: tabs, panes, sidebar, status bar.
    pub workspace: WorkspaceTheme,
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
        find_ui: FindUiTheme::default(),
        workspace: WorkspaceTheme::default(),
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

/// Fonts the workspace chrome used before it read the theme's UI font.
#[cfg(target_os = "macos")]
const PLATFORM_FONTS: (&str, &str) = (".SystemUIFont", "Menlo");
#[cfg(target_os = "windows")]
const PLATFORM_FONTS: (&str, &str) = ("Segoe UI", "Consolas");
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const PLATFORM_FONTS: (&str, &str) = ("Liberation Sans", "DejaVu Sans Mono");

// ---------------------------------------------------------------------
// Pickers: the command palette, quick switcher and jump to heading.
// ---------------------------------------------------------------------

/// Sizes, fonts and colours for the pickers.
#[derive(Clone, Debug, PartialEq)]
pub struct PickerTheme {
    pub font_family: &'static str,
    pub width: Pixels,
    /// How far below the top of the window the picker sits.
    pub top_offset: Pixels,
    pub visible_rows: usize,
    pub row_height: Pixels,
    pub row_padding_x: Pixels,
    pub row_gap: Pixels,
    /// Space between the lines of the shortcut capture prompt.
    pub capture_gap: Pixels,
    pub row_corner_radius: Pixels,
    pub list_padding: Pixels,
    pub corner_radius: Pixels,
    pub input_padding_x: Pixels,
    pub input_padding_y: Pixels,
    pub input_font_size: Pixels,
    pub input_line_height: Pixels,
    pub row_font_size: Pixels,
    pub detail_font_size: Pixels,
    pub icon_size: Pixels,
    /// Indent per heading level in the outline.
    pub level_indent: Pixels,
    pub keycap_padding_x: Pixels,
    pub keycap_padding_y: Pixels,
    pub keycap_corner_radius: Pixels,
    pub cursor_width: Pixels,
    pub composition_underline_thickness: Pixels,
    pub shadow_blur: Pixels,
    pub shadow_offset_y: Pixels,
    pub background: Hsla,
    pub shadow: Hsla,
    pub text: Hsla,
    pub detail_text: Hsla,
    pub placeholder_text: Hsla,
    pub match_text: Hsla,
    pub match_weight: FontWeight,
    pub icon: Hsla,
    pub selected_row: Hsla,
    pub hovered_row: Hsla,
    pub keycap_background: Hsla,
    pub keycap_text: Hsla,
    pub cursor: Hsla,
    pub selection: Hsla,
    pub warning_text: Hsla,
}

impl Default for PickerTheme {
    fn default() -> Self {
        Self {
            font_family: PLATFORM_FONTS.0,
            width: px(560.),
            top_offset: px(72.),
            visible_rows: 10,
            row_height: px(36.),
            row_padding_x: px(12.),
            row_gap: px(10.),
            capture_gap: px(6.),
            row_corner_radius: px(6.),
            list_padding: px(6.),
            corner_radius: px(10.),
            input_padding_x: px(16.),
            input_padding_y: px(14.),
            input_font_size: px(16.),
            input_line_height: px(24.),
            row_font_size: px(14.),
            detail_font_size: px(12.),
            icon_size: px(16.),
            level_indent: px(16.),
            keycap_padding_x: px(6.),
            keycap_padding_y: px(2.),
            keycap_corner_radius: px(4.),
            cursor_width: px(2.),
            composition_underline_thickness: px(1.),
            shadow_blur: px(32.),
            shadow_offset_y: px(8.),
            background: hsla(0., 0., 1., 1.),
            shadow: hsla(0., 0., 0., 0.18),
            text: hsla(0., 0., 0.13, 1.),
            detail_text: hsla(0., 0., 0.5, 1.),
            placeholder_text: hsla(0., 0., 0.6, 1.),
            match_text: hsla(0., 0., 0., 1.),
            match_weight: FontWeight::BOLD,
            icon: hsla(0., 0., 0.45, 1.),
            selected_row: hsla(0., 0., 0.92, 1.),
            hovered_row: hsla(0., 0., 0.96, 1.),
            keycap_background: hsla(0., 0., 0.94, 1.),
            keycap_text: hsla(0., 0., 0.4, 1.),
            cursor: hsla(0., 0., 0.13, 1.),
            selection: hsla(0.6, 0.9, 0.6, 0.3),
            warning_text: hsla(0.03, 0.7, 0.42, 1.),
        }
    }
}

impl PickerTheme {
    pub fn font(&self) -> Font {
        font(self.font_family)
    }
}

// ---- Workspace shell tokens (tabs, panes, sidebar, status bar, modals). ----

/// Sizes and colours for the workspace shell, from `size.*`, `space.*`,
/// `radius.*` and `color.*` in the config theme.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkspaceTheme {
    pub ui_font_size: Pixels,
    pub ui_small_font_size: Pixels,
    pub title_font_size: Pixels,
    pub tab_height: Pixels,
    pub tab_min_width: Pixels,
    pub tab_max_width: Pixels,
    pub status_height: Pixels,
    pub sidebar_width: Pixels,
    pub sidebar_min_width: Pixels,
    pub sidebar_max_width: Pixels,
    pub hover_edge_width: Pixels,
    pub divider_width: Pixels,
    /// Width of the invisible strip that grabs a divider.
    pub divider_grab_width: Pixels,
    pub focus_line_width: Pixels,
    pub dirty_dot_size: Pixels,
    pub icon_size: Pixels,
    pub small_icon_size: Pixels,
    pub modal_width: Pixels,
    pub modal_top_offset: Pixels,
    pub launcher_width: Pixels,
    pub space_xs: Pixels,
    pub space_sm: Pixels,
    pub space_md: Pixels,
    pub space_lg: Pixels,
    pub space_xl: Pixels,
    pub space_xxl: Pixels,
    pub radius_sm: Pixels,
    pub radius_md: Pixels,
    pub radius_lg: Pixels,
    pub shadow_blur: Pixels,
    pub shadow_offset: Pixels,
    pub chrome_background: Hsla,
    pub sidebar_background: Hsla,
    pub active_tab_background: Hsla,
    /// Hover on the chrome (tabs, buttons in the tab bar).
    pub hover_background: Hsla,
    /// Hover and selection in lists on the note background.
    pub list_hover_background: Hsla,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_faint: Hsla,
    pub divider: Hsla,
    pub accent: Hsla,
    pub on_accent: Hsla,
    pub conflict: Hsla,
    pub shadow: Hsla,
    pub backdrop: Hsla,
    pub selection: Hsla,
    pub cursor: Hsla,
}

impl Default for WorkspaceTheme {
    fn default() -> Self {
        Self {
            ui_font_size: px(13.),
            ui_small_font_size: px(12.),
            title_font_size: px(34.),
            tab_height: px(32.),
            tab_min_width: px(96.),
            tab_max_width: px(200.),
            status_height: px(24.),
            sidebar_width: px(260.),
            sidebar_min_width: px(160.),
            sidebar_max_width: px(520.),
            hover_edge_width: px(8.),
            divider_width: px(1.),
            divider_grab_width: px(6.),
            focus_line_width: px(2.),
            dirty_dot_size: px(8.),
            icon_size: px(16.),
            small_icon_size: px(14.),
            modal_width: px(560.),
            modal_top_offset: px(96.),
            launcher_width: px(420.),
            space_xs: px(2.),
            space_sm: px(4.),
            space_md: px(8.),
            space_lg: px(12.),
            space_xl: px(16.),
            space_xxl: px(24.),
            radius_sm: px(4.),
            radius_md: px(6.),
            radius_lg: px(10.),
            shadow_blur: px(16.),
            shadow_offset: px(4.),
            chrome_background: rgb(0xf4f4f5).into(),
            sidebar_background: rgb(0xf4f4f5).into(),
            active_tab_background: hsla(0., 0., 0.99, 1.),
            hover_background: rgb(0xe4e4e7).into(),
            list_hover_background: rgb(0xf4f4f5).into(),
            text: rgb(0x27272a).into(),
            text_muted: rgb(0x52525b).into(),
            text_faint: rgb(0xa1a1aa).into(),
            divider: rgb(0xe4e4e7).into(),
            accent: rgb(0x000000).into(),
            on_accent: rgb(0xffffff).into(),
            conflict: rgb(0xc62828).into(),
            shadow: hsla(0., 0., 0., 0.12),
            backdrop: hsla(0., 0., 0., 0.08),
            selection: rgb(0xe4e4e7).into(),
            cursor: rgb(0x000000).into(),
        }
    }
}

// ---- File tree and settings screen ----

/// Tokens for the file tree, the settings screen and their shared controls.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelTheme {
    pub font_family: &'static str,
    /// The settings screen's main pane.
    pub pane_background: Hsla,
    pub font_size: Pixels,
    pub small_font_size: Pixels,
    pub title_font_size: Pixels,
    pub strong_weight: FontWeight,
    pub row_height: Pixels,
    pub indent: Pixels,
    pub padding_x: Pixels,
    pub padding_y: Pixels,
    pub gap: Pixels,
    pub icon_size: Pixels,
    pub caret_size: Pixels,
    pub radius: Pixels,
    pub ring_width: Pixels,
    pub ring_blur: Pixels,
    pub background: Hsla,
    pub text: Hsla,
    pub muted_text: Hsla,
    pub icon: Hsla,
    pub hover: Hsla,
    /// A selected row while its list doesn't have focus. Opaque, so the
    /// focus ring (a shadow) only shows around the row.
    pub selected: Hsla,
    /// A selected row while its list has focus. Opaque, like `selected`.
    pub selected_focused: Hsla,
    pub focus_ring: Hsla,
    /// The row of the note that's open.
    pub active_text: Hsla,
    pub active_marker: Hsla,
    pub active_marker_width: Pixels,
    pub drop_target: Hsla,
    /// Opacity of an entry that's been cut and waits to be pasted.
    pub cut_opacity: f32,
    pub error_text: Hsla,
    pub menu_background: Hsla,
    pub menu_shadow: Hsla,
    pub menu_shadow_blur: Pixels,
    pub menu_shadow_offset: Pixels,
    pub menu_width: Pixels,
    pub input_background: Hsla,
    pub input_height: Pixels,
    pub text_selection: Hsla,
    pub caret: Hsla,
    pub caret_width: Pixels,
    pub composition_underline_thickness: Pixels,
    /// Space between a control's track and what sits in it.
    pub control_inset: Pixels,
    pub control_background: Hsla,
    pub control_selected: Hsla,
    pub control_selected_text: Hsla,
    pub toggle_width: Pixels,
    pub toggle_height: Pixels,
    pub toggle_knob: Hsla,
    pub toggle_knob_inset: Pixels,
    pub sidebar_width: Pixels,
    pub content_max_width: Pixels,
    pub section_padding: Pixels,
    pub setting_gap: Pixels,
    pub keycap_background: Hsla,
    pub keycap_padding_y: Pixels,
}

impl Default for PanelTheme {
    fn default() -> Self {
        Self {
            font_family: PLATFORM_FONTS.0,
            pane_background: hsla(0., 0., 1., 1.),
            font_size: px(13.),
            small_font_size: px(12.),
            title_font_size: px(20.),
            strong_weight: FontWeight::SEMIBOLD,
            row_height: px(28.),
            indent: px(14.),
            padding_x: px(8.),
            padding_y: px(4.),
            gap: px(6.),
            icon_size: px(16.),
            caret_size: px(12.),
            radius: px(6.),
            ring_width: px(1.5),
            ring_blur: px(0.5),
            background: hsla(0., 0., 0.97, 1.),
            text: hsla(0., 0., 0.15, 1.),
            muted_text: hsla(0., 0., 0.45, 1.),
            icon: hsla(0., 0., 0.4, 1.),
            hover: hsla(0., 0., 0., 0.04),
            selected: hsla(0., 0., 0.92, 1.),
            selected_focused: hsla(0., 0., 0.88, 1.),
            focus_ring: hsla(0., 0., 0.1, 0.55),
            active_text: hsla(0., 0., 0.05, 1.),
            active_marker: hsla(0., 0., 0.05, 1.),
            active_marker_width: px(2.),
            drop_target: hsla(0., 0., 0., 0.12),
            cut_opacity: 0.5,
            error_text: hsla(0.0, 0.65, 0.42, 1.),
            menu_background: hsla(0., 0., 1., 1.),
            menu_shadow: hsla(0., 0., 0., 0.18),
            menu_shadow_blur: px(16.),
            menu_shadow_offset: px(4.),
            menu_width: px(220.),
            input_background: hsla(0., 0., 1., 1.),
            input_height: px(24.),
            text_selection: hsla(0.6, 0.9, 0.6, 0.3),
            caret: hsla(0., 0., 0.1, 1.),
            caret_width: px(1.5),
            composition_underline_thickness: px(1.),
            control_inset: px(2.),
            control_background: hsla(0., 0., 0., 0.07),
            control_selected: hsla(0., 0., 0.08, 1.),
            control_selected_text: hsla(0., 0., 1., 1.),
            toggle_width: px(34.),
            toggle_height: px(20.),
            toggle_knob: hsla(0., 0., 1., 1.),
            toggle_knob_inset: px(2.),
            sidebar_width: px(200.),
            content_max_width: px(640.),
            section_padding: px(24.),
            setting_gap: px(18.),
            keycap_background: hsla(0., 0., 0., 0.06),
            keycap_padding_y: px(2.),
        }
    }
}

impl PanelTheme {
    /// The ring around whatever has keyboard focus.
    pub fn focus_ring(&self) -> BoxShadow {
        BoxShadow {
            color: self.focus_ring,
            offset: point(px(0.), px(0.)),
            blur_radius: self.ring_blur,
            spread_radius: self.ring_width,
        }
    }

    /// The shadow under menus and popovers.
    pub fn menu_shadow(&self) -> BoxShadow {
        BoxShadow {
            color: self.menu_shadow,
            offset: point(px(0.), self.menu_shadow_offset),
            blur_radius: self.menu_shadow_blur,
            spread_radius: px(0.),
        }
    }
}

// Find, search and export ----------------------------------------------

/// Tokens for the find bar, the vault search panel and the export dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct FindUiTheme {
    pub font_family: &'static str,
    pub font_size: Pixels,
    pub small_font_size: Pixels,
    pub title_font_size: Pixels,
    pub text: Hsla,
    pub muted_text: Hsla,
    pub disabled_text: Hsla,
    pub error_text: Hsla,
    pub panel_background: Hsla,
    pub panel_shadow: Hsla,
    pub panel_shadow_blur: Pixels,
    pub panel_padding: Pixels,
    pub gap: Pixels,
    pub radius: Pixels,
    pub input_height: Pixels,
    pub input_padding_x: Pixels,
    pub input_background: Hsla,
    pub input_focus_ring: Hsla,
    pub input_ring_width: Pixels,
    pub input_error_background: Hsla,
    pub input_selection: Hsla,
    pub placeholder: Hsla,
    pub caret: Hsla,
    pub caret_width: Pixels,
    pub button_size: Pixels,
    pub button_padding_x: Pixels,
    pub icon_size: Pixels,
    pub icon: Hsla,
    pub button_hover_background: Hsla,
    /// A toggle that is on, and a primary button: the accent.
    pub accent_background: Hsla,
    pub accent_text: Hsla,
    pub row_padding_y: Pixels,
    pub row_selected_background: Hsla,
    pub result_indent: Pixels,
    pub match_background: Hsla,
    pub search_panel_width: Pixels,
    pub dialog_width: Pixels,
    pub dialog_top_offset: Pixels,
    pub backdrop: Hsla,
}

impl Default for FindUiTheme {
    fn default() -> Self {
        Self {
            font_family: PLATFORM_FONTS.0,
            font_size: px(14.),
            small_font_size: px(12.),
            title_font_size: px(16.),
            text: hsla(0., 0., 0.13, 1.),
            muted_text: hsla(0., 0., 0.45, 1.),
            disabled_text: hsla(0., 0., 0.7, 1.),
            error_text: hsla(0.0, 0.65, 0.42, 1.),
            panel_background: hsla(0., 0., 0.97, 1.),
            panel_shadow: hsla(0., 0., 0., 0.12),
            panel_shadow_blur: px(12.),
            panel_padding: px(8.),
            gap: px(6.),
            radius: px(6.),
            input_height: px(28.),
            input_padding_x: px(8.),
            input_background: hsla(0., 0., 1., 1.),
            input_focus_ring: hsla(0., 0., 0.1, 1.),
            input_ring_width: px(1.5),
            input_error_background: hsla(0.0, 0.8, 0.95, 1.),
            input_selection: hsla(0.6, 0.9, 0.6, 0.3),
            placeholder: hsla(0., 0., 0.6, 1.),
            caret: hsla(0., 0., 0.1, 1.),
            caret_width: px(1.5),
            button_size: px(28.),
            button_padding_x: px(10.),
            icon_size: px(16.),
            icon: hsla(0., 0., 0.3, 1.),
            button_hover_background: hsla(0., 0., 0.9, 1.),
            accent_background: hsla(0., 0., 0.07, 1.),
            accent_text: hsla(0., 0., 1., 1.),
            row_padding_y: px(3.),
            row_selected_background: hsla(0., 0., 0.9, 1.),
            result_indent: px(12.),
            match_background: hsla(0.14, 0.95, 0.6, 0.45),
            search_panel_width: px(360.),
            dialog_width: px(320.),
            dialog_top_offset: px(96.),
            backdrop: hsla(0., 0., 0., 0.18),
        }
    }
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
