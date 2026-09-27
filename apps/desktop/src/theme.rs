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

// ---------------------------------------------------------------------
// Pickers: the command palette, quick switcher and jump to heading.
// ---------------------------------------------------------------------

/// Sizes, fonts and colours for the pickers.
#[derive(Clone, Debug)]
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
