import XCTest

/// The software keyboard stays down while the sidebar is open.
final class SidebarKeyboardUITests: XCTestCase {
    private let app = XCUIApplication()

    override func setUp() {
        continueAfterFailure = false
    }

    /// `-editing` puts the cursor in the note, then `-run` opens the
    /// sidebar over it.
    func testOpeningTheSidebarWhileTypingPutsTheKeyboardAway() {
        startOnAFreshSampleVault()
        launch(
            "-open", "gasp://open?path=Reading%20list.md", "-editing", "YES",
            "-run", "sidebar.files.toggle"
        )

        XCTAssertTrue(app.textFields["Search notes"].waitForExistence(timeout: 5))
        XCTAssertTrue(keyboardGoesAway())
    }

    private func keyboardGoesAway() -> Bool {
        let gone = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in self.app.keyboards.count == 0 }, object: nil
        )
        return XCTWaiter.wait(for: [gone], timeout: 3) == .completed
    }

    /// The tour's "Try the sample vault" writes a new copy of the sample
    /// and opens it, whatever earlier runs left behind.
    private func startOnAFreshSampleVault() {
        launch("-welcome", "YES", "-welcomeStep", "2")
        let sample = app.buttons["Try the sample vault"]
        XCTAssertTrue(sample.waitForExistence(timeout: 5))
        sample.tap()
    }

    private func launch(_ arguments: String...) {
        app.terminate()
        app.launchArguments = ["-skipWelcome", "YES"] + arguments
        app.launch()
    }
}
