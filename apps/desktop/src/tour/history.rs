//! The sync diagram's motion: a note leaves one device, lands at the end
//! of the repository's history as a new commit, then travels on to the
//! other device. The next note goes the other way. Everything is worked
//! out from the time alone, so the diagram needs no state.

/// One trip: a note from one device to the repository and on.
pub const TRIP_SECONDS: f32 = 3.6;
/// The commits the repository's card shows.
pub const SHOWN_COMMITS: usize = 5;

/// Where along a trip each part happens, as shares of it.
const ARRIVES: f32 = 0.34;
const COMMITTED: f32 = 0.5;
const DELIVERED: f32 = 0.84;

/// Which device a note or commit came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Mac,
    Phone,
}

impl Origin {
    fn of_trip(trip: u64) -> Origin {
        if trip.is_multiple_of(2) {
            Origin::Mac
        } else {
            Origin::Phone
        }
    }
}

/// Which line a travelling note is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    /// Between the Mac and the repository.
    Mac,
    /// Between the repository and the iPhone.
    Phone,
}

/// A note on its way, `along` from the Mac's end of `line` (0) to the
/// other end (1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Travelling {
    pub line: Line,
    pub along: f32,
    pub origin: Origin,
}

/// The diagram at one moment.
#[derive(Clone, Debug, PartialEq)]
pub struct Moment {
    pub travelling: Option<Travelling>,
    /// The commits shown, oldest first, and how far the newest has faded
    /// in; the row slides left by `slide` of a slot as it arrives.
    pub commits: Vec<Origin>,
    pub newest_shown: f32,
    pub slide: f32,
}

/// The diagram `seconds` after it started.
pub fn moment(seconds: f32) -> Moment {
    let trips = seconds.max(0.) / TRIP_SECONDS;
    let trip = trips.floor() as u64;
    let share = trips.fract();
    let origin = Origin::of_trip(trip);
    let landed = share >= ARRIVES;
    let arriving = ((share - ARRIVES) / (COMMITTED - ARRIVES)).clamp(0., 1.);
    let newest = if landed { trip + 1 } else { trip };
    let commits = (0..SHOWN_COMMITS as i64)
        .map(|slot| newest as i64 - (SHOWN_COMMITS as i64 - 1 - slot))
        .map(|number| Origin::of_trip((number - 1).rem_euclid(2) as u64))
        .collect();
    Moment {
        travelling: travelling(share, origin),
        commits,
        newest_shown: if landed { arriving } else { 1. },
        slide: if landed { 1. - arriving } else { 0. },
    }
}

fn travelling(share: f32, origin: Origin) -> Option<Travelling> {
    let (line, progress) = if share < ARRIVES {
        (towards_repository(origin), share / ARRIVES)
    } else if (COMMITTED..DELIVERED).contains(&share) {
        (
            away_from_repository(origin),
            (share - COMMITTED) / (DELIVERED - COMMITTED),
        )
    } else {
        return None;
    };
    let along = match origin {
        Origin::Mac => progress,
        Origin::Phone => 1. - progress,
    };
    Some(Travelling {
        line,
        along,
        origin,
    })
}

fn towards_repository(origin: Origin) -> Line {
    match origin {
        Origin::Mac => Line::Mac,
        Origin::Phone => Line::Phone,
    }
}

fn away_from_repository(origin: Origin) -> Line {
    match origin {
        Origin::Mac => Line::Phone,
        Origin::Phone => Line::Mac,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_goes_to_the_repository_then_on_to_the_other_device() {
        let at = |share: f32| moment(TRIP_SECONDS * share).travelling;
        let leaving = at(0.1).unwrap();
        assert_eq!((leaving.line, leaving.origin), (Line::Mac, Origin::Mac));
        assert!(leaving.along < 0.5);
        assert_eq!(at(0.4), None, "it's becoming a commit");
        let delivering = at(0.7).unwrap();
        assert_eq!(delivering.line, Line::Phone);
        assert_eq!(at(0.95), None);
    }

    #[test]
    fn the_next_note_comes_from_the_phone() {
        let back = moment(TRIP_SECONDS * 1.1).travelling.unwrap();
        assert_eq!((back.line, back.origin), (Line::Phone, Origin::Phone));
        assert!(back.along > 0.5, "it starts at the phone's end");
    }

    #[test]
    fn each_trip_adds_a_commit_from_its_device() {
        let before = moment(TRIP_SECONDS * 0.2);
        let after = moment(TRIP_SECONDS * 0.6);
        assert_eq!(before.commits.len(), SHOWN_COMMITS);
        assert_eq!(after.commits[..SHOWN_COMMITS - 1], before.commits[1..]);
        assert_eq!(after.commits.last(), Some(&Origin::Mac));
        let next = moment(TRIP_SECONDS * 1.6);
        assert_eq!(next.commits.last(), Some(&Origin::Phone));
    }
}
