//! What the print dialog lets you change: the choices PDF Export Plus
//! offered that the Typst export supports, and how each becomes
//! [`PdfOptions`].

use gasp_export::pdf::{Margins, PdfOptions};

/// Lines the drop cap spans when it's on, as in PDF Export Plus.
pub const DROP_CAP_LINES: u32 = 3;

/// A paper size the export can lay out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paper {
    A4,
    Letter,
    A5,
    A3,
    Legal,
    Tabloid,
}

impl Paper {
    pub const ALL: [Paper; 6] = [
        Paper::A4,
        Paper::Letter,
        Paper::A5,
        Paper::A3,
        Paper::Legal,
        Paper::Tabloid,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Paper::A4 => "A4",
            Paper::Letter => "Letter",
            Paper::A5 => "A5",
            Paper::A3 => "A3",
            Paper::Legal => "Legal",
            Paper::Tabloid => "Tabloid",
        }
    }

    /// The size, as the menu shows it beside the name.
    pub fn detail(self) -> &'static str {
        match self {
            Paper::A4 => "210 × 297 mm",
            Paper::Letter => "8.5 × 11 in",
            Paper::A5 => "148 × 210 mm",
            Paper::A3 => "297 × 420 mm",
            Paper::Legal => "8.5 × 14 in",
            Paper::Tabloid => "11 × 17 in",
        }
    }

    /// Height over width, for a page-shaped placeholder before the first
    /// page is laid out.
    pub fn aspect(self) -> f32 {
        match self {
            Paper::A4 | Paper::A5 | Paper::A3 => 297. / 210.,
            Paper::Letter => 11. / 8.5,
            Paper::Legal => 14. / 8.5,
            Paper::Tabloid => 17. / 11.,
        }
    }

    /// The name [`PdfOptions::page_size`] takes; an unknown one reads as
    /// A4, as the export does.
    pub fn from_page_size(name: &str) -> Paper {
        Paper::ALL
            .into_iter()
            .find(|paper| paper.label().eq_ignore_ascii_case(name))
            .unwrap_or(Paper::A4)
    }
}

/// How much room is left around the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarginSize {
    /// The owner's PDF Export Plus margins: 18, 16, 12 and 12 mm.
    Standard,
    Narrow,
    Wide,
}

impl MarginSize {
    pub const ALL: [MarginSize; 3] = [MarginSize::Standard, MarginSize::Narrow, MarginSize::Wide];

    pub fn label(self) -> &'static str {
        match self {
            MarginSize::Standard => "Standard",
            MarginSize::Narrow => "Narrow",
            MarginSize::Wide => "Wide",
        }
    }

    pub fn margins(self) -> Margins {
        let even = |mm| Margins {
            top: mm,
            bottom: mm,
            left: mm,
            right: mm,
        };
        match self {
            MarginSize::Standard => PdfOptions::default().margins,
            MarginSize::Narrow => even(8.),
            MarginSize::Wide => even(25.),
        }
    }

    /// Top, bottom, left and right, as the menu shows them.
    pub fn detail(self) -> String {
        let Margins {
            top,
            bottom,
            left,
            right,
        } = self.margins();
        if top == bottom && bottom == left && left == right {
            format!("{top} mm all round")
        } else {
            format!("{top}, {bottom}, {left}, {right} mm")
        }
    }
}

/// The font Gasp carries inside itself, so a PDF always has a body font
/// even where none of the others is installed.
pub const BUNDLED_FONT: &str = "Libertinus Serif";

/// The body fonts the dialog offers, print faces first. The menu lists
/// the ones this computer has, and always the bundled one.
pub const PRINT_FONTS: [&str; 9] = [
    "Iowan Old Style",
    "Charter",
    "Palatino",
    "Baskerville",
    "Georgia",
    "Times New Roman",
    "Helvetica Neue",
    "Avenir Next",
    BUNDLED_FONT,
];

/// Text sizes in points, with how the button shows each.
pub const TEXT_SIZES: [(u8, &str); 5] = [
    (10, "10 pt"),
    (11, "11 pt"),
    (12, "12 pt"),
    (13, "13 pt"),
    (14, "14 pt"),
];

/// The space between lines, as word processors name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineSpacing {
    Single,
    OneAndAHalf,
    Double,
}

