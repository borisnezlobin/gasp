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

/// One control in the dialog, in the order they're listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Paper,
    Margins,
    PageNumbers,
    Title,
    DropCap,
}

impl Control {
    pub const ALL: [Control; 5] = [
        Control::Paper,
        Control::Margins,
        Control::PageNumbers,
        Control::Title,
        Control::DropCap,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Control::Paper => "Paper",
            Control::Margins => "Margins",
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
            page_numbers: options.show_page_numbers,
            title: options.include_title,
            drop_cap: options.drop_cap_lines > 0,
        }
    }
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
            ..PdfOptions::default()
        }
    }

    /// Whether `control`'s switch is on; false for a list.
    pub fn is_on(&self, control: Control) -> bool {
        match control {
            Control::PageNumbers => self.page_numbers,
            Control::Title => self.title,
            Control::DropCap => self.drop_cap,
            Control::Paper | Control::Margins => false,
        }
    }

    /// The current choice of a list control, as its button shows it.
    pub fn choice_label(&self, control: Control) -> &'static str {
        match control {
            Control::Paper => self.paper.label(),
            Control::Margins => self.margins.label(),
            _ => "",
        }
    }

    /// The settings with `control` moved on one: a switch flips, a list
    /// takes its next (or previous) choice.
    pub fn stepped(mut self, control: Control, forward: bool) -> PrintSettings {
        match control {
            Control::Paper => self.paper = cycle(&Paper::ALL, self.paper, forward),
            Control::Margins => self.margins = cycle(&MarginSize::ALL, self.margins, forward),
            Control::PageNumbers => self.page_numbers = !self.page_numbers,
            Control::Title => self.title = !self.title,
            Control::DropCap => self.drop_cap = !self.drop_cap,
        }
        self
    }
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
            page_numbers: false,
            title: false,
            drop_cap: true,
        };
        let options = settings.options();
        assert_eq!(options.page_size, "Letter");
        assert_eq!(options.margins.left, 25.);
        assert!(!options.show_page_numbers && !options.include_title);
        assert_eq!(options.drop_cap_lines, DROP_CAP_LINES);
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
    fn margins_read_as_millimetres() {
        assert_eq!(MarginSize::Standard.detail(), "18, 16, 12, 12 mm");
        assert_eq!(MarginSize::Narrow.detail(), "8 mm all round");
    }
}
