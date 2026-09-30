import Foundation

/// The sync diagram's motion: a note leaves one device, rests a moment in
/// iCloud, then travels on to the other device. The next note goes the
/// other way. Everything is worked out from the time alone.
enum SyncHistory {
    /// One trip: a note from one device to iCloud and on.
    static let tripSeconds = 3.6

    /// Where along a trip each part happens, as shares of it.
    private static let arrives = 0.34
    private static let leaves = 0.5
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
        /// Between the iPhone and iCloud.
        case phone
        /// Between iCloud and the Mac.
        case mac
    }

    /// A note on its way, `along` from the iPhone's end of `line` (0) to
    /// the Mac's end (1).
    struct Travelling: Equatable {
        let line: Line
        let along: Double
        let origin: Origin
    }

    /// The diagram at one moment: the note on its way, or the note resting
    /// in iCloud, `settled` from just arrived (0) to leaving (1).
    struct Moment: Equatable {
        let travelling: Travelling?
        let resting: Origin?
        let settled: Double
    }

    /// The diagram `seconds` after it started.
    static func moment(at seconds: Double) -> Moment {
        let trips = max(seconds, 0) / tripSeconds
        let trip = Int(trips.rounded(.down))
        let share = trips - Double(trip)
        let origin = Origin.ofTrip(trip)
        let isResting = share >= arrives && share < leaves
        return Moment(
            travelling: travelling(share: share, origin: origin),
            resting: isResting ? origin : nil,
            settled: isResting ? (share - arrives) / (leaves - arrives) : 0
        )
    }

    private static func travelling(share: Double, origin: Origin) -> Travelling? {
        let leaving = share < arrives
        let delivering = share >= leaves && share < delivered
        guard leaving || delivering else { return nil }
        let progress = leaving ? share / arrives : (share - leaves) / (delivered - leaves)
        let fromPhone = origin == .phone
        let line: Line = leaving == fromPhone ? .phone : .mac
        return Travelling(line: line, along: fromPhone ? progress : 1 - progress, origin: origin)
    }
}
