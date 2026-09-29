//! The editor's visual tokens, bridged from the config crate's resolved
//! theme (`crates/config/defaults/theme.toml` plus a vault's overrides) and
//! the `appearance.base-font-size` setting. Every visual value the view uses
//! comes from here.
//!
//! The base font size is in points, as in the settings file, and the view
//! works in GPUI's logical pixels, which follow the CSS convention of 96 per
//! inch. A point is 1/72 inch, so 12pt is 16px: the size Obsidian and most
//! browsers use for body text.

use gasp_config::Config;
use gasp_config::theme::{Theme as Tokens, TokenValue};
use gasp_core::syntax::{CalloutKind, ConflictSide};

use crate::preview::code_highlight::CodeKind;
use gpui::{
    BoxShadow, Font, FontStyle, FontWeight, Hsla, Pixels, Point, Rgba, SharedString, font, hsla,
    point, px,
};

/// Logical pixels per typographic point.
pub const PIXELS_PER_POINT: f32 = 96. / 72.;

/// How much of the focus colour the focus ring shows.
const FOCUS_RING_ALPHA: f32 = 0.3;

/// The blur a hairline ring needs to be drawn at all. The Metal renderer
/// draws an unblurred shadow as a crisp edge; the Blade renderer used on
/// Linux and Windows draws nothing for it, so there a half-pixel blur
/// stands in.
pub const RING_BLUR: f32 = if cfg!(target_os = "macos") { 0. } else { 0.5 };

/// How thick a focus ring is.
pub const FOCUS_RING_WIDTH: f32 = 2.;

/// The ring around whatever has keyboard focus: a crisp two-pixel band of
/// `color`. Every focus ring in the app is drawn with this. It's a
/// shadow, so it shows through a translucent fill: whatever wears it
/// needs an opaque one (see [`over`]).
pub fn focus_ring(color: Hsla) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.), px(0.)),
        blur_radius: px(RING_BLUR),
        spread_radius: px(FOCUS_RING_WIDTH),
    }
}

/// `top` drawn over `bottom`, as one opaque colour. A focus ring is a
/// shadow, which shows through a translucent fill, so whatever wears one
/// needs an opaque fill: this gives the fill that looks the same.
pub fn over(top: Hsla, bottom: Hsla) -> Hsla {
    let top_rgb = top.to_rgb();
    let bottom_rgb = bottom.to_rgb();
    let mix = |a: f32, b: f32| a * top.a + b * (1. - top.a);
    Rgba {
        r: mix(top_rgb.r, bottom_rgb.r),
        g: mix(top_rgb.g, bottom_rgb.g),
        b: mix(top_rgb.b, bottom_rgb.b),
        a: 1.,
    }
    .into()
}

