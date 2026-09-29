//! The theme tokens the iPhone app draws with, resolved from the built-in
//! theme and the vault's own `.gasp/theme.toml`.

use std::collections::HashMap;

use editor_config::loader::Config;
use editor_config::theme::Theme as Tokens;
use editor_core::syntax::parse_color;

/// The desktop app turns the base size in points into pixels at 96 per
/// inch, and the phone draws a pixel's size as one point.
const POINTS_PER_BASE_POINT: f64 = 96. / 72.;

const CALLOUT_PREFIX: &str = "color.callout.";

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ThemeTokens {
    pub light: Palette,
    pub dark: Palette,
    pub typography: Typography,
    pub spacing: Spacing,
}

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct ThemeColor {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Palette {
    pub background: ThemeColor,
    pub surface: ThemeColor,
    pub card: ThemeColor,
    pub text: ThemeColor,
    pub text_strong: ThemeColor,
    pub text_muted: ThemeColor,
    pub text_detail: ThemeColor,
    pub text_faint: ThemeColor,
    pub accent: ThemeColor,
    pub on_accent: ThemeColor,
    pub icon: ThemeColor,
    pub link: ThemeColor,
    pub link_underline: ThemeColor,
    pub highlight: ThemeColor,
    pub code_background: ThemeColor,
    pub selection: ThemeColor,
    pub divider: ThemeColor,
    pub fill: ThemeColor,
    pub fill_strong: ThemeColor,
    pub ring: ThemeColor,
    pub indent_guide: ThemeColor,
    pub math: ThemeColor,
    /// Tints behind sentences by length, for sentence-length highlighting.
    pub sentence_short: ThemeColor,
    pub sentence_medium: ThemeColor,
    pub sentence_long: ThemeColor,
    pub search_match: ThemeColor,
    pub backdrop: ThemeColor,
    pub popover: ThemeColor,
    pub shadow: ThemeColor,
    /// Each callout kind's colour by its name, such as `warning`.
    pub callouts: HashMap<String, ThemeColor>,
    /// How strongly a callout's colour tints its surface.
    pub callout_opacity: f64,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Typography {
    pub text_font: String,
    pub ui_font: String,
    pub code_font: String,
    /// Body text size in points.
    pub body_size: f64,
    /// Multiples of `body_size`.
    pub small_scale: f64,
    pub code_scale: f64,
    pub title_scale: f64,
    /// `h1` to `h6`.
    pub heading_scales: Vec<f64>,
    /// Line heights as multiples of the font size.
    pub body_line_height: f64,
    pub code_line_height: f64,
    pub heading_line_height: f64,
    /// Room above a heading, in ems of its own size.
    pub heading_space_above: f64,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Spacing {
    pub xs: f64,
    pub sm: f64,
    pub md: f64,
    pub lg: f64,
    pub xl: f64,
    pub xxl: f64,
    pub radius_sm: f64,
    pub radius_md: f64,
    pub radius_lg: f64,
    pub editor_max_width: f64,
}

/// Reads tokens, falling back to the built-in value of any the vault's
/// theme leaves malformed.
struct TokenReader<'a> {
    tokens: &'a Tokens,
    defaults: &'a Tokens,
}

impl TokenReader<'_> {
    fn color(&self, name: &str) -> ThemeColor {
        [self.tokens, self.defaults]
            .iter()
            .find_map(|tokens| tokens.text(name).and_then(parse_color))
            .map_or(ThemeColor::CLEAR, |rgba| ThemeColor {
                red: f64::from(rgba.r) / 255.,
                green: f64::from(rgba.g) / 255.,
                blue: f64::from(rgba.b) / 255.,
                alpha: f64::from(rgba.a) / 255.,
            })
    }

    fn number(&self, name: &str) -> f64 {
        [self.tokens, self.defaults]
            .iter()
            .find_map(|tokens| tokens.get(name).and_then(|value| value.as_f64()))
            .unwrap_or(1.)
    }

    fn callout_colors(&self) -> HashMap<String, ThemeColor> {
        self.defaults
            .names()
            .filter_map(|name| name.strip_prefix(CALLOUT_PREFIX))
            .map(|kind| {
                (
                    kind.to_owned(),
                    self.color(&format!("{CALLOUT_PREFIX}{kind}")),
                )
            })
            .collect()
    }

    fn text(&self, name: &str) -> String {
        [self.tokens, self.defaults]
            .iter()
            .find_map(|tokens| tokens.text(name))
            .unwrap_or_default()
            .to_owned()
    }
}

impl ThemeColor {
    const CLEAR: ThemeColor = ThemeColor {
        red: 0.,
        green: 0.,
        blue: 0.,
        alpha: 0.,
    };
}

/// The theme with no vault's tokens on top.
#[uniffi::export]
pub fn built_in_theme() -> ThemeTokens {
    theme(&Config::defaults())
}

pub(crate) fn theme(config: &Config) -> ThemeTokens {
    let defaults = Config::defaults();
    let reader = |dark: bool| TokenReader {
        tokens: config.theme.for_mode(dark),
        defaults: defaults.theme.for_mode(dark),
    };
    let base_size = f64::from(config.settings.appearance.base_font_size.max(1));
    ThemeTokens {
        light: palette(&reader(false)),
        dark: palette(&reader(true)),
        typography: typography(&reader(false), base_size * POINTS_PER_BASE_POINT),
        spacing: spacing(&reader(false)),
    }
}

/// One colour token in light or dark mode, such as `color.flag-spelling`.
pub(crate) fn token_color(config: &Config, name: &str, dark: bool) -> ThemeColor {
    let defaults = Config::defaults();
    TokenReader {
        tokens: config.theme.for_mode(dark),
        defaults: defaults.theme.for_mode(dark),
    }
    .color(name)
}

fn palette(read: &TokenReader<'_>) -> Palette {
    Palette {
        background: read.color("color.background"),
        surface: read.color("color.surface"),
        card: read.color("color.card"),
        text: read.color("color.text"),
        text_strong: read.color("color.text-strong"),
        text_muted: read.color("color.text-muted"),
        text_detail: read.color("color.text-detail"),
        text_faint: read.color("color.text-faint"),
        accent: read.color("color.accent"),
        on_accent: read.color("color.on-accent"),
        icon: read.color("color.icon"),
        link: read.color("color.link"),
        link_underline: read.color("color.link-underline"),
        highlight: read.color("color.highlight"),
        code_background: read.color("color.code-background"),
        selection: read.color("color.selection"),
        divider: read.color("color.divider"),
        fill: read.color("color.fill"),
        fill_strong: read.color("color.fill-strong"),
        ring: read.color("color.ring"),
        indent_guide: read.color("color.indent-guide"),
        math: read.color("color.math.bracket-1"),
        sentence_short: read.color("color.sentence.short"),
        sentence_medium: read.color("color.sentence.medium"),
        sentence_long: read.color("color.sentence.long"),
        search_match: read.color("color.search-match"),
        backdrop: read.color("color.backdrop"),
        popover: read.color("color.popover"),
        shadow: read.color("color.popover-shadow"),
        callouts: read.callout_colors(),
        callout_opacity: read.number("opacity.callout"),
    }
}

fn typography(read: &TokenReader<'_>, body_size: f64) -> Typography {
    Typography {
        text_font: read.text("font.text"),
        ui_font: read.text("font.ui"),
        code_font: read.text("font.code"),
        body_size: body_size * read.number("font.scale.body"),
        small_scale: read.number("font.scale.small"),
        code_scale: read.number("font.scale.code"),
        title_scale: read.number("font.scale.title"),
        heading_scales: (1..=6)
            .map(|level| read.number(&format!("font.scale.h{level}")))
            .collect(),
        body_line_height: read.number("font.line-height.body"),
        code_line_height: read.number("font.line-height.code"),
        heading_line_height: read.number("font.heading.line-height"),
        heading_space_above: read.number("font.heading.space-above"),
    }
}

fn spacing(read: &TokenReader<'_>) -> Spacing {
    Spacing {
        xs: read.number("space.xs"),
        sm: read.number("space.sm"),
        md: read.number("space.md"),
        lg: read.number("space.lg"),
        xl: read.number("space.xl"),
        xxl: read.number("space.xxl"),
        radius_sm: read.number("radius.sm"),
        radius_md: read.number("radius.md"),
        radius_lg: read.number("radius.lg"),
        editor_max_width: read.number("size.editor-max-width"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_theme_sets_charter_on_white() {
        let theme = theme(&Config::defaults());
        assert_eq!(theme.typography.text_font, "Charter");
        assert_eq!(theme.typography.code_font, "Courier New");
        assert_eq!(theme.light.background.red, 1.);
        assert!(theme.dark.background.red < 0.2);
        assert_eq!(theme.typography.heading_scales.len(), 6);
        assert!((theme.typography.body_size - 16.).abs() < 1e-9);
    }

    #[test]
    fn translucent_tokens_keep_their_alpha() {
        let theme = theme(&Config::defaults());
        assert!((theme.light.link_underline.alpha - 0.3).abs() < 0.01);
    }

    #[test]
    fn every_callout_kind_has_a_colour() {
        let theme = theme(&Config::defaults());
        assert_eq!(theme.light.callouts.len(), 14);
        assert!(theme.dark.callouts["warning"].alpha > 0.);
    }
}