impl LineSpacing {
    pub const ALL: [LineSpacing; 3] = [
        LineSpacing::Single,
        LineSpacing::OneAndAHalf,
        LineSpacing::Double,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LineSpacing::Single => "Single",
            LineSpacing::OneAndAHalf => "1.5 lines",
            LineSpacing::Double => "Double",
        }
    }

    /// Each line's height as a multiple of the text size.
    pub fn line_height(self) -> f64 {
        match self {
            LineSpacing::Single => 1.2,
            LineSpacing::OneAndAHalf => 1.5,
            LineSpacing::Double => 2.0,
        }
    }

    /// The choice nearest `line_height`.
    pub fn nearest(line_height: f64) -> LineSpacing {
        let distance = |spacing: &LineSpacing| (spacing.line_height() - line_height).abs();
        LineSpacing::ALL
            .into_iter()
            .min_by(|a, b| distance(a).total_cmp(&distance(b)))
            .unwrap_or(LineSpacing::Double)
    }
}

/// How far the first line of a paragraph that follows another paragraph
/// is pushed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Indent {
    None,
    Tab,
}

impl Indent {
    pub const ALL: [Indent; 2] = [Indent::None, Indent::Tab];

    pub fn label(self) -> &'static str {
        match self {
            Indent::None => "None",
            Indent::Tab => "Tab",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            Indent::None => "every line starts at the margin",
            Indent::Tab => "half an inch",
        }
    }

    pub fn inches(self) -> f64 {
        match self {
            Indent::None => 0.,
            Indent::Tab => 0.5,
        }
    }
}

/// One control in the dialog, in the order they're listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Paper,
    Margins,
    Font,
    TextSize,
    LineSpacing,
    Indent,
    PageNumbers,
    Title,
    DropCap,
}

impl Control {
    pub const ALL: [Control; 9] = [
        Control::Paper,
        Control::Margins,
        Control::Font,
        Control::TextSize,
        Control::LineSpacing,
        Control::Indent,
        Control::PageNumbers,
        Control::Title,
        Control::DropCap,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Control::Paper => "Paper",
            Control::Margins => "Margins",
            Control::Font => "Font",
            Control::TextSize => "Text size",
            Control::LineSpacing => "Line spacing",
            Control::Indent => "First-line indent",
            Control::PageNumbers => "Page numbers",
            Control::Title => "Note title at the top",
            Control::DropCap => "Drop cap",
        }
    }

    /// Whether the control is a switch rather than a list of choices.
    pub fn is_switch(self) -> bool {
        matches!(
            self,
            Control::PageNumbers | Control::Title | Control::DropCap
        )
    }

    /// A name for tests and element ids.
    pub fn key(self) -> &'static str {
        match self {
            Control::Paper => "paper",
            Control::Margins => "margins",
            Control::Font => "font",
            Control::TextSize => "text-size",
            Control::LineSpacing => "line-spacing",
            Control::Indent => "indent",
            Control::PageNumbers => "page-numbers",
            Control::Title => "title",
            Control::DropCap => "drop-cap",
        }
    }
}

/// The dialog's choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrintSettings {
    pub paper: Paper,
    pub margins: MarginSize,
    /// One of [`PRINT_FONTS`].
    pub font: &'static str,
    /// Points, one of [`TEXT_SIZES`].
    pub text_size: u8,
    pub line_spacing: LineSpacing,
    pub indent: Indent,
    pub page_numbers: bool,
    pub title: bool,
    pub drop_cap: bool,
}

impl Default for PrintSettings {
    /// The export's defaults, which are the owner's settings.
    fn default() -> Self {
        let options = PdfOptions::default();
        PrintSettings {
            paper: Paper::from_page_size(&options.page_size),
            margins: MarginSize::Standard,
            font: print_font(options.font_family.first().map_or("", String::as_str)),
            text_size: options.font_size_pt.round() as u8,
            line_spacing: LineSpacing::nearest(options.line_height),
            indent: if options.paragraph_indent_in > 0. {
                Indent::Tab
            } else {
                Indent::None
            },
            page_numbers: options.show_page_numbers,
            title: options.include_title,
            drop_cap: options.drop_cap_lines > 0,
        }
    }
}

