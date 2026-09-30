import XCTest
@testable import Gasp

final class UsagePingTests: XCTestCase {
    func testAPingIsDueOnceADay() {
        XCTAssertTrue(UsagePing.isDue(lastPing: nil, today: "2026-09-30"))
        XCTAssertTrue(UsagePing.isDue(lastPing: "2026-09-29", today: "2026-09-30"))
        XCTAssertFalse(UsagePing.isDue(lastPing: "2026-09-30", today: "2026-09-30"))
    }

    func testTodayIsTheLocalCalendarDate() throws {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = try XCTUnwrap(TimeZone(identifier: "America/Los_Angeles"))
        let lateEvening = Date(timeIntervalSince1970: 1_790_825_400)
        XCTAssertEqual(UsagePing.today(lateEvening, calendar: calendar), "2026-09-30")
    }

    func testThePayloadCarriesOnlyTheFourFields() throws {
        let payload = UsagePing.Payload(version: "0.1.0", platform: "ios", os: "26.0", arch: "arm64")
        let json = try JSONSerialization.jsonObject(with: JSONEncoder().encode(payload)) as? [String: String]
        XCTAssertEqual(json, ["version": "0.1.0", "platform": "ios", "os": "26.0", "arch": "arm64"])
        XCTAssertEqual(UsagePing.Payload.thisDevice.platform, "ios")
    }

    func testTestsNeverSendIt() {
        XCTAssertFalse(UsagePing.runsInThisLaunch())
    }
}
