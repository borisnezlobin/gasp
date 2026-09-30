import XCTest
@testable import Gasp

final class SyncHistoryTests: XCTestCase {
    private func travelling(atShare share: Double) -> SyncHistory.Travelling? {
        SyncHistory.moment(at: SyncHistory.tripSeconds * share).travelling
    }

    func testANoteGoesToICloudThenOnToTheOtherDevice() throws {
        let leaving = try XCTUnwrap(travelling(atShare: 0.1))
        XCTAssertEqual(leaving.line, .phone)
        XCTAssertEqual(leaving.origin, .phone)
        XCTAssertLessThan(leaving.along, 0.5)
        XCTAssertNil(travelling(atShare: 0.4), "it's resting in iCloud")
        XCTAssertEqual(travelling(atShare: 0.7)?.line, .mac)
        XCTAssertNil(travelling(atShare: 0.95))
    }

    func testTheNextNoteComesFromTheMac() throws {
        let back = try XCTUnwrap(travelling(atShare: 1.1))
        XCTAssertEqual(back.line, .mac)
        XCTAssertEqual(back.origin, .mac)
        XCTAssertGreaterThan(back.along, 0.5, "it starts at the Mac's end")
    }

    func testEachNoteRestsInICloudBetweenItsTwoLegs() {
        let resting = SyncHistory.moment(at: SyncHistory.tripSeconds * 0.4)
        XCTAssertNil(resting.travelling)
        XCTAssertEqual(resting.resting, .phone)
        XCTAssertNil(SyncHistory.moment(at: SyncHistory.tripSeconds * 0.2).resting)
        XCTAssertEqual(SyncHistory.moment(at: SyncHistory.tripSeconds * 1.4).resting, .mac)
    }
}

final class WelcomeFlowTests: XCTestCase {
    private var arguments: UserDefaults!

    override func setUp() {
        arguments = UserDefaults(suiteName: "WelcomeFlowTests")
        arguments.removePersistentDomain(forName: "WelcomeFlowTests")
    }

    func testTheTourShowsOnlyBeforeAnyVaultIsChosen() {
        XCTAssertTrue(WelcomeFlow.isDue(vaultIsSetUp: false, arguments: arguments))
        XCTAssertFalse(WelcomeFlow.isDue(vaultIsSetUp: true, arguments: arguments))
    }

    func testTheTourShowsOnce() {
        arguments.set(true, forKey: "welcome.seen")
        XCTAssertFalse(WelcomeFlow.isDue(vaultIsSetUp: false, arguments: arguments))
    }

    func testProbesSkipTheTour() {
        arguments.set("launch", forKey: "probe")
        XCTAssertFalse(WelcomeFlow.isDue(vaultIsSetUp: false, arguments: arguments))
    }

    func testTheCaretMovesFromStartToEnd() {
        XCTAssertEqual(WelcomeStep.hello.tide, 0)
        XCTAssertEqual(WelcomeStep.sync.tide, 1)
    }
}
