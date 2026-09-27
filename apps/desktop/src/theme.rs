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
    /// Find bar, vault search panel and export dialog.
    pub find_ui: FindUiTheme,
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
            find_ui: FindUiTheme::default(),
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

// Find, search and export ----------------------------------------------

/// Tokens for the find bar, the vault search panel and the export dialog.
#[derive(Clone, Debug)]
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
