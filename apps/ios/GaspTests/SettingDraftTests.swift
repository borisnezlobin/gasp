import XCTest
@testable import Gasp

final class SettingDraftTests: XCTestCase {
    func testTheFieldShowsTheSavedValueUntilEditingStarts() {
        var draft = SettingDraft()
        XCTAssertEqual(draft.shown(saved: "master"), "master")
        draft.beginEditing(saved: "master")
        XCTAssertEqual(draft.shown(saved: "master"), "master")
    }

    func testARefreshWhileEditingKeepsWhatIsTyped() {
        var draft = SettingDraft()
        draft.beginEditing(saved: "master")
        draft.type("notes")
        XCTAssertEqual(draft.shown(saved: "master"), "notes", "sync refreshed the overview mid-edit")
        draft.beginEditing(saved: "master")
        XCTAssertEqual(draft.shown(saved: "master"), "notes")
    }

    func testLeavingTheFieldSavesTheTrimmedChange() {
        var draft = SettingDraft()
        draft.beginEditing(saved: "fake-default")
        draft.type("  main ")
        XCTAssertEqual(draft.endEditing(saved: "fake-default"), "main")
        XCTAssertFalse(draft.isEditing)
    }

    func testLeavingItUnchangedSavesNothing() {
        var draft = SettingDraft()
        draft.beginEditing(saved: "master")
        draft.type("master ")
        XCTAssertNil(draft.endEditing(saved: "master"))
    }

    func testItSavesOnceWhenFocusIsLostAndThenTheRowGoesAway() {
        var draft = SettingDraft()
        draft.beginEditing(saved: "master")
        draft.type("main")
        XCTAssertEqual(draft.endEditing(saved: "master"), "main")
        XCTAssertNil(draft.endEditing(saved: "master"))
    }

    func testClearingTheFieldSavesAnEmptyValue() {
        var draft = SettingDraft()
        draft.beginEditing(saved: "main")
        draft.type("")
        XCTAssertEqual(draft.endEditing(saved: "main"), "")
    }
}
