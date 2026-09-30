//! Which of a docked bar's items fit along it. The items that don't fit
//! go, in order from the end, into the bar's trailing More button. The
//! answer depends only on the items' lengths and the room the bar has, so
//! a bar drawn at the same width always shows the same items.

use gpui::{Pixels, px};

/// What one item takes along its bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Extent {
    /// A command or menu button, which More can offer instead.
    Button(Pixels),
    /// The thin line between groups.
    Separator(Pixels),
    /// A status widget, which More leaves out.
    Widget(Pixels),
    /// Room that grows to push what follows to the far end.
    Spacer,
}

impl Extent {
    pub fn length(self) -> Pixels {
        match self {
            Extent::Button(length) | Extent::Separator(length) | Extent::Widget(length) => length,
            Extent::Spacer => Pixels::ZERO,
        }
    }

    /// Whether a bar can end on it just before its More button.
    fn can_end_a_run(self) -> bool {
        matches!(self, Extent::Button(_) | Extent::Widget(_))
    }
}

/// How many of a bar's items show, from its start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fit {
    pub shown: usize,
    /// Whether the rest need the More button, which is when a command or
    /// menu is among them.
    pub overflows: bool,
}

/// Layout rounds to whole pixels, so a bar exactly as long as its room can
/// come out a fraction longer; that fraction never overflows it.
const ROUNDING: Pixels = px(0.5);

/// How long `items` are laid end to end, `gap` apart.
pub fn run_length(items: &[Extent], gap: Pixels) -> Pixels {
    let lengths = items
        .iter()
        .fold(Pixels::ZERO, |total, item| total + item.length());
    let gaps = gap * items.len().saturating_sub(1) as f32;
    lengths + gaps
}

/// How many leading items of `items` show in `room`, leaving `more` (and
/// a gap before it) for the More button when they don't all fit. A run
/// never ends on a separator or a spacer.
pub fn fit_items(items: &[Extent], gap: Pixels, room: Pixels, more: Pixels) -> Fit {
    if run_length(items, gap) <= room + ROUNDING {
        return Fit {
            shown: items.len(),
            overflows: false,
        };
    }
    let shown = (0..items.len())
        .rev()
        .map(|count| trimmed(&items[..count]))
        .find(|run| run_with_more(run, gap, more) <= room + ROUNDING)
        .map_or(0, <[Extent]>::len);
    Fit {
        shown,
        overflows: items[shown..]
            .iter()
            .any(|item| matches!(item, Extent::Button(_))),
    }
}

/// `items` without the separators and spacers at their end.
fn trimmed(items: &[Extent]) -> &[Extent] {
    let end = items
        .iter()
        .rposition(|item| item.can_end_a_run())
        .map_or(0, |last| last + 1);
    &items[..end]
}

fn run_with_more(run: &[Extent], gap: Pixels, more: Pixels) -> Pixels {
    if run.is_empty() {
        return more;
    }
    run_length(run, gap) + gap + more
}

/// What each spacer grows by to fill `room`, once every item shows.
pub fn spacer_length(items: &[Extent], gap: Pixels, room: Pixels) -> Pixels {
    let spacers = items
        .iter()
        .filter(|item| matches!(item, Extent::Spacer))
        .count();
    if spacers == 0 {
        return Pixels::ZERO;
    }
    ((room - run_length(items, gap)) / spacers as f32).max(Pixels::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn button() -> Extent {
        Extent::Button(px(30.))
    }

    const GAP: Pixels = px(4.);
    const MORE: Pixels = px(30.);

    #[test]
    fn everything_shows_when_it_fits() {
        let items = [button(), button(), Extent::Spacer, button()];
        assert_eq!(run_length(&items, GAP), px(102.));
        let fit = fit_items(&items, GAP, px(102.), MORE);
        assert_eq!(
            fit,
            Fit {
                shown: 4,
                overflows: false
            }
        );
        assert_eq!(spacer_length(&items, GAP, px(202.)), px(100.));
    }

    #[test]
    fn items_that_do_not_fit_leave_for_the_more_button_from_the_end() {
        let items = [button(), button(), button(), button()];
        // Two buttons, a gap and More take 30 + 4 + 30 + 4 + 30 = 98.
        let fit = fit_items(&items, GAP, px(100.), MORE);
        assert_eq!(
            fit,
            Fit {
                shown: 2,
                overflows: true
            }
        );
        assert_eq!(fit_items(&items, GAP, px(97.), MORE).shown, 1);
    }

    #[test]
    fn a_run_never_ends_on_a_separator_or_a_spacer() {
        let items = [
            button(),
            Extent::Separator(px(1.)),
            Extent::Spacer,
            button(),
            button(),
        ];
        let fit = fit_items(&items, GAP, px(80.), MORE);
        assert_eq!(fit.shown, 1, "the separator and spacer go with the rest");
        assert!(fit.overflows);
    }

    #[test]
    fn a_bar_too_short_for_anything_keeps_only_more() {
        let items = [button(), button()];
        let fit = fit_items(&items, GAP, px(40.), MORE);
        assert_eq!(
            fit,
            Fit {
                shown: 0,
                overflows: true
            }
        );
    }

    #[test]
    fn widgets_alone_past_the_end_need_no_more_button() {
        let items = [button(), Extent::Widget(px(80.))];
        let fit = fit_items(&items, GAP, px(70.), MORE);
        assert_eq!(fit.shown, 1);
        assert!(!fit.overflows);
    }

    #[test]
    fn the_same_room_always_fits_the_same_items() {
        let items = [button(), Extent::Widget(px(57.3)), button(), button()];
        let fits: Vec<Fit> = (0..3)
            .map(|_| fit_items(&items, GAP, px(131.), MORE))
            .collect();
        assert!(fits.windows(2).all(|pair| pair[0] == pair[1]));
    }
}
