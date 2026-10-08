import XCTest

/// The welcome tour's practice note: it waits for a tap before raising the
/// keyboard, and its first task box ticks like any other.
final class WelcomeWritingUITests: XCTestCase {
    private let app = XCUIApplication()

    /// Where the practice note's first box draws on an iPhone 17 Pro, in
    /// points from the screen's top left.
    private let firstBox = CGVector(dx: 61, dy: 394)

    override func setUp() {
        continueAfterFailure = false
    }

    func testTheFirstBoxTicksWithoutRaisingTheKeyboard() {
        app.launchArguments = ["-skipWelcome", "YES", "-welcome", "YES", "-welcomeStep", "1"]
        app.launch()
        let note = app.textViews.firstMatch
        XCTAssertTrue(note.waitForExistence(timeout: 5))
        XCTAssertTrue(stays(app.keyboards.count == 0))

        app.coordinate(withNormalizedOffset: .zero).withOffset(firstBox).tap()

        XCTAssertTrue(becomes { (note.value as? String)?.contains("- [x] Tick this box") == true })
        XCTAssertEqual(app.keyboards.count, 0)
    }

    private func becomes(_ condition: @escaping () -> Bool) -> Bool {
        let met = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in condition() }, object: nil)
        return XCTWaiter.wait(for: [met], timeout: 3) == .completed
    }

    /// Whether `condition` still holds once the step has had time to settle.
    private func stays(_ condition: @autoclosure () -> Bool) -> Bool {
        Thread.sleep(forTimeInterval: 1)
        return condition()
    }
}
