//! Theme tokens for the editor view. Every visual value the view uses is
//! defined here once.

use gpui::{Font, FontStyle, FontWeight, Hsla, Pixels, font, hsla, px};

/// Sizes, fonts and colours for the editor view.
#[derive(Clone, Debug)]
pub struct Theme {
    pub body_font_family: &'static str,
    pub code_font_family: &'static str,
    pub body_font_size: Pixels,
    /// Font sizes for heading levels 1 to 6.
    pub heading_font_sizes: [Pixels; 6],
    /// Line height as a multiple of the font size.
    pub line_height_factor: f32,
    pub image_height: Pixels,
    pub image_gap: Pixels,
    pub image_corner_radius: Pixels,
    pub text_padding: Pixels,
    pub cursor_width: Pixels,
    /// Extra width that shows a selected line break.
    pub newline_selection_width: Pixels,
    pub background: Hsla,
    pub text: Hsla,
    pub heading_text: Hsla,
    pub markup_dimmed: Hsla,
    pub code_text: Hsla,
    pub code_background: Hsla,
    pub selection: Hsla,
    pub cursor: Hsla,
    pub composition_underline: Hsla,
    pub composition_underline_thickness: Pixels,
}

#[cfg(target_os = "macos")]
const PLATFORM_FONTS: (&str, &str) = (".SystemUIFont", "Menlo");
#[cfg(target_os = "windows")]
const PLATFORM_FONTS: (&str, &str) = ("Segoe UI", "Consolas");
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const PLATFORM_FONTS: (&str, &str) = ("Liberation Sans", "DejaVu Sans Mono");

impl Default for Theme {
    fn default() -> Self {
        Self {
            body_font_family: PLATFORM_FONTS.0,
            code_font_family: PLATFORM_FONTS.1,
            body_font_size: px(16.),
            heading_font_sizes: [px(30.), px(25.), px(21.), px(18.), px(17.), px(16.)],
            line_height_factor: 1.5,
            image_height: px(72.),
            image_gap: px(4.),
            image_corner_radius: px(4.),
            text_padding: px(24.),
            cursor_width: px(2.),
            newline_selection_width: px(6.),
            background: hsla(0., 0., 0.99, 1.),
            text: hsla(0., 0., 0.13, 1.),
            heading_text: hsla(0., 0., 0.07, 1.),
            markup_dimmed: hsla(0., 0., 0.6, 1.),
            code_text: hsla(0.97, 0.55, 0.42, 1.),
            code_background: hsla(0., 0., 0.94, 1.),
            selection: hsla(0.6, 0.9, 0.6, 0.3),
            cursor: hsla(0.6, 0.9, 0.45, 1.),
            composition_underline: hsla(0., 0., 0.13, 1.),
            composition_underline_thickness: px(1.),
        }
    }
}

impl Theme {
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

    pub fn body_font(&self) -> Font {
        font(self.body_font_family)
    }

    pub fn code_font(&self) -> Font {
        font(self.code_font_family)
    }

    pub fn strong_font(&self) -> Font {
        let mut strong = self.body_font();
        strong.weight = FontWeight::BOLD;
        strong
    }

    pub fn emphasis_font(&self) -> Font {
        let mut emphasis = self.body_font();
        emphasis.style = FontStyle::Italic;
        emphasis
    }

    pub fn heading_font(&self) -> Font {
        let mut heading = self.body_font();
        heading.weight = FontWeight::BOLD;
        heading
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_are_larger_than_body_text() {
        let theme = Theme::default();
        for level in 1..=5 {
            assert!(theme.font_size(level) > theme.body_font_size);
        }
        assert_eq!(theme.font_size(0), theme.body_font_size);
        assert_eq!(theme.font_size(9), theme.body_font_size);
    }

    #[test]
    fn line_height_scales_with_font_size() {
        let theme = Theme::default();
        assert!(theme.line_height(px(30.)) > theme.line_height(px(16.)));
    }
}
