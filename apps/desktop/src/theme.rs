//! Theme tokens for the editor view. Every visual value the view uses is
//! defined here once.

use gpui::{BoxShadow, Font, FontStyle, FontWeight, Hsla, Pixels, font, hsla, point, px};

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

// ---- File tree and settings screen ----

/// Tokens for the file tree, the settings screen and their shared controls.
#[derive(Clone, Debug)]
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
