import Foundation
import UIKit

/// The once-a-day usage ping: the app's version, the platform, iOS's
/// version and the chip type, sent to gaspmd.com so the owner can count
/// how many people use Gasp. Nothing else goes with it, and the vault's
/// `telemetry.enabled` setting turns it off. Debug builds, tests and
/// performance probes never send it.
enum UsagePing {
    static let url = URL(string: "https://gaspmd.com/api/ping")!
    static let settingKey = "telemetry.enabled"
    private static let lastPingKey = "usagePing.lastDay"
    private static let settleDelay: Duration = .seconds(10)
    private static let timeout: TimeInterval = 5

    struct Payload: Encodable, Equatable {
        let version: String
        let platform: String
        let os: String
        let arch: String

        static var thisDevice: Payload {
            Payload(
                version: Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0.0.0",
                platform: "ios",
                os: UIDevice.current.systemVersion,
                arch: chip
            )
        }

        private static var chip: String {
            #if arch(arm64)
            "arm64"
            #else
            "x86_64"
            #endif
        }
    }

    /// Whether a ping is due, given the day the last one went out.
    static func isDue(lastPing: String?, today: String) -> Bool {
        lastPing != today
    }

    static let utcCalendar: Calendar = {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC") ?? .gmt
        return calendar
    }()

    /// Today's date in UTC, such as `2026-09-30`: the day the server counts
    /// the ping under, so a day never gets two pings or none around midnight.
    static func today(_ now: Date = .now, calendar: Calendar = utcCalendar) -> String {
        let day = calendar.dateComponents([.year, .month, .day], from: now)
        return String(format: "%04d-%02d-%02d", day.year ?? 0, day.month ?? 0, day.day ?? 0)
    }

    /// Whether this launch may send one at all.
    static func runsInThisLaunch(
        arguments: UserDefaults = .standard,
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> Bool {
        #if DEBUG
        return false
        #else
        let probing = arguments.string(forKey: "probe") != nil
        let uiTesting = arguments.bool(forKey: "skipWelcome")
        let unitTesting = environment["XCTestConfigurationFilePath"] != nil
        return !probing && !uiTesting && !unitTesting
        #endif
    }

    /// Sends today's ping a little after the app comes to the front, when
    /// one is due and `isAllowed` still says yes by then.
    static func sendIfDue(isAllowed: @escaping @MainActor () -> Bool, defaults: UserDefaults = .standard) {
        guard runsInThisLaunch() else { return }
        Task { @MainActor in
            try? await Task.sleep(for: settleDelay)
            let today = today()
            guard isAllowed(), isDue(lastPing: defaults.string(forKey: lastPingKey), today: today) else { return }
            if await send(.thisDevice) {
                defaults.set(today, forKey: lastPingKey)
            }
        }
    }

    private static func send(_ payload: Payload) async -> Bool {
        var request = URLRequest(url: url, timeoutInterval: timeout)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try? JSONEncoder().encode(payload)
        guard let (_, response) = try? await URLSession.shared.data(for: request) else { return false }
        return (response as? HTTPURLResponse).map { (200..<300).contains($0.statusCode) } ?? false
    }
}
