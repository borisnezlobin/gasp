import XCTest

/// Types into a note in the running app and checks that it's drawn and
/// saved through the core.
final class EditingTests: XCTestCase {
    private let note = "editor://open?path=Lemma.md"
    private let typed = "Typed on the **phone** and saved."

    override func setUp() {
        continueAfterFailure = false
    }

    func testTypedTextIsStyledAndSaved() {
        let app = launch(at: note)
        let editor = app.textViews.firstMatch
        XCTAssertTrue(editor.waitForExistence(timeout: 10))

        editor.tap()
        editor.typeKey(.downArrow, modifierFlags: .command)
        editor.typeText("\n\n\(typed)")
        keepScreenshot(of: app, named: "Typing at the end of a note")

        app.navigationBars.buttons.element(boundBy: 0).tap()
        keepScreenshot(of: app, named: "Back at the note list")
        app.terminate()

        let reopened = launch(at: note)
        let saved = reopened.textViews.firstMatch
        XCTAssertTrue(saved.waitForExistence(timeout: 10))
        let text = saved.value as? String ?? ""
        XCTAssertTrue(text.hasSuffix(typed), "The note ends with: \(text.suffix(80))")
    }

    func testTheNoteListShowsTheSampleVault() {
        let app = XCUIApplication()
        app.launch()
        XCTAssertTrue(app.staticTexts["Entropy"].waitForExistence(timeout: 10))
        keepScreenshot(of: app, named: "Note list")
    }

    private func launch(at link: String) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-open", link]
        app.launch()
        return app
    }

    private func keepScreenshot(of app: XCUIApplication, named name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
