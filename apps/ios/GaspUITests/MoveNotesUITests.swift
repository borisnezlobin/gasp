import XCTest

/// Moving notes by tapping a folder and by dragging in the file tree, on a
/// fresh sample vault the welcome tour makes.
final class MoveNotesUITests: XCTestCase {
    private let app = XCUIApplication()

    override func setUp() {
        continueAfterFailure = false
    }

    func testANoteMovesIntoANewFolderThenAnotherIsDraggedAfterIt() {
        startOnAFreshSampleVault()

        launch("-run", "note.move")
        app.buttons["New folder"].tap()
        let name = app.textFields["Folder name"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        name.typeText("Guides")
        app.buttons["Move"].tap()
        XCTAssertTrue(exists("gasp://open?path=Guides/Start%20here.md", title: "Start here"))

        launch("-run", "sidebar.files.show")
        let folder = app.buttons["Guides"]
        let note = app.buttons["Reading list"]
        XCTAssertTrue(folder.waitForExistence(timeout: 5))
        XCTAssertTrue(note.waitForExistence(timeout: 5))
        note.press(forDuration: 1.2, thenDragTo: folder)
        sleep(1)
        XCTAssertTrue(exists("gasp://open?path=Guides/Reading%20list.md", title: "Reading list"))
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

    /// Opens `link` and answers whether the note it names is the one showing.
    private func exists(_ link: String, title: String) -> Bool {
        launch("-open", link)
        return app.buttons[title].waitForExistence(timeout: 5)
    }
}
