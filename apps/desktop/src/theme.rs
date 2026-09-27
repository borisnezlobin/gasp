//! Theme tokens for the editor view. Every visual value the view uses is
//! defined here once.

use gpui::{BoxShadow, Font, FontStyle, FontWeight, Hsla, Pixels, font, hsla, point, px, rgb};

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
    /// The workspace shell around the editor: tabs, panes, sidebar, status bar.
    pub workspace: WorkspaceTheme,
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
            workspace: WorkspaceTheme::default(),
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

// ---- Workspace shell tokens (tabs, panes, sidebar, status bar, modals). ----

/// Sizes and colours for the workspace shell, from `size.*`, `space.*`,
/// `radius.*` and `color.*` in the config theme.
#[derive(Clone, Debug)]
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