/// The offered font named `family`, or the first one.
fn print_font(family: &str) -> &'static str {
    PRINT_FONTS
        .into_iter()
        .find(|font| font.eq_ignore_ascii_case(family))
        .unwrap_or(PRINT_FONTS[0])
}

/// How the button shows a text size.
fn size_label(points: u8) -> &'static str {
    TEXT_SIZES
        .iter()
        .find(|(size, _)| *size == points)
        .map_or("", |(_, label)| label)
}

/// The item after (or before) `current` in `all`, wrapping.
fn cycle<T: Copy + PartialEq>(all: &[T], current: T, forward: bool) -> T {
    let index = all.iter().position(|item| *item == current).unwrap_or(0);
    let count = all.len();
    let next = if forward {
        (index + 1) % count
    } else {
        (index + count - 1) % count
    };
    all[next]
}

impl PrintSettings {
    /// The export settings: the defaults with these choices.
    pub fn options(&self) -> PdfOptions {
        PdfOptions {
            page_size: self.paper.label().to_owned(),
            margins: self.margins.margins(),
            show_page_numbers: self.page_numbers,
            include_title: self.title,
            drop_cap_lines: if self.drop_cap { DROP_CAP_LINES } else { 0 },
            font_family: self.font_family(),
            font_size_pt: f64::from(self.text_size),
            line_height: self.line_spacing.line_height(),
            paragraph_indent_in: self.indent.inches(),
            ..PdfOptions::default()
        }
    }

    /// The chosen font, then the bundled one for any letters it lacks.
    fn font_family(&self) -> Vec<String> {
        let mut family = vec![self.font.to_owned()];
        if self.font != BUNDLED_FONT {
            family.push(BUNDLED_FONT.to_owned());
        }
        family
    }

    /// Whether `control`'s switch is on; false for a list.
    pub fn is_on(&self, control: Control) -> bool {
        match control {
            Control::PageNumbers => self.page_numbers,
            Control::Title => self.title,
            Control::DropCap => self.drop_cap,
            _ => false,
        }
    }

    /// The current choice of a list control, as its button shows it.
    pub fn choice_label(&self, control: Control) -> &'static str {
        match control {
            Control::Paper => self.paper.label(),
            Control::Margins => self.margins.label(),
            Control::Font => self.font,
            Control::TextSize => size_label(self.text_size),
            Control::LineSpacing => self.line_spacing.label(),
            Control::Indent => self.indent.label(),
            _ => "",
        }
    }

    /// The settings with `control` moved on one: a switch flips, a list
    /// takes its next (or previous) choice.
    pub fn stepped(mut self, control: Control, forward: bool) -> PrintSettings {
        match control {
            Control::Paper => self.paper = cycle(&Paper::ALL, self.paper, forward),
            Control::Margins => self.margins = cycle(&MarginSize::ALL, self.margins, forward),
            Control::Font => self.font = cycle(&PRINT_FONTS, self.font, forward),
            Control::TextSize => {
                self.text_size = cycle(&TEXT_SIZES.map(|(size, _)| size), self.text_size, forward)
            }
            Control::LineSpacing => {
                self.line_spacing = cycle(&LineSpacing::ALL, self.line_spacing, forward)
            }
            Control::Indent => self.indent = cycle(&Indent::ALL, self.indent, forward),
            Control::PageNumbers => self.page_numbers = !self.page_numbers,
            Control::Title => self.title = !self.title,
            Control::DropCap => self.drop_cap = !self.drop_cap,
        }
        self
    }
}

/// A menu line and the settings choosing it gives.
pub type Choice = (String, PrintSettings);

