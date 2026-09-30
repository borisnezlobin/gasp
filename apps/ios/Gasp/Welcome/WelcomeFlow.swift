import Foundation
import Observation

/// One screen of the welcome tour.
enum WelcomeStep: Int, CaseIterable, Identifiable {
    case hello
    case writing
    case vault
    case syncHow
    case syncSetUp

    var id: Int { rawValue }

    /// Where the tour's caret sits along the sea, 0 at the first step and
    /// 1 at the last.
    var tide: Double {
        Double(rawValue) / Double(Self.allCases.count - 1)
    }
}

/// Whether the welcome tour is showing, and whether it's the first
/// launch's, which has no vault yet and so must end with one chosen, or a
/// replay from Settings, which can be closed at any step.
@Observable
final class WelcomeFlow {
    private(set) var isShowing: Bool
    private(set) var isFirstRun: Bool
    /// The step the tour opens on.
    private(set) var firstStep: WelcomeStep

    private static let seenKey = "welcome.seen"

    init(isFirstRun: Bool) {
        self.isFirstRun = isFirstRun
        isShowing = isFirstRun || Self.forcedByLaunchArgument
        firstStep = Self.stepFromLaunchArgument
    }

    /// Whether the tour opens at launch: the first time the app opens with
    /// no vault chosen, unless a performance probe or `-skipWelcome YES`
    /// is running it headless.
    static func isDue(vaultIsSetUp: Bool, arguments: UserDefaults = .standard) -> Bool {
        let seen = arguments.bool(forKey: seenKey)
        let skipped = arguments.bool(forKey: "skipWelcome") || arguments.string(forKey: "probe") != nil
        return !vaultIsSetUp && !seen && !skipped
    }

    /// `-welcome YES` shows the tour over the vault that's open, for
    /// screenshots.
    private static var forcedByLaunchArgument: Bool {
        UserDefaults.standard.bool(forKey: "welcome")
    }

    /// `-welcomeStep 3` opens the tour on its fourth step.
    private static var stepFromLaunchArgument: WelcomeStep {
        WelcomeStep(rawValue: UserDefaults.standard.integer(forKey: "welcomeStep")) ?? .hello
    }

    /// Shows the tour again from Settings.
    func replay() {
        isFirstRun = false
        firstStep = .hello
        isShowing = true
    }

    func finish() {
        UserDefaults.standard.set(true, forKey: Self.seenKey)
        isFirstRun = false
        isShowing = false
    }
}
