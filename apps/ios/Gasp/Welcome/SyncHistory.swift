import Foundation

/// The sync diagram's motion, as the Mac's tour draws it: a note leaves
/// one device, lands at the end of the repository's history as a new
/// commit, then travels on to the other device. The next note goes the
/// other way. Everything is worked out from the time alone.
enum SyncHistory {
    /// One trip: a note from one device to the repository and on.
    static let tripSeconds = 3.6
    /// The commits the repository's card shows.
    static let shownCommits = 5

    /// Where along a trip each part happens, as shares of it.
    private static let arrives = 0.34
    private static let committed = 0.5
    private static let delivered = 0.84

    enum Origin: Equatable {
        case phone
        case mac

        static func ofTrip(_ trip: Int) -> Origin {
            trip.isMultiple(of: 2) ? .phone : .mac
        }
    }

    /// Which line a travelling note is on.
    enum Line: Equatable {
        /// Between the iPhone and the repository.
        case phone
        /// Between the repository and the Mac.
        case mac
    }

    /// A note on its way, `along` from the iPhone's end of `line` (0) to
    /// the Mac's end (1).
    struct Travelling: Equatable {
        let line: Line
        let along: Double
        let origin: Origin
    }

    /// The diagram at one moment: the note on its way, if any, and the
    /// commits shown, oldest first. The newest fades in by `newestShown`
    /// while the row slides left by `slide` of a slot.
    struct Moment: Equatable {
        let travelling: Travelling?
        let commits: [Origin]
        let newestShown: Double
        let slide: Double
    }

    /// The diagram `seconds` after it started.
    static func moment(at seconds: Double) -> Moment {
        let trips = max(seconds, 0) / tripSeconds
        let trip = Int(trips.rounded(.down))
        let share = trips - Double(trip)
        let landed = share >= arrives
        let arriving = min(max((share - arrives) / (committed - arrives), 0), 1)
        let newest = landed ? trip + 1 : trip
        let commits = (0..<shownCommits).map { slot in
            Origin.ofTrip(newest - (shownCommits - slot))
        }
        return Moment(
            travelling: travelling(share: share, origin: .ofTrip(trip)),
            commits: commits,
            newestShown: landed ? arriving : 1,
            slide: landed ? 1 - arriving : 0
        )
    }

    private static func travelling(share: Double, origin: Origin) -> Travelling? {
        let leaving = share < arrives
        let delivering = share >= committed && share < delivered
        guard leaving || delivering else { return nil }
        let progress = leaving ? share / arrives : (share - committed) / (delivered - committed)
        let fromPhone = origin == .phone
        let line: Line = leaving == fromPhone ? .phone : .mac
        return Travelling(line: line, along: fromPhone ? progress : 1 - progress, origin: origin)
    }
}