impl PrintSettings {
    /// The choices `control`'s menu lists, each as the whole settings it
    /// would give. `installed` names this computer's fonts, when known, so
    /// the font menu leaves out the ones it lacks.
    pub fn choices(&self, control: Control, installed: Option<&[String]>) -> Vec<Choice> {
        let current = *self;
        match control {
            Control::Paper => Paper::ALL
                .into_iter()
                .map(|paper| {
                    (
                        format!("{} ({})", paper.label(), paper.detail()),
                        PrintSettings { paper, ..current },
                    )
                })
                .collect(),
            Control::Margins => MarginSize::ALL
                .into_iter()
                .map(|margins| {
                    (
                        format!("{} ({})", margins.label(), margins.detail()),
                        PrintSettings { margins, ..current },
                    )
                })
                .collect(),
            Control::Font => font_choices(current, installed),
            Control::TextSize => TEXT_SIZES
                .into_iter()
                .map(|(text_size, label)| {
                    (
                        label.to_owned(),
                        PrintSettings {
                            text_size,
                            ..current
                        },
                    )
                })
                .collect(),
            Control::LineSpacing => LineSpacing::ALL
                .into_iter()
                .map(|line_spacing| {
                    (
                        line_spacing.label().to_owned(),
                        PrintSettings {
                            line_spacing,
                            ..current
                        },
                    )
                })
                .collect(),
            Control::Indent => Indent::ALL
                .into_iter()
                .map(|indent| {
                    (
                        format!("{} ({})", indent.label(), indent.detail()),
                        PrintSettings { indent, ..current },
                    )
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// The offered fonts this computer has, the bundled one always, and the
/// current one even if it's gone, so the menu still shows what's chosen.
fn font_choices(current: PrintSettings, installed: Option<&[String]>) -> Vec<Choice> {
    let has = |font: &str| {
        font == BUNDLED_FONT
            || font == current.font
            || installed
                .is_none_or(|names| names.iter().any(|name| name.eq_ignore_ascii_case(font)))
    };
    PRINT_FONTS
        .into_iter()
        .filter(|font| has(font))
        .map(|font| (font.to_owned(), PrintSettings { font, ..current }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_export_defaults() {
        assert_eq!(PrintSettings::default().options(), PdfOptions::default());
    }

    #[test]
    fn choices_become_export_options() {
        let settings = PrintSettings {
            paper: Paper::Letter,
            margins: MarginSize::Wide,
            font: "Charter",
            text_size: 11,
            line_spacing: LineSpacing::Single,
            indent: Indent::None,
            page_numbers: false,
            title: false,
            drop_cap: true,
        };
        let options = settings.options();
        assert_eq!(options.page_size, "Letter");
        assert_eq!(options.margins.left, 25.);
        assert!(!options.show_page_numbers && !options.include_title);
        assert_eq!(options.drop_cap_lines, DROP_CAP_LINES);
        assert_eq!(options.font_family, ["Charter", BUNDLED_FONT]);
        assert_eq!(options.font_size_pt, 11.);
        assert_eq!(options.line_height, 1.2);
        assert_eq!(options.paragraph_indent_in, 0.);
        assert_eq!(Paper::from_page_size("letter"), Paper::Letter);
        assert_eq!(Paper::from_page_size("B5"), Paper::A4);
    }

    #[test]
    fn stepping_cycles_lists_and_flips_switches() {
        let settings = PrintSettings::default();
        assert_eq!(settings.stepped(Control::Paper, true).paper, Paper::Letter);
        assert_eq!(
            settings.stepped(Control::Paper, false).paper,
            Paper::Tabloid
        );
        assert_eq!(
            settings.stepped(Control::Margins, false).margins,
            MarginSize::Wide
        );
        assert!(!settings.stepped(Control::PageNumbers, true).page_numbers);
        assert!(settings.stepped(Control::DropCap, false).drop_cap);
    }

    #[test]
    fn the_font_menu_lists_installed_fonts_and_the_bundled_one() {
        let installed = ["Charter".to_owned(), "Georgia".to_owned()];
        let fonts: Vec<String> = PrintSettings::default()
            .choices(Control::Font, Some(&installed))
            .into_iter()
            .map(|(label, _)| label)
            .collect();
        assert_eq!(
            fonts,
            ["Iowan Old Style", "Charter", "Georgia", BUNDLED_FONT]
        );
    }

    #[test]
    fn line_spacing_reads_the_nearest_choice() {
        assert_eq!(LineSpacing::nearest(2.0), LineSpacing::Double);
        assert_eq!(LineSpacing::nearest(1.4), LineSpacing::OneAndAHalf);
    }

    #[test]
    fn margins_read_as_millimetres() {
        assert_eq!(MarginSize::Standard.detail(), "18, 16, 12, 12 mm");
        assert_eq!(MarginSize::Narrow.detail(), "8 mm all round");
    }
}