/// Declares [`Palette`]: one field per colour token.
macro_rules! palette {
    ($($(#[$doc:meta])* $field:ident = $token:literal,)*) => {
        /// Every colour the app draws with, read once from the resolved
        /// tokens of the current mode (`color.*`, with `dark.color.*` in
        /// dark mode). The theme structs below take their colours from
        /// here, so one set of tokens drives the whole app.
        #[derive(Clone, Debug, PartialEq)]
        pub struct Palette {
            $($(#[$doc])* pub $field: Hsla,)*
            /// Syntax colours in code blocks, one per [`CodeKind`].
            pub code: [Hsla; 7],
            pub callouts: CalloutColors,
            /// Opacity of a callout's tinted surface.
            pub callout_tint: f32,
            /// Opacity of a file tree entry that's been cut.
            pub cut_opacity: f32,
        }

        impl Palette {
            /// Reads each colour from `tokens`, taking `fallback`'s for
            /// any that are missing or malformed.
            fn read(tokens: &Tokens, fallback: Option<&Palette>) -> Palette {
                let read = TokenReader { tokens };
                Palette {
                    $($field: read.color($token, fallback.map(|f| f.$field)),)*
                    code: CodeKind::ALL.map(|kind| {
                        read.color(kind.token(), fallback.map(|f| f.code[kind as usize]))
                    }),
                    callouts: read_callouts(&read, fallback),
                    callout_tint: read.number(
                        "opacity.callout",
                        fallback.map_or(0.1, |f| f.callout_tint),
                    ),
                    cut_opacity: read.number("opacity.cut", fallback.map_or(0.5, |f| f.cut_opacity)),
                }
            }
        }
    };
}

palette! {
    background = "color.background",
    surface = "color.surface",
    sidebar = "color.sidebar",
    /// The window around the note surfaces.
    app_background = "color.app-background",
    /// Menus, pickers and dialogs.
    popover = "color.popover",
    tooltip = "color.tooltip",
    tooltip_text = "color.tooltip-text",
    tooltip_hint = "color.tooltip-hint",
    text = "color.text",
    text_muted = "color.text-muted",
    text_faint = "color.text-faint",
    text_detail = "color.text-detail",
    text_strong = "color.text-strong",
    icon = "color.icon",
    icon_strong = "color.icon-strong",
    icon_disabled = "color.icon-disabled",
    accent = "color.accent",
    on_accent = "color.on-accent",
    /// Full strength: [`Palette::focus`] gives the ring's colour.
    focus_ring = "color.focus-ring",
    link = "color.link",
    link_underline = "color.link-underline",
    selection = "color.selection",
    hover = "color.hover",
    divider = "color.divider",
    shadow = "color.shadow",
    syncing = "color.syncing",
    conflict = "color.conflict",
    highlight = "color.highlight",
    code_background = "color.code-background",
    code_highlight = "color.code-highlight",
    fill_faint = "color.fill-faint",
    fill = "color.fill",
    fill_strong = "color.fill-strong",
    fill_pressed = "color.fill-pressed",
    field = "color.field",
    field_error = "color.field-error",
    ring = "color.ring",
    popover_ring = "color.popover-ring",
    popover_shadow = "color.popover-shadow",
    tab_shadow = "color.tab-shadow",
    backdrop = "color.backdrop",
    indent_guide = "color.indent-guide",
    drop_target = "color.drop-target",
    divider_active = "color.divider-active",
    drop_zone = "color.drop-zone",
    drop_zone_ring = "color.drop-zone-ring",
    drop_indicator = "color.drop-indicator",
    search_match = "color.search-match",
    active_search_match = "color.active-search-match",
    /// Sentence-length tints, behind short, medium and long sentences.
    sentence_short = "color.sentence.short",
    sentence_medium = "color.sentence.medium",
    sentence_long = "color.sentence.long",
    /// The wavy underline under a misspelled word.
    flag_spelling = "color.flag-spelling",
    /// The wavy underline under a spacing or repeated-word problem.
    flag_mechanical = "color.flag-mechanical",
    /// Lines a restore brings back, and lines it removes.
    diff_added = "color.diff-added",
    diff_removed = "color.diff-removed",
    knob = "color.knob",
    card = "color.card",
    /// A tab stop a snippet left, still to be visited.
    tab_stop = "color.tab-stop",
    math_bracket_1 = "color.math.bracket-1",
    math_bracket_2 = "color.math.bracket-2",
    math_bracket_3 = "color.math.bracket-3",
    /// Tables drawn as grids: see [`TableTheme`].
    table_rule = "color.table.rule",
    table_header = "color.table.header",
    table_header_rule = "color.table.header-rule",
    table_selection = "color.table.selection",
    table_active = "color.table.active",
    /// This device's version in a note's sync conflict.
    this_device = "color.this-device",
    /// The other device's version in a note's sync conflict.
    other_device = "color.other-device",
}

/// Each callout type's colour from its `color.callout.<name>` token.
fn read_callouts(read: &TokenReader<'_>, fallback: Option<&Palette>) -> CalloutColors {
    CalloutColors(
        CALLOUT_NAMES
            .iter()
            .map(|(kind, name)| {
                let old = fallback.map(|f| f.callouts.get(*kind));
                (*kind, read.color(&format!("color.callout.{name}"), old))
            })
            .collect(),
    )
}

impl Palette {
    /// The colours `tokens` describe. Tokens that are missing or
    /// malformed keep the built-in light value.
    pub fn from_tokens(tokens: &Tokens) -> Palette {
        Palette::read(tokens, Some(Palette::builtin()))
    }

    /// The built-in light palette, read once.
    pub fn builtin() -> &'static Palette {
        static BUILTIN: std::sync::OnceLock<Palette> = std::sync::OnceLock::new();
        BUILTIN.get_or_init(|| Palette::read(&Config::defaults().theme, None))
    }

    /// The colour of the focus ring: see [`focus_ring`].
    pub fn focus(&self) -> Hsla {
        Hsla {
            a: self.focus_ring.a * FOCUS_RING_ALPHA,
            ..self.focus_ring
        }
    }
}

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
    /// The note's inline title above its text.
    pub title_font_size: Pixels,
    /// Font sizes for heading levels 1 to 6.
    pub heading_font_sizes: [Pixels; 6],
    pub small_font_size: Pixels,
    /// Code size as a multiple of the surrounding text size.
    pub code_scale: f32,
    /// Superscript and subscript size, as a multiple of the text around
    /// them: footnote numbers, `<sup>` and `<sub>`.
    pub script_scale: f32,
    /// How far a superscript's baseline rises and a subscript's drops, in
    /// ems of the text around it.
    pub superscript_rise: f32,
    pub subscript_drop: f32,
    /// The least contrast ratio a colour written in a note keeps against
    /// what is behind it; one with less is lightened or darkened.
    pub note_color_contrast: f32,
    /// Line height as a multiple of the font size.
    pub line_height_factor: f32,
    pub code_line_height_factor: f32,
    pub heading_line_height_factor: f32,
    /// Room above a heading, in ems of its size.
    pub heading_space_above: f32,
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
    /// How wide the mark on an empty tab stop is.
    pub tab_stop_width: Pixels,
    /// Room on each side of an empty tab stop's block.
    pub tab_stop_gap: Pixels,
    /// Room at each end of inline code, inside its fill.
    pub inline_code_padding: Pixels,
    /// Room at each end of a list property's item, inside its fill.
    pub property_chip_padding: Pixels,
    /// Space between a list property's items.
    pub property_chip_gap: Pixels,
    pub composition_underline_thickness: Pixels,
    /// The line under a footnote problem.
    pub problem_underline_thickness: Pixels,
    /// How far a sentence tint stops short of its row's top and bottom,
    /// leaving a gap between the lines of a paragraph.
    pub sentence_tint_inset: Pixels,
    /// The grammar checker's wavy underline: its stroke, and how far its
    /// top sits above the bottom of the text.
    pub flag_underline_thickness: Pixels,
    pub flag_underline_rise: Pixels,
    /// A web page drawn as a card.
    pub link_card_height: Pixels,
    pub link_card_max_width: Pixels,
    pub link_card_ring: Pixels,
    pub link_card_icon_size: Pixels,
    /// Width over height of the card's preview image.
    pub link_card_image_aspect: f32,
    pub rule_thickness: Pixels,
    pub quote_bar_width: Pixels,
    /// Horizontal room each quote or callout level takes.
    pub quote_indent: Pixels,
    /// Width of the slot a list bullet or checkbox is drawn in.
    pub list_marker_width: Pixels,
    pub bullet_size: Pixels,
    /// The column a note's property values start in, from the
    /// frontmatter's inset.
    pub property_key_width: Pixels,
    /// A task checkbox's side at body size.
    pub checkbox_size: Pixels,
    pub checkbox_border_width: Pixels,
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
    /// The band behind a code line the fence picks out with `{1,3-5}`.
    pub code_highlight: Hsla,
    /// Syntax colours in code blocks, one per [`CodeKind`].
    pub code_syntax: [Hsla; 7],
    pub selection: Hsla,
    pub cursor: Hsla,
    pub accent: Hsla,
    pub composition_underline: Hsla,
    pub link: Hsla,
    /// A link's underline: quieter than its text.
    pub link_underline: Hsla,
    pub tag_background: Hsla,
    pub highlight: Hsla,
    /// `<kbd>` text in a note: a key cap's fill and glyphs, as shortcuts
    /// are drawn everywhere else.
    pub keycap_fill: Hsla,
    pub keycap_text: Hsla,
    pub divider: Hsla,
    pub error: Hsla,
    pub shadow: Hsla,
    /// What floats over the note, such as the live math preview: raised
    /// off the page so it doesn't read as part of the text it covers.
    pub popover: Hsla,
    /// The hairline around a popover.
    pub popover_ring: Hsla,
    pub search_match: Hsla,
    pub active_search_match: Hsla,
    /// The mark on a tab stop still to be visited.
    pub tab_stop: Hsla,
    /// Brackets in math source, by nesting depth.
    pub math_brackets: [Hsla; 3],
    /// Tints behind short, medium and long sentences, in that order.
    pub sentence: [Hsla; 3],
    /// Wavy underlines under a misspelling and a mechanical problem.
    pub flag_spelling: Hsla,
    pub flag_mechanical: Hsla,
    /// Opacity of a callout's tinted surface.
    pub callout_tint: f32,
    pub callout_colors: CalloutColors,
    /// The labels of a sync conflict's versions, this device's first.
    pub conflict_sides: [Hsla; 2],
    /// Find bar, vault search panel and export dialog.
    pub find_ui: FindUiTheme,
    /// The workspace shell around the editor: tabs, panes, sidebar, status bar.
    pub workspace: WorkspaceTheme,
    /// Tables drawn as grids, and the table editor's marks.
    pub table: TableTheme,
    /// Heading folds: the chevron in the margin and the count of lines a
    /// folded heading hides.
    pub fold: FoldTheme,
}

/// The colours of heading folds. Their sizes are the icon size and the
/// spacing scale, so they follow zoom like the text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FoldTheme {
    /// The chevron beside a heading under the pointer, and beside a
    /// folded one.
    pub chevron: Hsla,
    pub chevron_folded: Hsla,
    /// The chevron under the pointer: its icon and the fill behind it.
    pub chevron_hover: Hsla,
    pub chevron_fill: Hsla,
    /// The count of hidden lines after a folded heading.
    pub count_text: Hsla,
    pub count_fill: Hsla,
    pub count_fill_hover: Hsla,
}

impl FoldTheme {
    fn from_palette(p: &Palette) -> Self {
        Self {
            chevron: p.text_faint,
            chevron_folded: p.text_muted,
            chevron_hover: p.text,
            chevron_fill: p.fill_strong,
            count_text: p.text_muted,
            count_fill: p.fill,
            count_fill_hover: p.fill_pressed,
        }
    }
}

/// How a table drawn as a grid looks: its cells, header and rules, and
/// the table editor's marks — the cell being edited, the drag handles and
/// what a drag shows. Built from the `table.*`, `color.table.*` and
/// `opacity.table-*` tokens.
#[derive(Clone, Debug, PartialEq)]
pub struct TableTheme {
    /// Room between a cell's edges and its text.
    pub cell_padding_x: Pixels,
    pub cell_padding_y: Pixels,
    /// The least a column is wide and a row is tall, padding included,
    /// so an empty cell is still a place to click and type.
    pub min_column_width: Pixels,
    pub min_row_height: Pixels,
    /// The fill behind the header row.
    pub header_fill: Hsla,
    /// The rules around and between cells.
    pub rule: Hsla,
    pub rule_width: Pixels,
    /// The heavier rule under the header row.
    pub header_rule: Hsla,
    pub header_rule_width: Pixels,
    /// The cell the caret is in: a tint and a ring inside its square
    /// edges, both in the accent.
    pub active_fill: Hsla,
    pub active_ring: Hsla,
    pub active_ring_width: Pixels,
    /// Selected cells, and a row or column picked by its handle.
    pub selection: Hsla,
    /// The handle beside a row or above a column: its side, its gap from
    /// the table, and its corner.
    pub handle_size: Pixels,
    pub handle_gap: Pixels,
    pub handle_radius: Pixels,
    /// The handle's dots, at rest and under the pointer.
    pub handle_icon: Hsla,
    pub handle_icon_hover: Hsla,
    /// The fill behind a handle under the pointer or being dragged.
    pub handle_fill: Hsla,
    /// How long a handle takes to fade in once its row or column is
    /// under the pointer.
    pub handle_fade: std::time::Duration,
    /// The line between rows or columns where a drag would drop.
    pub drop_indicator: Hsla,
    pub drop_indicator_width: Pixels,
    /// A row or column being dragged: lifted off the page on a raised
    /// fill with a shadow, a little faded.
    pub drag_fill: Hsla,
    pub drag_shadow: Hsla,
    /// The card's edge, a hairline drawn as a shadow, which the shadow
    /// alone doesn't show on a dark page.
    pub drag_ring: Hsla,
    pub drag_shadow_blur: Pixels,
    pub drag_opacity: f32,
    /// Laid over the row or column's own place while it's dragged, so it
    /// reads as the gap it leaves.
    pub drag_source_veil: Hsla,
    /// How far the pointer moves on a handle before a press becomes a
    /// drag.
    pub drag_threshold: Pixels,
    /// How near the note's top or bottom a dragged row scrolls it.
    pub autoscroll_band: Pixels,
}

impl TableTheme {
    /// The built-in look, before the tokens' sizes are read.
    fn from_palette(p: &Palette) -> Self {
        Self {
            cell_padding_x: px(12.),
            cell_padding_y: px(4.),
            min_column_width: px(128.),
            min_row_height: px(38.4),
            header_fill: p.table_header,
            rule: p.table_rule,
            rule_width: px(1.),
            header_rule: p.table_header_rule,
            header_rule_width: px(1.5),
            active_fill: with_alpha(p.table_active, 0.06),
            active_ring: with_alpha(p.table_active, 0.5),
            active_ring_width: px(2.),
            selection: p.table_selection,
            handle_size: px(16.),
            handle_gap: px(4.),
            handle_radius: px(4.),
            handle_icon: p.text_faint,
            handle_icon_hover: p.text_muted,
            handle_fill: p.fill,
            handle_fade: std::time::Duration::from_millis(120),
            drop_indicator: p.accent,
            drop_indicator_width: px(2.),
            drag_fill: p.background,
            drag_shadow: p.shadow,
            drag_ring: p.popover_ring,
            drag_shadow_blur: px(12.),
            drag_opacity: 0.85,
            drag_source_veil: with_alpha(p.background, 0.6),
            drag_threshold: px(4.),
            autoscroll_band: px(32.),
        }
    }

    /// The look `tokens` describe, with minimums measured in lines of
    /// `line_height`. Missing tokens keep the built-in values.
    fn from_tokens(read: &TokenReader<'_>, p: &Palette, line_height: Pixels) -> Self {
        let built_in = Self::from_palette(p);
        let length = |name: &str, fallback: Pixels| px(read.number(name, f32::from(fallback)));
        let lines = |name: &str, fallback: f32| line_height * read.number(name, fallback);
        let tint =
            |name: &str, fallback: f32| with_alpha(p.table_active, read.number(name, fallback));
        Self {
            cell_padding_x: length("table.cell-padding-x", built_in.cell_padding_x),
            cell_padding_y: length("table.cell-padding-y", built_in.cell_padding_y),
            min_column_width: lines("table.min-column-width", 5.),
            min_row_height: lines("table.min-row-height", 1.5),
            rule_width: length("table.rule-width", built_in.rule_width),
            header_rule_width: length("table.header-rule-width", built_in.header_rule_width),
            active_fill: tint("opacity.table-active-fill", 0.06),
            active_ring: tint("opacity.table-active-ring", 0.5),
            active_ring_width: length("table.active-ring-width", built_in.active_ring_width),
            handle_size: length("table.handle-size", built_in.handle_size),
            handle_gap: length("table.handle-gap", built_in.handle_gap),
            handle_radius: length("table.handle-radius", built_in.handle_radius),
            drop_indicator_width: length("table.drop-line-width", built_in.drop_indicator_width),
            drag_shadow_blur: length("table.drag-shadow-blur", built_in.drag_shadow_blur),
            drag_opacity: read.number("opacity.table-drag", built_in.drag_opacity),
            ..built_in
        }
    }

    fn scaled(&self, zoom: f32) -> Self {
        Self {
            cell_padding_x: self.cell_padding_x * zoom,
            cell_padding_y: self.cell_padding_y * zoom,
            min_column_width: self.min_column_width * zoom,
            min_row_height: self.min_row_height * zoom,
            rule_width: self.rule_width * zoom,
            header_rule_width: self.header_rule_width * zoom,
            active_ring_width: self.active_ring_width * zoom,
            handle_size: self.handle_size * zoom,
            handle_gap: self.handle_gap * zoom,
            handle_radius: self.handle_radius * zoom,
            drop_indicator_width: self.drop_indicator_width * zoom,
            drag_shadow_blur: self.drag_shadow_blur * zoom,
            autoscroll_band: self.autoscroll_band * zoom,
            ..self.clone()
        }
    }
}

impl Default for TableTheme {
    fn default() -> Self {
        Self::from_palette(Palette::builtin())
    }
}

/// `color` with its opacity multiplied by `alpha`.
fn with_alpha(color: Hsla, alpha: f32) -> Hsla {
    Hsla {
        a: color.a * alpha,
        ..color
    }
}

/// One colour per callout type.
#[derive(Clone, Debug, PartialEq)]
pub struct CalloutColors(Vec<(CalloutKind, Hsla)>);

/// Each callout type's `color.callout.<name>` token.
const CALLOUT_NAMES: [(CalloutKind, &str); 14] = [
    (CalloutKind::Note, "note"),
    (CalloutKind::Abstract, "abstract"),
    (CalloutKind::Info, "info"),
    (CalloutKind::Todo, "todo"),
    (CalloutKind::Tip, "tip"),
    (CalloutKind::Success, "success"),
    (CalloutKind::Question, "question"),
    (CalloutKind::Warning, "warning"),
    (CalloutKind::Failure, "failure"),
    (CalloutKind::Danger, "danger"),
    (CalloutKind::Bug, "bug"),
    (CalloutKind::Example, "example"),
    (CalloutKind::Quote, "quote"),
    (CalloutKind::Custom, "custom"),
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
    /// The built-in theme at the built-in base font size, built once:
    /// every pane and editor starts from it.
    fn default() -> Self {
        static DEFAULT: std::sync::LazyLock<Theme> =
            std::sync::LazyLock::new(|| Theme::from_config(&Config::defaults(), false));
        DEFAULT.clone()
    }
}

impl Theme {
    /// The colour of code of this kind.
    pub fn code_color(&self, kind: CodeKind) -> Hsla {
        self.code_syntax[kind as usize]
    }

    /// The theme a loaded config describes, in light or dark mode.
    pub fn from_config(config: &Config, dark: bool) -> Self {
        Self::from_tokens(
            config.theme.for_mode(dark),
            config.settings.appearance.base_font_size,
        )
    }

    /// Builds the theme from resolved tokens and a base size in points.
    /// Tokens that are missing or malformed fall back to the built-in value.
    pub fn from_tokens(tokens: &Tokens, base_font_points: u32) -> Self {
        let read = TokenReader { tokens };
        let base = px(base_font_points.max(1) as f32 * PIXELS_PER_POINT);
        let palette = Palette::from_tokens(tokens);
        let colors = read_colors(&palette);
        let scale = |name: &str, default: f32| base * read.number(name, default);
        let space = |name: &str, default: f32| px(read.number(name, default));
        let heading = |level: usize, default: f32| scale(&format!("font.scale.h{level}"), default);
        let mut theme = Self {
            body_font_family: read.text("font.text", "Charter").into(),
            ui_font_family: read.text("font.ui", "Charter").into(),
            code_font_family: read.text("font.code", "Courier New").into(),
            regular_weight: FontWeight(read.number("font.weight.regular", 400.)),
            medium_weight: FontWeight(read.number("font.weight.medium", 500.)),
            bold_weight: FontWeight(read.number("font.weight.bold", 700.)),
            body_font_size: scale("font.scale.body", 1.),
            title_font_size: scale("font.scale.title", 2.),
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
            script_scale: 0.7,
            superscript_rise: 0.4,
            subscript_drop: 0.2,
            note_color_contrast: 3.,
            line_height_factor: read.number("font.line-height.body", 1.6),
            code_line_height_factor: read.number("font.line-height.code", 1.45),
            heading_line_height_factor: read.number("font.heading.line-height", 1.3),
            heading_space_above: read.number("font.heading.space-above", 0.5),
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
            tab_stop_width: space("space.sm", 4.) * 1.5,
            tab_stop_gap: space("space.xs", 2.),
            inline_code_padding: space("space.xs", 2.) * 1.5,
            property_chip_padding: space("space.sm", 4.) * 1.5,
            property_chip_gap: space("space.sm", 4.),
            composition_underline_thickness: px(1.),
            problem_underline_thickness: px(1.5),
            sentence_tint_inset: space("space.xs", 2.) * 1.5,
            flag_underline_thickness: px(1.),
            flag_underline_rise: px(1.),
            link_card_height: px(112.),
            link_card_max_width: px(640.),
            link_card_ring: px(1.),
            link_card_icon_size: px(14.),
            link_card_image_aspect: 1.5,
            rule_thickness: px(1.),
            quote_bar_width: px(3.),
            quote_indent: space("space.xl", 16.),
            list_marker_width: base * 1.25,
            bullet_size: base * 0.3,
            checkbox_size: base * 0.875,
            property_key_width: base * 7.,
            checkbox_border_width: px(1.5),
            tab_columns: 4,
            ..colors
        };
        let line_height = theme.body_line_height();
        theme.table = TableTheme::from_tokens(&read, &palette, line_height);
        theme
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
        scaled.table = self.table.scaled(zoom);
        scaled
    }

    fn sizes_mut(&mut self) -> [&mut Pixels; 40] {
        [
            &mut self.body_font_size,
            &mut self.title_font_size,
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
            &mut self.tab_stop_width,
            &mut self.tab_stop_gap,
            &mut self.inline_code_padding,
            &mut self.property_chip_padding,
            &mut self.property_chip_gap,
            &mut self.composition_underline_thickness,
            &mut self.problem_underline_thickness,
            &mut self.sentence_tint_inset,
            &mut self.flag_underline_thickness,
            &mut self.flag_underline_rise,
            &mut self.link_card_height,
            &mut self.link_card_max_width,
            &mut self.link_card_icon_size,
            &mut self.rule_thickness,
            &mut self.quote_bar_width,
            &mut self.quote_indent,
            &mut self.list_marker_width,
            &mut self.bullet_size,
            &mut self.checkbox_size,
            &mut self.checkbox_border_width,
            &mut self.property_key_width,
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

    /// The body, interface and code font families.
    pub fn font_families(&self) -> [SharedString; 3] {
        [
            self.body_font_family.clone(),
            self.ui_font_family.clone(),
            self.code_font_family.clone(),
        ]
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

    /// The label colour of one version of a sync conflict.
    pub fn conflict_color(&self, side: ConflictSide) -> Hsla {
        self.conflict_sides[side as usize]
    }

    /// The tinted surface behind one version of a sync conflict.
    pub fn conflict_surface(&self, side: ConflictSide) -> Hsla {
        let mut color = self.conflict_color(side);
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

/// The editor's colours from `palette`, with every size left at zero for
/// [`Theme::from_tokens`] to fill in.
fn read_colors(palette: &Palette) -> Theme {
    let p = palette;
    Theme {
        background: p.background,
        surface: p.surface,
        text: p.text,
        text_muted: p.text_muted,
        text_faint: p.text_faint,
        heading_text: p.text,
        markup_dimmed: p.text_faint,
        code_text: p.text,
        code_background: p.code_background,
        code_highlight: p.code_highlight,
        code_syntax: p.code,
        selection: p.selection,
        cursor: p.accent,
        accent: p.accent,
        composition_underline: p.text,
        link: p.link,
        link_underline: p.link_underline,
        tag_background: p.hover,
        highlight: p.highlight,
        keycap_fill: keycap_colors(p.text).0,
        keycap_text: keycap_colors(p.text).1,
        divider: p.divider,
        error: p.conflict,
        shadow: p.shadow,
        popover: p.popover,
        popover_ring: p.popover_ring,
        search_match: p.search_match,
        active_search_match: p.active_search_match,
        tab_stop: p.tab_stop,
        math_brackets: [p.math_bracket_1, p.math_bracket_2, p.math_bracket_3],
        sentence: [p.sentence_short, p.sentence_medium, p.sentence_long],
        flag_spelling: p.flag_spelling,
        flag_mechanical: p.flag_mechanical,
        callout_tint: p.callout_tint,
        callout_colors: p.callouts.clone(),
        conflict_sides: [p.this_device, p.other_device],
        find_ui: FindUiTheme::from_palette(p),
        workspace: WorkspaceTheme::from_palette(p),
        table: TableTheme::from_palette(p),
        fold: FoldTheme::from_palette(p),
        ..zero_sizes()
    }
}

fn zero_sizes() -> Theme {
    let zero = px(0.);
    let black = hsla(0., 0., 0., 1.);
    Theme {
        find_ui: FindUiTheme::default(),
        workspace: WorkspaceTheme::default(),
        table: TableTheme::default(),
        fold: FoldTheme::default(),
        body_font_family: SharedString::default(),
        ui_font_family: SharedString::default(),
        code_font_family: SharedString::default(),
        regular_weight: FontWeight::NORMAL,
        medium_weight: FontWeight::MEDIUM,
        bold_weight: FontWeight::BOLD,
        body_font_size: zero,
        title_font_size: zero,
        heading_font_sizes: [zero; 6],
        small_font_size: zero,
        code_scale: 1.,
        script_scale: 1.,
        superscript_rise: 0.,
        subscript_drop: 0.,
        note_color_contrast: 1.,
        line_height_factor: 1.,
        code_line_height_factor: 1.,
        heading_line_height_factor: 1.,
        heading_space_above: 0.,
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
        tab_stop_width: zero,
        tab_stop_gap: zero,
        inline_code_padding: zero,
        property_chip_padding: zero,
        property_chip_gap: zero,
        composition_underline_thickness: zero,
        problem_underline_thickness: zero,
        sentence_tint_inset: zero,
        flag_underline_thickness: zero,
        flag_underline_rise: zero,
        link_card_height: zero,
        link_card_max_width: zero,
        link_card_ring: zero,
        link_card_icon_size: zero,
        link_card_image_aspect: 1.,
        rule_thickness: zero,
        quote_bar_width: zero,
        quote_indent: zero,
        list_marker_width: zero,
        bullet_size: zero,
        checkbox_size: zero,
        property_key_width: zero,
        checkbox_border_width: zero,
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
        code_highlight: black,
        code_syntax: [black; 7],
        selection: black,
        cursor: black,
        accent: black,
        composition_underline: black,
        link: black,
        link_underline: black,
        tag_background: black,
        highlight: black,
        keycap_fill: black,
        keycap_text: black,
        divider: black,
        error: black,
        shadow: black,
        popover: black,
        popover_ring: black,
        search_match: black,
        active_search_match: black,
        tab_stop: black,
        math_brackets: [black; 3],
        sentence: [black; 3],
        flag_spelling: black,
        flag_mechanical: black,
        callout_tint: 0.,
        callout_colors: CalloutColors(Vec::new()),
        conflict_sides: [black; 2],
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

    /// A colour token, or `fallback` when it's missing or malformed.
    /// With no fallback it's a loud magenta, so a token the built-in
    /// theme lacks shows up at once.
    fn color(&self, name: &str, fallback: Option<Hsla>) -> Hsla {
        self.tokens
            .text(name)
            .and_then(parse_color)
            .or(fallback)
            .unwrap_or(hsla(0.83, 1., 0.5, 1.))
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
    pub font_family: SharedString,
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
    pub row_font_size: Pixels,
    pub detail_font_size: Pixels,
    pub icon_size: Pixels,
    /// Indent per heading level in the outline.
    pub level_indent: Pixels,
    pub keycap: KeycapTheme,
    pub shadow_blur: Pixels,
    pub shadow_offset_y: Pixels,
    pub background: Hsla,
    pub shadow: Hsla,
    pub text: Hsla,
    pub detail_text: Hsla,
    pub match_text: Hsla,
    pub match_weight: FontWeight,
    pub icon: Hsla,
    pub selected_row: Hsla,
    pub hovered_row: Hsla,
    pub warning_text: Hsla,
}

impl Default for PickerTheme {
    fn default() -> Self {
        Self::from_palette(Palette::builtin())
    }
}

impl PickerTheme {
    /// The built-in sizes in the colours of `p`.
    pub fn from_palette(p: &Palette) -> Self {
        Self {
            font_family: PLATFORM_FONTS.0.into(),
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
            row_font_size: px(14.),
            detail_font_size: px(12.),
            icon_size: px(16.),
            level_indent: px(16.),
            keycap: KeycapTheme::default(),
            shadow_blur: px(32.),
            shadow_offset_y: px(8.),
            background: p.popover,
            shadow: p.popover_shadow,
            text: p.text,
            detail_text: p.text_detail,
            match_text: p.text_strong,
            match_weight: FontWeight::BOLD,
            icon: p.icon,
            selected_row: p.fill_strong,
            hovered_row: p.fill_faint,
            warning_text: p.conflict,
        }
    }
}

impl PickerTheme {
    pub fn font(&self) -> Font {
        font(self.font_family.clone())
    }
}

// ---- Workspace shell tokens (tabs, panes, sidebar, status bar, modals). ----

/// Sizes and colours for the workspace shell, from `size.*`, `space.*`,
/// `radius.*` and `color.*` in the config theme.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkspaceTheme {
    pub ui_font_size: Pixels,
    pub ui_small_font_size: Pixels,
    pub sidebar_width: Pixels,
    pub sidebar_min_width: Pixels,
    pub sidebar_max_width: Pixels,
    pub hover_edge_width: Pixels,
    pub divider_width: Pixels,
    /// Width of the invisible strip that grabs a divider.
    pub divider_grab_width: Pixels,
    pub focus_line_width: Pixels,
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
}

impl Default for WorkspaceTheme {
    fn default() -> Self {
        Self::from_palette(Palette::builtin())
    }
}

impl WorkspaceTheme {
    /// The built-in sizes in the colours of `p`.
    pub fn from_palette(p: &Palette) -> Self {
        Self {
            ui_font_size: px(13.),
            ui_small_font_size: px(12.),
            sidebar_width: px(260.),
            sidebar_min_width: px(160.),
            sidebar_max_width: px(520.),
            hover_edge_width: px(8.),
            divider_width: px(1.),
            divider_grab_width: px(6.),
            focus_line_width: px(2.),
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
            hover_background: p.fill_strong,
            list_hover_background: p.hover,
            text: p.text,
            text_muted: p.text_muted,
            text_faint: p.text_faint,
            divider: p.divider,
            accent: p.accent,
            on_accent: p.on_accent,
            conflict: p.conflict,
            shadow: p.shadow,
            backdrop: p.backdrop,
        }
    }
}

// ---- File tree and settings screen ----

/// Tokens for the file tree, the settings screen and their shared controls.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelTheme {
    pub font_family: SharedString,
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
}

impl Default for PanelTheme {
    fn default() -> Self {
        Self::from_palette(Palette::builtin())
    }
}

impl PanelTheme {
    /// The built-in sizes in the colours of `p`.
    pub fn from_palette(p: &Palette) -> Self {
        Self {
            font_family: PLATFORM_FONTS.0.into(),
            pane_background: p.background,
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
            background: p.surface,
            text: p.text,
            muted_text: p.text_detail,
            icon: p.icon,
            hover: p.fill_faint,
            selected: over(p.fill_strong, p.surface),
            selected_focused: over(p.fill_pressed, p.surface),
            focus_ring: p.focus(),
            active_text: p.text_strong,
            active_marker: p.text_strong,
            active_marker_width: px(2.),
            drop_target: p.drop_target,
            cut_opacity: p.cut_opacity,
            error_text: p.conflict,
            menu_background: p.popover,
            menu_shadow: p.popover_shadow,
            menu_shadow_blur: px(16.),
            menu_shadow_offset: px(4.),
            menu_width: px(220.),
            control_inset: px(2.),
            control_background: p.fill_strong,
            control_selected: p.accent,
            control_selected_text: p.on_accent,
            toggle_width: px(34.),
            toggle_height: px(20.),
            toggle_knob: p.knob,
            toggle_knob_inset: px(2.),
            sidebar_width: px(200.),
            content_max_width: px(640.),
            section_padding: px(24.),
            setting_gap: px(18.),
        }
    }
}

impl PanelTheme {
    /// The ring around whatever has keyboard focus.
    pub fn focus_ring(&self) -> BoxShadow {
        focus_ring(self.focus_ring)
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
    pub font_family: SharedString,
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
        Self::from_palette(Palette::builtin())
    }
}

impl FindUiTheme {
    /// The built-in sizes in the colours of `p`.
    pub fn from_palette(p: &Palette) -> Self {
        Self {
            font_family: PLATFORM_FONTS.0.into(),
            font_size: px(14.),
            small_font_size: px(12.),
            title_font_size: px(16.),
            text: p.text,
            muted_text: p.text_detail,
            disabled_text: p.text_faint,
            error_text: p.conflict,
            panel_background: p.surface,
            panel_shadow: p.shadow,
            panel_shadow_blur: px(12.),
            panel_padding: px(8.),
            gap: px(6.),
            radius: px(6.),
            button_size: px(28.),
            button_padding_x: px(10.),
            icon_size: px(16.),
            icon: p.icon,
            button_hover_background: p.fill,
            accent_background: p.accent,
            accent_text: p.on_accent,
            row_padding_y: px(3.),
            row_selected_background: p.fill_strong,
            result_indent: px(12.),
            match_background: p.search_match,
            search_panel_width: px(360.),
            dialog_width: px(320.),
            dialog_top_offset: px(96.),
            backdrop: p.backdrop,
        }
    }
}

#[cfg(test)]
mod tests {
    use gasp_config::loader::build_theme;

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

    /// The WCAG contrast ratio of `a` over `b`.
    fn contrast(a: Hsla, b: Hsla) -> f32 {
        let luminance = |color: Hsla| {
            let rgb = over(color, b).to_rgb();
            let channel = |c: f32| {
                if c <= 0.039_28 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * channel(rgb.r) + 0.7152 * channel(rgb.g) + 0.0722 * channel(rgb.b)
        };
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    fn palette(dark: bool) -> Palette {
        Palette::from_tokens(Config::defaults().theme.for_mode(dark))
    }

    #[test]
    fn dark_mode_has_its_own_palette() {
        let (light, dark) = (palette(false), palette(true));
        assert_eq!(&light, Palette::builtin());
        assert!(dark.background.l < 0.15, "a near-black page");
        assert!(dark.background.l > 0.05, "but not pure black");
        // Surfaces get lighter as they rise.
        assert!(dark.app_background.l < dark.background.l);
        assert!(dark.background.l < dark.popover.l);
        assert!(dark.popover.l < dark.tooltip.l);
        // The accent is light, not paint.
        assert!(dark.accent.l > 0.8 && dark.accent.s < 0.3);
    }

    #[test]
    fn text_reads_in_both_modes() {
        for dark in [false, true] {
            let p = palette(dark);
            for surface in [p.background, p.popover, p.app_background, p.card] {
                assert!(contrast(p.text, surface) >= 7., "body text, dark: {dark}");
                assert!(
                    contrast(p.text_muted, surface) >= 4.5,
                    "muted, dark: {dark}"
                );
            }
            for surface in [p.background, p.popover] {
                assert!(
                    contrast(p.text_detail, surface) >= 4.5,
                    "detail, dark: {dark}"
                );
            }
            for color in p.code {
                assert!(
                    contrast(color, p.code_background) >= 4.5,
                    "code, dark: {dark}"
                );
            }
            for back in [p.selection, p.highlight, p.active_search_match] {
                assert!(contrast(p.text, back) >= 4.5, "marked text, dark: {dark}");
            }
            assert!(contrast(p.on_accent, p.accent) >= 4.5);
            assert!(contrast(p.tooltip_text, p.tooltip) >= 7.);
        }
    }

    #[test]
    fn dark_callout_titles_read_on_their_tint() {
        let p = palette(true);
        for (kind, _) in CALLOUT_NAMES {
            let color = p.callouts.get(kind);
            let surface = over(
                Hsla {
                    a: p.callout_tint,
                    ..color
                },
                p.background,
            );
            assert!(contrast(color, surface) >= 4.5, "{kind:?}");
        }
    }

    #[test]
    fn a_vault_can_set_dark_colours() {
        let (tokens, _) =
            build_theme("theme.toml", Some("[dark.color]\ntext = \"#ff0000\"\n")).unwrap();
        let light = Theme::from_tokens(tokens.for_mode(false), 12);
        let dark = Theme::from_tokens(tokens.for_mode(true), 12);
        assert_eq!(dark.text, parse_color("#ff0000").unwrap());
        assert_ne!(light.text, dark.text);
        assert_eq!(dark.heading_text, dark.text);
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

// Text inputs ------------------------------------------------------------

/// Tokens for every one-line text input (`crate::text_input::TextInput`):
/// the picker query, find and search fields, settings fields, inline
/// renames and the note's title. Each input style picks from these.
#[derive(Clone, Debug)]
pub struct InputTheme {
    /// Replaced by the resolved UI font when an input is made.
    pub font_family: SharedString,
    /// Field and inline inputs.
    pub font_size: Pixels,
    /// The picker's query.
    pub query_font_size: Pixels,
    /// The note's title.
    pub title_font_size: Pixels,
    pub title_weight: FontWeight,
    /// Line height as a multiple of the font size.
    pub line_height_factor: f32,
    /// A field in a form or bar.
    pub field_height: Pixels,
    /// A field inside a list row.
    pub inline_height: Pixels,
    pub padding_x: Pixels,
    pub radius: Pixels,
    /// A field's fill. Translucent, so it reads on white and grey surfaces.
    pub background: Hsla,
    /// A field's fill while its text isn't valid.
    /// A focused field's fill: opaque, so the focus ring (a shadow)
    /// shows only around it.
    pub focused_background: Hsla,
    pub error_background: Hsla,
    pub focus_ring: Hsla,
    pub ring_width: Pixels,
    pub ring_blur: Pixels,
    pub text: Hsla,
    pub title_text: Hsla,
    pub placeholder: Hsla,
    pub selection: Hsla,
    pub caret: Hsla,
    pub caret_width: Pixels,
    pub composition_underline_thickness: Pixels,
}

impl Default for InputTheme {
    fn default() -> Self {
        Self::from_palette(Palette::builtin())
    }
}

impl InputTheme {
    /// The built-in sizes in the colours of `p`.
    pub fn from_palette(p: &Palette) -> Self {
        Self {
            font_family: PLATFORM_FONTS.0.into(),
            font_size: px(13.),
            query_font_size: px(16.),
            title_font_size: px(34.),
            title_weight: FontWeight::BOLD,
            line_height_factor: 1.5,
            field_height: px(28.),
            inline_height: px(24.),
            padding_x: px(8.),
            radius: px(6.),
            background: p.field,
            focused_background: p.background,
            error_background: p.field_error,
            focus_ring: p.focus(),
            ring_width: px(1.5),
            ring_blur: px(0.5),
            text: p.text,
            title_text: p.text_strong,
            placeholder: p.text_faint,
            selection: p.selection,
            caret: p.accent,
            caret_width: px(1.5),
            composition_underline_thickness: px(1.),
        }
    }
}

impl InputTheme {
    /// Takes `other`'s colours and font and keeps this input's sizes, so
    /// an input follows a theme or font change, or the installed fonts
    /// arriving after startup.
    pub fn recolor(&mut self, other: &InputTheme) {
        self.font_family = other.font_family.clone();
        self.background = other.background;
        self.focused_background = other.focused_background;
        self.error_background = other.error_background;
        self.focus_ring = other.focus_ring;
        self.text = other.text;
        self.title_text = other.title_text;
        self.placeholder = other.placeholder;
        self.selection = other.selection;
        self.caret = other.caret;
    }

    /// Height of one line of input text at `font_size`.
    pub fn line_height(&self, font_size: Pixels) -> Pixels {
        font_size * self.line_height_factor
    }

    /// The ring around a focused field.
    pub fn focus_ring(&self) -> BoxShadow {
        focus_ring(self.focus_ring)
    }
}

// ---- Workspace chrome and the shared UI primitives in `crate::ui` ----
//
// The sidebar, tab bar, note header, status bar, menus, tooltips and icon
// buttons. Values follow `crates/config/defaults/theme.toml`: Charter for
// the UI, a black accent and a light grey app background around a white
// note surface.

/// The UI font and its fallbacks, first installed one wins. Charter ships
/// with macOS; the rest are serifs common on Windows and Linux.
pub const UI_FONT_CANDIDATES: [&str; 6] = [
    "Charter",
    "Bitstream Charter",
    "Georgia",
    "Liberation Serif",
    "DejaVu Serif",
    "Times New Roman",
];

/// The face key glyphs are set in, first installed one wins: the
/// platform's own sans, the kind printed on keyboards. Keys are glyphs,
/// not prose, so they don't take the serif interface font.
#[cfg(target_os = "macos")]
pub const KEY_FONT_CANDIDATES: [&str; 1] = [".SystemUIFont"];
#[cfg(target_os = "windows")]
pub const KEY_FONT_CANDIDATES: [&str; 2] = ["Segoe UI", "Arial"];
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const KEY_FONT_CANDIDATES: [&str; 6] = [
    "Inter",
    "Cantarell",
    "Noto Sans",
    "Liberation Sans",
    "DejaVu Sans",
    "FreeSans",
];

/// The first of `candidates` among `installed`, or the first candidate.
fn first_installed(candidates: &[&str], installed: &[String]) -> SharedString {
    candidates
        .iter()
        .find(|candidate| installed.iter().any(|name| name == *candidate))
        .unwrap_or(&candidates[0])
        .to_string()
        .into()
}

/// How much of the text colour a keycap's fill takes.
const KEYCAP_FILL_ALPHA: f32 = 0.075;
/// How much of the text colour a keycap's glyphs take.
const KEYCAP_GLYPH_ALPHA: f32 = 0.82;

/// A key cap's fill and glyph colours on a surface whose text is `text`.
pub fn keycap_colors(text: Hsla) -> (Hsla, Hsla) {
    let fill = Hsla {
        a: text.a * KEYCAP_FILL_ALPHA,
        ..text
    };
    let glyph = Hsla {
        a: text.a * KEYCAP_GLYPH_ALPHA,
        ..text
    };
    (fill, glyph)
}

/// A shortcut drawn as a flat chip, the same wherever one appears: the
/// settings screen, the palette, menus, tooltips and the help dialog.
/// The fill and glyphs are the surface's text colour at low and high
/// strength, so a chip reads on any background without a border or
/// shadow.
#[derive(Clone, Debug, PartialEq)]
pub struct KeycapTheme {
    pub font_family: SharedString,
    pub font_size: Pixels,
    pub font_weight: FontWeight,
    pub icon_size: Pixels,
    pub height: Pixels,
    pub padding_x: Pixels,
    /// Space between the keys of a chord inside one chip.
    pub gap: Pixels,
    pub radius: Pixels,
    pub fill: Hsla,
    pub glyph: Hsla,
}

impl Default for KeycapTheme {
    fn default() -> Self {
        KeycapTheme {
            font_family: KEY_FONT_CANDIDATES[0].into(),
            font_size: px(12.5),
            font_weight: FontWeight::MEDIUM,
            icon_size: px(12.),
            height: px(22.),
            padding_x: px(6.),
            gap: px(4.),
            radius: px(5.),
            fill: hsla(0., 0., 0., 0.),
            glyph: hsla(0., 0., 0., 0.),
        }
        .on_text(Palette::builtin().text)
    }
}

impl KeycapTheme {
    /// The chip recoloured for a surface whose text is `text`.
    pub fn on_text(mut self, text: Hsla) -> KeycapTheme {
        (self.fill, self.glyph) = keycap_colors(text);
        self
    }

    /// A chip that stands out from its neighbours, such as the key a
    /// search found: a deeper fill and full-strength glyphs.
    pub fn emphasized(mut self) -> KeycapTheme {
        self.fill.a *= 2.4;
        self.glyph.a = 1.;
        self
    }

    /// The smaller chip menus and tooltips use, where it sits beside a
    /// label rather than standing alone.
    pub fn compact(mut self) -> KeycapTheme {
        self.font_size = px(11.5);
        self.icon_size = px(11.);
        self.height = px(18.);
        self.padding_x = px(4.);
        self.gap = px(3.);
        self.radius = px(4.);
        self
    }

    /// The default chip set in the first installed key face.
    pub fn with_installed_fonts(installed: &[String]) -> KeycapTheme {
        KeycapTheme {
            font_family: first_installed(&KEY_FONT_CANDIDATES, installed),
            ..KeycapTheme::default()
        }
    }
}

/// Sizes, fonts and colours for the workspace chrome and `crate::ui`.
#[derive(Clone, Debug)]
pub struct UiTheme {
    pub font_family: SharedString,
    pub font_size: Pixels,
    pub small_font_size: Pixels,
    pub space_xs: Pixels,
    pub space_sm: Pixels,
    pub space_md: Pixels,
    pub space_lg: Pixels,
    pub space_xl: Pixels,
    pub app_background: Hsla,
    /// Each pane draws its note on a surface of the editor's background.
    pub surface_radius: Pixels,
    /// A hairline ring (a shadow, not a border) around the note surface.
    pub surface_ring: Hsla,
    pub surface_ring_width: Pixels,
    /// Separators in menus.
    pub hairline: Pixels,
    /// Space between the note surface and the window edge or the next pane.
    pub surface_gap: Pixels,
    /// The widest the note column gets (`size.editor-max-width`).
    pub readable_width: Pixels,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_faint: Hsla,
    pub icon_button_size: Pixels,
    pub icon_button_radius: Pixels,
    pub icon_size: Pixels,
    pub small_icon_size: Pixels,
    pub icon: Hsla,
    pub icon_active: Hsla,
    pub icon_disabled: Hsla,
    pub control_hover: Hsla,
    pub control_pressed: Hsla,
    /// The fill of a control that's on, such as the current sidebar view.
    pub control_active: Hsla,
    pub tooltip_background: Hsla,
    pub tooltip_text: Hsla,
    pub tooltip_hint: Hsla,
    pub tooltip_padding_x: Pixels,
    pub tooltip_padding_y: Pixels,
    pub tooltip_radius: Pixels,
    pub menu_background: Hsla,
    pub menu_radius: Pixels,
    pub menu_padding: Pixels,
    pub menu_min_width: Pixels,
    pub menu_max_width: Pixels,
    pub menu_row_height: Pixels,
    pub menu_row_padding_x: Pixels,
    pub menu_row_radius: Pixels,
    pub menu_highlight: Hsla,
    pub menu_separator: Hsla,
    pub menu_shadow: Hsla,
    pub menu_shadow_blur: Pixels,
    pub menu_shadow_offset: Pixels,
    pub menu_ring: Hsla,
    /// How matched characters stand out in a suggestion.
    /// Matched characters in a suggestion: heavier, and a shade darker,
    /// which GPUI needs to keep a weight change within one line.
    pub match_text: Hsla,
    pub match_weight: FontWeight,
    /// Suggestion rows shown at once; the list scrolls past them.
    pub suggestion_rows: usize,
    /// Space between the line being typed and its suggestions.
    pub suggestion_gap: Pixels,
    /// The column an emoji or symbol sits in before its name.
    pub suggestion_glyph_width: Pixels,
    pub suggestion_glyph_size: Pixels,
    /// The chip beside a pasted address that offers to make a card.
    pub card_chip_height: Pixels,
    /// The popover a link shows its note in when hovered.
    pub hover_preview_width: Pixels,
    pub hover_preview_height: Pixels,
    pub hover_preview_header_height: Pixels,
    /// Space between the preview's edge and the note's text.
    pub hover_preview_padding: Pixels,
    /// How much smaller than the editor the previewed note is drawn.
    pub hover_preview_zoom: f32,
    /// A hovered footnote's text, and short notices.
    pub hover_footnote_width: Pixels,
    /// The notices at the window's bottom right: how wide they are, and
    /// how long one that reports something done stays.
    pub notice_width: Pixels,
    pub notice_duration: std::time::Duration,
    /// The card a grammar flag shows: the widest its message runs, and the
    /// narrowest the card gets so its buttons fit.
    pub flag_card_width: Pixels,
    pub flag_card_min_width: Pixels,
    /// How wide a flag's card may grow to keep its buttons on one row;
    /// its message still wraps at `flag_card_width`.
    pub flag_card_max_width: Pixels,
    /// One line of interface text, for measuring a wrapped message.
    pub text_line_height: Pixels,
    /// How long the pointer rests on a link before its preview opens.
    pub hover_preview_delay: std::time::Duration,
    /// How long the preview stays after the pointer leaves, so it can
    /// cross the gap to the popover.
    pub hover_preview_grace: std::time::Duration,
    /// The thumb that shows where a scrolling list is, and how much of it
    /// shows.
    pub scroll_thumb: Hsla,
    pub scroll_thumb_width: Pixels,
    pub tab_bar_height: Pixels,
    /// Room at the window's top-left for the platform's own window
    /// buttons, where they're drawn over the app (macOS).
    pub window_buttons_width: Pixels,
    pub tab_height: Pixels,
    pub tab_radius: Pixels,
    pub tab_min_width: Pixels,
    pub tab_max_width: Pixels,
    pub tab_padding_x: Pixels,
    pub tab_gap: Pixels,
    pub tab_shadow: Hsla,
    pub tab_shadow_blur: Pixels,
    pub dirty_dot_size: Pixels,
    pub conflict: Hsla,
    /// The sync indicator at rest (synced).
    pub sync_quiet: Hsla,
    /// The sync indicator while it works or waits (syncing, offline).
    pub sync_busy: Hsla,
    /// The sync indicator when it needs the person (conflict, failure).
    pub sync_attention: Hsla,
    /// One turn of the syncing arrows.
    pub sync_spin: std::time::Duration,
    pub popover_width: Pixels,
    pub popover_padding: Pixels,
    /// The strip over a note that has a sync conflict.
    pub banner_background: Hsla,
    pub banner_padding_y: Pixels,
    pub note_header_height: Pixels,
    pub sidebar_padding: Pixels,
    pub sidebar_footer_height: Pixels,
    pub tree_row_height: Pixels,
    pub tree_indent: Pixels,
    pub tree_row_radius: Pixels,
    pub tree_row_gap: Pixels,
    /// How close to the tree's top or bottom a drag has to come for the
    /// tree to scroll toward it.
    pub tree_autoscroll_band: Pixels,
    /// How far the tree scrolls in a second with a drag at its very edge;
    /// it slows toward the band's inner side.
    pub tree_autoscroll_speed: Pixels,
    /// How often the tree scrolls a step while a drag waits at its edge.
    pub tree_autoscroll_frame: std::time::Duration,
    pub tree_active_background: Hsla,
    pub tree_hover_background: Hsla,
    pub indent_guide: Hsla,
    /// The ring around the selected row while the tree has the keyboard.
    pub tree_focus_ring: Hsla,
    pub indent_guide_width: Pixels,
    pub status_height: Pixels,
    pub status_gap: Pixels,
    /// Room the cursor position keeps whatever it says, up to "000:00",
    /// so moving the caret never moves the status bar's other widgets.
    pub status_position_width: Pixels,
    pub help_row_height: Pixels,
    /// The shortcuts sheet that holding Mod shows: its width, row height
    /// and how wide one column of it is at least.
    pub sheet_width: Pixels,
    pub sheet_row_height: Pixels,
    pub sheet_column_width: Pixels,
    pub keycap: KeycapTheme,
    pub backdrop: Hsla,
    /// Secondary text that still has to be read, such as a note's folder
    /// in a list: between `text_muted` and `text_faint`.
    pub text_detail: Hsla,
    /// Dialogs over the workspace: pickers, search, export, help, prompts.
    pub dialog_radius: Pixels,
    pub dialog_padding: Pixels,
    pub dialog_width: Pixels,
    pub small_dialog_width: Pixels,
    pub wide_dialog_width: Pixels,
    /// Source shown in a dialog, such as export's HTML preview: the code
    /// font, its fill and how tall the scrolling box is.
    pub code_font_family: SharedString,
    pub source_background: Hsla,
    pub source_preview_height: Pixels,
    pub dialog_top_offset: Pixels,
    pub dialog_shadow: Hsla,
    pub dialog_shadow_blur: Pixels,
    pub dialog_shadow_offset: Pixels,
    /// A list row in a dialog or the launcher.
    pub row_height: Pixels,
    /// A short row that belongs to the row above it, such as a search hit.
    pub compact_row_height: Pixels,
    pub row_radius: Pixels,
    pub row_padding_x: Pixels,
    /// The row the keyboard is on.
    pub row_selected: Hsla,
    /// The row under the pointer, fainter than the selection so both can
    /// show at once.
    pub row_hover: Hsla,
    /// Text buttons in dialogs and bars.
    pub button_height: Pixels,
    pub button_padding_x: Pixels,
    pub button_background: Hsla,
    /// Room kept beside wrapped text for a one-word button on its first
    /// line, such as a backlink's Link.
    pub inline_button_width: Pixels,
    pub accent: Hsla,
    pub on_accent: Hsla,
    /// What keyboard focus looks like: see [`focus_ring`].
    pub focus_ring: Hsla,
    /// Marks a match inside text, such as a search excerpt.
    pub match_background: Hsla,
    pub error: Hsla,
    /// The floating find bar.
    pub find_bar_width: Pixels,
    /// A file tree row while something is dragged over it.
    pub drop_target: Hsla,
    /// Opacity of a file tree entry that's been cut and waits to be pasted.
    pub cut_opacity: f32,
    /// The soft edge where tabs run past the tab strip.
    pub tab_fade_width: Pixels,
    /// The smallest a pane gets while its divider is dragged.
    pub pane_min_width: Pixels,
    pub pane_min_height: Pixels,
    /// The line a divider shows under the pointer and while dragged.
    pub divider_active: Hsla,
    pub divider_line_width: Pixels,
    /// Where a dragged tab will land on a pane: a soft fill with a ring.
    pub drop_zone: Hsla,
    pub drop_zone_ring: Hsla,
    /// How long the drop zone takes to move to a new side.
    pub drop_zone_motion: std::time::Duration,
    /// The bar between tabs where a dragged tab will go.
    pub drop_indicator: Hsla,
    pub drop_indicator_width: Pixels,
    /// Opacity of a tab's place in the strip while it's dragged.
    pub dragged_tab_opacity: f32,
    /// Where a dragged tab's stand-in hangs from the pointer.
    pub drag_preview_offset: Point<Pixels>,
    /// The note surface each pane draws on, and the active tab.
    pub note_background: Hsla,
    /// The shadow under the sidebar where it slides over the note.
    pub overlay_shadow: Hsla,
    /// The print dialog and its page preview.
    pub print: PrintTheme,
    /// Toolbars' sizes, from the theme's `toolbar.` tokens.
    pub toolbar: ToolbarTheme,
}

/// Sizes for toolbars, read from the theme's `toolbar.` tokens so a vault
/// can change them. Colours come from [`UiTheme`]'s controls.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolbarTheme {
    pub compact_button: Pixels,
    pub comfortable_button: Pixels,
    pub compact_gap: Pixels,
    pub comfortable_gap: Pixels,
    pub compact_icon: Pixels,
    pub comfortable_icon: Pixels,
    /// Inside a bar that floats or runs down a window edge.
    pub padding: Pixels,
    pub button_radius: Pixels,
    /// Between a button's icon and its label.
    pub label_gap: Pixels,
    /// Between a floating bar and the text it belongs to.
    pub float_gap: Pixels,
    /// How near its edge the pointer comes before a bar shown on hover appears.
    pub hover_edge: Pixels,
    pub separator_width: Pixels,
}

impl Default for ToolbarTheme {
    fn default() -> Self {
        Self::from_tokens(&Config::defaults().theme)
    }
}

impl ToolbarTheme {
    pub fn from_tokens(tokens: &Tokens) -> Self {
        let read = TokenReader { tokens };
        let size = |name: &str, default: f32| px(read.number(name, default));
        ToolbarTheme {
            compact_button: size("toolbar.compact-button", 26.),
            comfortable_button: size("toolbar.comfortable-button", 32.),
            compact_gap: size("toolbar.compact-gap", 2.),
            comfortable_gap: size("toolbar.comfortable-gap", 6.),
            compact_icon: size("toolbar.compact-icon", 16.),
            comfortable_icon: size("toolbar.comfortable-icon", 18.),
            padding: size("toolbar.padding", 4.),
            button_radius: size("toolbar.button-radius", 6.),
            label_gap: size("toolbar.label-gap", 6.),
            float_gap: size("toolbar.float-gap", 8.),
            hover_edge: size("toolbar.hover-edge", 10.),
            separator_width: size("toolbar.separator-width", 1.),
        }
    }

    /// A button's side at `density`.
    pub fn button(&self, density: gasp_config::toolbars::Density) -> Pixels {
        match density {
            gasp_config::toolbars::Density::Compact => self.compact_button,
            gasp_config::toolbars::Density::Comfortable => self.comfortable_button,
        }
    }

    /// The space between buttons at `density`.
    pub fn gap(&self, density: gasp_config::toolbars::Density) -> Pixels {
        match density {
            gasp_config::toolbars::Density::Compact => self.compact_gap,
            gasp_config::toolbars::Density::Comfortable => self.comfortable_gap,
        }
    }

    /// A floating bar's corners: concentric with its buttons'.
    pub fn bar_radius(&self) -> Pixels {
        self.button_radius + self.padding
    }
}

impl Default for UiTheme {
    fn default() -> Self {
        Self::from_palette(Palette::builtin())
    }
}

impl UiTheme {
    /// The built-in sizes in the colours of `p`.
    pub fn from_palette(p: &Palette) -> Self {
        Self {
            font_family: UI_FONT_CANDIDATES[0].into(),
            font_size: px(14.),
            small_font_size: px(12.),
            space_xs: px(2.),
            space_sm: px(4.),
            space_md: px(8.),
            space_lg: px(12.),
            space_xl: px(16.),
            app_background: p.app_background,
            surface_radius: px(8.),
            surface_ring: p.ring,
            surface_ring_width: px(1.),
            hairline: px(1.),
            surface_gap: px(8.),
            readable_width: px(720.),
            text: p.text,
            text_muted: p.text_muted,
            text_faint: p.text_faint,
            icon_button_size: px(28.),
            icon_button_radius: px(6.),
            icon_size: px(18.),
            small_icon_size: px(14.),
            icon: p.icon,
            icon_active: p.icon_strong,
            icon_disabled: p.icon_disabled,
            control_hover: p.fill,
            control_pressed: p.fill_pressed,
            control_active: p.fill_strong,
            tooltip_background: p.tooltip,
            tooltip_text: p.tooltip_text,
            tooltip_hint: p.tooltip_hint,
            tooltip_padding_x: px(8.),
            tooltip_padding_y: px(4.),
            tooltip_radius: px(6.),
            menu_background: p.popover,
            menu_radius: px(8.),
            menu_padding: px(4.),
            menu_min_width: px(220.),
            menu_max_width: px(360.),
            menu_row_height: px(28.),
            menu_row_padding_x: px(10.),
            menu_row_radius: px(5.),
            menu_highlight: p.fill_strong,
            menu_separator: p.divider,
            menu_shadow: p.popover_shadow,
            menu_shadow_blur: px(24.),
            menu_shadow_offset: px(8.),
            menu_ring: p.popover_ring,
            match_text: p.text_strong,
            match_weight: FontWeight::BOLD,
            suggestion_rows: 8,
            suggestion_gap: px(4.),
            suggestion_glyph_width: px(20.),
            suggestion_glyph_size: px(16.),
            card_chip_height: px(26.),
            hover_preview_width: px(480.),
            hover_preview_height: px(360.),
            hover_preview_header_height: px(32.),
            hover_preview_padding: px(16.),
            hover_preview_zoom: 0.875,
            hover_footnote_width: px(360.),
            notice_width: px(380.),
            notice_duration: std::time::Duration::from_secs(6),
            flag_card_width: px(320.),
            flag_card_min_width: px(220.),
            flag_card_max_width: px(440.),
            text_line_height: px(20.),
            hover_preview_delay: std::time::Duration::from_millis(350),
            hover_preview_grace: std::time::Duration::from_millis(250),
            scroll_thumb: p.divider_active,
            scroll_thumb_width: px(3.),
            tab_bar_height: px(40.),
            window_buttons_width: if cfg!(target_os = "macos") {
                px(72.)
            } else {
                px(0.)
            },
            tab_height: px(30.),
            tab_radius: px(8.),
            tab_min_width: px(110.),
            tab_max_width: px(180.),
            tab_padding_x: px(10.),
            tab_gap: px(2.),
            tab_shadow: p.tab_shadow,
            tab_shadow_blur: px(3.),
            dirty_dot_size: px(7.),
            conflict: p.conflict,
            sync_quiet: p.text_faint,
            sync_busy: p.syncing,
            sync_attention: p.conflict,
            sync_spin: std::time::Duration::from_millis(1600),
            popover_width: px(320.),
            popover_padding: px(12.),
            banner_background: p.fill_faint,
            banner_padding_y: px(8.),
            note_header_height: px(44.),
            sidebar_padding: px(8.),
            sidebar_footer_height: px(44.),
            tree_row_height: px(30.),
            tree_indent: px(18.),
            tree_row_radius: px(6.),
            tree_row_gap: px(7.),
            // About two rows.
            tree_autoscroll_band: px(60.),
            tree_autoscroll_speed: px(900.),
            tree_autoscroll_frame: std::time::Duration::from_millis(16),
            tree_active_background: p.fill_strong,
            tree_hover_background: p.fill_faint,
            indent_guide: p.indent_guide,
            tree_focus_ring: p.focus(),
            indent_guide_width: px(1.),
            status_height: px(24.),
            status_gap: px(16.),
            status_position_width: px(40.),
            help_row_height: px(32.),
            sheet_width: px(920.),
            sheet_row_height: px(26.),
            sheet_column_width: px(260.),
            keycap: KeycapTheme::default().on_text(p.text),
            backdrop: p.backdrop,
            text_detail: p.text_detail,
            dialog_radius: px(12.),
            dialog_padding: px(6.),
            dialog_width: px(560.),
            small_dialog_width: px(360.),
            wide_dialog_width: px(640.),
            code_font_family: MONO_FALLBACKS[0].into(),
            // The faint fill buttons use, laid over the dialog, so the box
            // shows in both modes (the note's code fill matches the
            // popover in dark mode).
            source_background: over(p.fill, p.popover),
            source_preview_height: px(300.),
            dialog_top_offset: px(96.),
            dialog_shadow: p.popover_shadow,
            dialog_shadow_blur: px(40.),
            dialog_shadow_offset: px(12.),
            row_height: px(36.),
            compact_row_height: px(26.),
            row_radius: px(6.),
            row_padding_x: px(10.),
            row_selected: p.fill_strong,
            row_hover: p.fill_faint,
            button_height: px(28.),
            button_padding_x: px(12.),
            button_background: p.fill,
            inline_button_width: px(52.),
            accent: p.accent,
            on_accent: p.on_accent,
            focus_ring: p.focus(),
            match_background: p.search_match,
            error: p.conflict,
            find_bar_width: px(480.),
            drop_target: p.drop_target,
            cut_opacity: p.cut_opacity,
            tab_fade_width: px(24.),
            pane_min_width: px(240.),
            pane_min_height: px(160.),
            divider_active: p.divider_active,
            divider_line_width: px(2.),
            drop_zone: p.drop_zone,
            drop_zone_ring: p.drop_zone_ring,
            drop_zone_motion: std::time::Duration::from_millis(120),
            drop_indicator: p.drop_indicator,
            drop_indicator_width: px(2.),
            dragged_tab_opacity: 0.35,
            drag_preview_offset: point(px(10.), px(14.)),
            note_background: p.background,
            overlay_shadow: p.shadow,
            print: PrintTheme::from_palette(p),
            toolbar: ToolbarTheme::default(),
        }
    }
}

impl UiTheme {
    /// The UI font with figures that are all one width, for numbers that
    /// change in place, such as the status bar's counts.
    pub fn tabular_font(&self) -> Font {
        Font {
            features: gpui::FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)])),
            ..font(self.font_family.clone())
        }
    }

    /// The shadow and hairline ring under dialogs: larger and softer than
    /// a menu's, since a dialog sits higher.
    pub fn dialog_shadows(&self) -> Vec<BoxShadow> {
        vec![
            BoxShadow {
                color: self.dialog_shadow,
                offset: point(px(0.), self.dialog_shadow_offset),
                blur_radius: self.dialog_shadow_blur,
                spread_radius: px(0.),
            },
            self.ring(self.menu_ring),
        ]
    }

    /// The ring around whatever has keyboard focus.
    pub fn focus(&self) -> BoxShadow {
        focus_ring(self.focus_ring)
    }

    /// The default tokens with the first candidate UI font found among
    /// `installed` font family names.
    pub fn with_installed_fonts(installed: &[String]) -> UiTheme {
        UiTheme::themed(Palette::builtin(), installed)
    }

    /// The tokens in the colours of `palette`, with the first candidate
    /// UI font found among `installed` font family names.
    pub fn themed(palette: &Palette, installed: &[String]) -> UiTheme {
        UiTheme {
            font_family: first_installed(&UI_FONT_CANDIDATES, installed),
            code_font_family: first_installed(MONO_FALLBACKS, installed),
            keycap: KeycapTheme::with_installed_fonts(installed).on_text(palette.text),
            ..UiTheme::from_palette(palette)
        }
    }

    /// The shadow under menus and tooltips, with a hairline ring.
    pub fn menu_shadows(&self) -> Vec<BoxShadow> {
        vec![
            BoxShadow {
                color: self.menu_shadow,
                offset: point(px(0.), self.menu_shadow_offset),
                blur_radius: self.menu_shadow_blur,
                spread_radius: px(0.),
            },
            self.ring(self.menu_ring),
        ]
    }

    /// The ring around the note surface.
    pub fn surface_shadows(&self) -> Vec<BoxShadow> {
        vec![self.ring(self.surface_ring)]
    }

    /// The lift under the active tab.
    pub fn tab_shadows(&self) -> Vec<BoxShadow> {
        vec![
            BoxShadow {
                color: self.tab_shadow,
                offset: point(px(0.), px(1.)),
                blur_radius: self.tab_shadow_blur,
                spread_radius: px(0.),
            },
            self.ring(self.surface_ring),
        ]
    }

    /// A hairline ring of `color`, drawn as a shadow.
    pub fn ring(&self, color: Hsla) -> BoxShadow {
        BoxShadow {
            color,
            offset: point(px(0.), px(0.)),
            blur_radius: px(RING_BLUR),
            spread_radius: self.surface_ring_width,
        }
    }
}

// Print dialog ------------------------------------------------------------

/// The print dialog: its page preview, a column of white pages on a
/// neutral well, and the settings beside it.
#[derive(Clone, Debug, PartialEq)]
pub struct PrintTheme {
    /// How wide each previewed page is drawn.
    pub page_width: Pixels,
    /// Space around the pages inside the well, and between them.
    pub well_padding: Pixels,
    pub page_gap: Pixels,
    /// The well is as tall as the window allows, within these; the
    /// allowance is the room the rest of the dialog and its offset take.
    pub preview_min_height: Pixels,
    pub preview_max_height: Pixels,
    pub window_allowance: Pixels,
    pub well_radius: Pixels,
    /// The settings column beside the preview.
    pub settings_width: Pixels,
    pub settings_padding: Pixels,
    pub setting_row_height: Pixels,
    /// Paper is white in both modes: it's what comes out of the printer.
    pub paper: Hsla,
    /// The page-shaped placeholder shown until the first page is drawn.
    pub placeholder: Hsla,
    pub well: Hsla,
    pub page_shadow: Hsla,
    pub page_shadow_blur: Pixels,
    pub page_shadow_offset: Pixels,
    pub page_ring: Hsla,
    /// The widest a layout error's text runs inside the well.
    pub message_width: Pixels,
    /// How long settings rest before the preview is laid out again.
    pub debounce: std::time::Duration,
}

impl Default for PrintTheme {
    fn default() -> Self {
        Self::from_palette(Palette::builtin())
    }
}

impl PrintTheme {
    pub fn from_palette(p: &Palette) -> Self {
        let paper = hsla(0., 0., 1., 1.);
        // A shade under the dialog: the window's colour in dark mode, and
        // a touch deeper in light mode, where the window is nearly as
        // light as the dialog.
        let dark = p.popover.l < 0.5;
        let well = if dark {
            p.app_background
        } else {
            over(p.fill, p.app_background)
        };
        PrintTheme {
            page_width: px(400.),
            well_padding: px(24.),
            page_gap: px(20.),
            preview_min_height: px(280.),
            preview_max_height: px(620.),
            window_allowance: px(200.),
            well_radius: px(8.),
            settings_width: px(290.),
            settings_padding: px(12.),
            setting_row_height: px(40.),
            paper,
            placeholder: over(paper.opacity(0.55), well),
            well,
            page_shadow: p.popover_shadow,
            page_shadow_blur: px(10.),
            page_shadow_offset: px(2.),
            page_ring: p.popover_ring,
            message_width: px(320.),
            debounce: std::time::Duration::from_millis(150),
        }
    }

    /// The soft shadow under each page, with a hairline ring.
    pub fn page_shadows(&self) -> Vec<BoxShadow> {
        vec![
            BoxShadow {
                color: self.page_shadow,
                offset: point(px(0.), self.page_shadow_offset),
                blur_radius: self.page_shadow_blur,
                spread_radius: px(0.),
            },
            BoxShadow {
                color: self.page_ring,
                offset: point(px(0.), px(0.)),
                blur_radius: px(RING_BLUR),
                spread_radius: px(1.),
            },
        ]
    }

    /// The well's width: a page and the padding either side.
    pub fn well_width(&self) -> Pixels {
        self.page_width + self.well_padding * 2.
    }
}

// Settings screen ---------------------------------------------------------

/// Accent colours the settings screen offers as swatches, as written to
/// `color.accent` in `theme.toml`. The first is the built-in accent.
pub const ACCENT_CHOICES: [&str; 6] = [
    "#000000", "#2f5fd0", "#7048c8", "#1f8a4c", "#c2541b", "#c02b4a",
];

/// The same hues for dark mode, as written to `dark.color.accent`: light
/// enough to read on the dark note (at least 4.5:1). The first is the
/// built-in dark accent.
pub const DARK_ACCENT_CHOICES: [&str; 6] = [
    "#ebe7e0", "#8fb0f5", "#b59cf2", "#6fcf97", "#f0a06c", "#f38ba3",
];

/// Tokens for the settings screen: the modal, its section list, the
/// grouped rows and their controls. Fonts and colours come from the
/// config theme's `font.*` and `color.*` tokens and spacing from
/// `space.*`, `radius.*` and `size.*`, like [`Theme::from_tokens`].
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsTheme {
    pub font_family: SharedString,
    pub code_font_family: SharedString,
    pub text_size: Pixels,
    pub small_text_size: Pixels,
    pub page_title_size: Pixels,
    pub strong_weight: FontWeight,
    pub line_height_factor: f32,
    /// Share of the window the modal takes, up to the maximums.
    pub modal_fraction: f32,
    pub modal_max_width: Pixels,
    pub modal_max_height: Pixels,
    pub modal_radius: Pixels,
    pub nav_width: Pixels,
    pub nav_padding: Pixels,
    pub nav_item_height: Pixels,
    pub nav_group_gap: Pixels,
    pub content_padding_x: Pixels,
    pub content_padding_y: Pixels,
    pub content_max_width: Pixels,
    pub card_gap: Pixels,
    pub card_padding_x: Pixels,
    pub card_radius: Pixels,
    pub row_padding_y: Pixels,
    /// Rows in long lists, such as the shortcuts, sit closer together.
    pub list_row_padding_y: Pixels,
    /// Space between a row's text column and its control column.
    pub row_gap: Pixels,
    /// The narrowest a row's text column gets before its controls wrap.
    pub text_min_width: Pixels,
    /// A dialog with only a short message and a button, such as one
    /// saying there's nothing to show yet.
    pub empty_dialog_width: Pixels,
    /// How tall the Appearance page's sample note is.
    pub preview_height: Pixels,
    /// The section list's share of the modal in narrow windows.
    pub nav_fraction: f32,
    pub text_gap: Pixels,
    pub control_gap: Pixels,
    pub gap_xs: Pixels,
    pub gap_sm: Pixels,
    pub radius: Pixels,
    pub icon_size: Pixels,
    pub small_icon_size: Pixels,
    pub hairline: Pixels,
    pub control_height: Pixels,
    pub control_padding_x: Pixels,
    pub toggle_width: Pixels,
    pub toggle_height: Pixels,
    pub toggle_knob_inset: Pixels,
    /// Room between a segmented control's track and its segments.
    pub segment_inset: Pixels,
    pub swatch_size: Pixels,
    /// The card-coloured gap between a swatch and its ring.
    pub swatch_gap: Pixels,
    /// How thick the ring around the chosen swatch is.
    pub swatch_ring: Pixels,
    pub stepper_value_width: Pixels,
    pub field_width: Pixels,
    /// The box a shortcut is pressed into.
    pub capture_field_width: Pixels,
    pub hex_field_width: Pixels,
    /// A font dropdown's width, the same for every font picked.
    pub font_button_width: Pixels,
    pub menu_width: Pixels,
    pub menu_max_height: Pixels,
    pub menu_offset: Pixels,
    /// The room a menu keeps from the window's edges.
    pub menu_margin: Pixels,
    pub ring_width: Pixels,
    /// Rings need a little blur to be drawn at all.
    pub ring_blur: Pixels,
    pub shadow_blur: Pixels,
    pub shadow_offset: Pixels,
    pub background: Hsla,
    pub card_background: Hsla,
    pub hover: Hsla,
    /// A borderless control's fill under the pointer, such as an icon
    /// button, a menu option or a section. It's see-through, so it shows
    /// on the card, the modal and a menu alike, in light and dark mode.
    pub hover_fill: Hsla,
    /// A control's fill while the pointer presses it, over whatever it
    /// sits on.
    pub pressed: Hsla,
    pub selected: Hsla,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_faint: Hsla,
    /// How faint a row gets while another setting keeps it from applying.
    pub inactive_opacity: f32,
    pub divider: Hsla,
    pub accent: Hsla,
    pub on_accent: Hsla,
    pub control_background: Hsla,
    /// The hairline ring that outlines buttons, keycaps and menus.
    pub control_ring: Hsla,
    pub toggle_off: Hsla,
    pub toggle_off_hover: Hsla,
    pub toggle_off_pressed: Hsla,
    /// The track a segmented control's chosen segment is raised out of.
    pub segment_track: Hsla,
    pub knob: Hsla,
    pub focus_ring: Hsla,
    pub warning: Hsla,
    /// The fill behind a control that asks once more before it removes.
    pub warning_fill: Hsla,
    pub shadow: Hsla,
    /// A primary button's fill under the pointer.
    pub accent_hover: Hsla,
    /// A primary button's or an on switch's fill while pressed.
    pub accent_pressed: Hsla,
    /// A clickable row's fill under the pointer; it reads on a card.
    pub card_hover: Hsla,
    /// The fill of the row a dragged toolbar item would land on.
    pub drop_fill: Hsla,
    /// The column a snippet's keys sit in, so the arrows after them line up.
    pub trigger_column_width: Pixels,
    /// The size a snippet's math result is rendered at.
    pub snippet_math_size: Pixels,
    /// The size the worked example above the snippets is drawn at.
    pub example_math_size: Pixels,
    /// The square a where-or-when mark on a snippet row takes.
    pub indicator_size: Pixels,
    /// How far a clickable row's hover plate sits inside the row.
    pub plate_inset: Pixels,
    /// The mark drawn where a snippet leaves the cursor to type into. It's
    /// outlined, so its corners stay square.
    pub slot_width: Pixels,
    pub slot_height: Pixels,
    pub slot_border: Pixels,
    pub slot_fill: Hsla,
}

impl Default for SettingsTheme {
    fn default() -> Self {
        Self::from_tokens(&Config::defaults().theme)
    }
}

impl SettingsTheme {
    /// Builds the tokens from a resolved config theme. Missing or
    /// malformed tokens fall back to the built-in value.
    pub fn from_tokens(tokens: &Tokens) -> Self {
        let read = TokenReader { tokens };
        let space = |name: &str, default: f32| px(read.number(name, default));
        let p = Palette::from_tokens(tokens);
        Self {
            font_family: read.text("font.ui", "Charter").into(),
            code_font_family: read.text("font.code", "Courier New").into(),
            text_size: px(15.),
            small_text_size: px(13.5),
            page_title_size: px(21.),
            strong_weight: FontWeight(read.number("font.weight.medium", 500.)),
            line_height_factor: read.number("font.line-height.ui", 1.3),
            modal_fraction: 0.8,
            modal_max_width: px(1080.),
            modal_max_height: px(780.),
            modal_radius: space("radius.lg", 10.) * 1.4,
            nav_width: px(252.),
            nav_padding: space("space.lg", 12.),
            nav_item_height: px(30.),
            nav_group_gap: space("space.xl", 16.),
            content_padding_x: space("space.xxl", 24.) * 1.5,
            content_padding_y: space("space.xxl", 24.),
            content_max_width: px(820.),
            card_gap: space("space.xxl", 24.),
            card_padding_x: space("space.xl", 16.) * 1.25,
            card_radius: space("radius.lg", 10.) * 1.2,
            row_padding_y: space("space.lg", 12.) * 1.25,
            list_row_padding_y: space("space.md", 8.),
            row_gap: space("space.xxl", 24.),
            text_min_width: px(140.),
            empty_dialog_width: px(440.),
            preview_height: px(168.),
            nav_fraction: 0.3,
            text_gap: space("space.xs", 2.),
            control_gap: space("space.md", 8.),
            gap_xs: space("space.xs", 2.),
            gap_sm: space("space.sm", 4.),
            radius: space("radius.md", 6.),
            icon_size: space("size.icon", 16.) * 1.125,
            small_icon_size: space("size.icon", 16.) * 0.875,
            hairline: px(1.),
            control_height: px(30.),
            control_padding_x: space("space.lg", 12.),
            toggle_width: px(40.),
            toggle_height: px(22.),
            toggle_knob_inset: space("space.xs", 2.),
            segment_inset: space("space.xs", 2.),
            swatch_size: px(22.),
            swatch_gap: space("space.xs", 2.),
            swatch_ring: space("space.xs", 2.),
            stepper_value_width: px(34.),
            field_width: px(220.),
            capture_field_width: px(168.),
            hex_field_width: px(92.),
            font_button_width: px(200.),
            menu_width: px(260.),
            menu_max_height: px(320.),
            menu_offset: space("space.sm", 4.),
            menu_margin: space("space.lg", 12.),
            ring_width: px(1.),
            ring_blur: px(0.5),
            shadow_blur: px(12.),
            shadow_offset: px(2.),
            background: p.popover,
            card_background: p.card,
            hover: p.hover,
            hover_fill: p.fill_strong,
            pressed: p.fill_pressed,
            selected: p.selection,
            text: p.text,
            text_muted: p.text_muted,
            text_faint: p.text_faint,
            inactive_opacity: 0.4,
            divider: p.divider,
            accent: p.accent,
            on_accent: p.on_accent,
            control_background: p.popover,
            control_ring: p.shadow,
            toggle_off: p.text_faint,
            toggle_off_hover: over(p.fill_pressed, p.text_faint),
            toggle_off_pressed: over(p.fill_pressed, over(p.fill_pressed, p.text_faint)),
            segment_track: p.fill_pressed,
            knob: p.knob,
            focus_ring: p.focus(),
            warning: p.conflict,
            warning_fill: p.conflict.opacity(0.12),
            shadow: p.shadow,
            accent_hover: over(p.accent.opacity(0.85), p.popover),
            accent_pressed: over(p.accent.opacity(0.7), p.popover),
            card_hover: p.fill_strong,
            drop_fill: p.drop_target,
            trigger_column_width: px(112.),
            snippet_math_size: px(18.),
            example_math_size: px(24.),
            indicator_size: px(24.),
            plate_inset: space("space.sm", 4.),
            slot_width: px(8.),
            slot_height: px(15.),
            slot_border: px(1.),
            slot_fill: p.accent.opacity(0.12),
        }
    }

    /// The hairline ring that outlines buttons, keycaps and menus.
    pub fn outline(&self) -> BoxShadow {
        BoxShadow {
            color: self.control_ring,
            offset: point(px(0.), px(0.)),
            blur_radius: self.ring_blur,
            spread_radius: self.ring_width,
        }
    }

    /// A soft drop shadow under raised controls.
    pub fn lift(&self) -> BoxShadow {
        BoxShadow {
            color: self.shadow,
            offset: point(px(0.), self.ring_width),
            blur_radius: self.ring_width * 2.,
            spread_radius: px(0.),
        }
    }

    /// A hairline ring of `color`, such as around the version of a
    /// conflict that's kept.
    pub fn ring(&self, color: Hsla) -> BoxShadow {
        BoxShadow {
            color,
            offset: point(px(0.), px(0.)),
            blur_radius: self.ring_blur,
            spread_radius: self.ring_width,
        }
    }

    /// The ring around whatever has keyboard focus.
    pub fn focus(&self) -> BoxShadow {
        focus_ring(self.focus_ring)
    }

    /// The shadow under the modal and menus.
    pub fn popover_shadow(&self) -> BoxShadow {
        BoxShadow {
            color: self.shadow,
            offset: point(px(0.), self.shadow_offset * 2.),
            blur_radius: self.shadow_blur * 2.,
            spread_radius: px(0.),
        }
    }
}

impl PickerTheme {
    /// The picker tokens in the workspace chrome's font and colours, so a
    /// picker, the vault search and the launcher read as one family with
    /// the menus and the file tree.
    pub fn from_ui(ui: &UiTheme) -> PickerTheme {
        PickerTheme {
            font_family: ui.font_family.clone(),
            width: ui.dialog_width,
            top_offset: ui.dialog_top_offset,
            row_height: ui.row_height,
            row_padding_x: ui.row_padding_x,
            row_corner_radius: ui.row_radius,
            list_padding: ui.dialog_padding,
            corner_radius: ui.dialog_radius,
            row_font_size: ui.font_size,
            detail_font_size: ui.small_font_size,
            icon_size: ui.icon_size - px(2.),
            keycap: ui.keycap.clone(),
            shadow_blur: ui.dialog_shadow_blur,
            shadow_offset_y: ui.dialog_shadow_offset,
            background: ui.menu_background,
            shadow: ui.dialog_shadow,
            text: ui.text,
            detail_text: ui.text_detail,
            match_text: ui.icon_active,
            icon: ui.icon,
            selected_row: ui.row_selected,
            hovered_row: ui.row_hover,
            warning_text: ui.error,
            ..PickerTheme::default()
        }
    }
}
