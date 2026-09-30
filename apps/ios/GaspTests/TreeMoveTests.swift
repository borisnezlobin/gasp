import XCTest
@testable import Gasp

final class TreeMoveTests: XCTestCase {
    func testANoteMovesAnywhereButWhereItIs() {
        let note = TreeEntry.note("Reading/Lemma.md")
        XCTAssertTrue(note.canMove(into: ""))
        XCTAssertTrue(note.canMove(into: "Physics"))
        XCTAssertFalse(note.canMove(into: "Reading"))
    }

    func testAFolderNeverMovesIntoItselfOrItsOwnFolders() {
        let folder = TreeEntry.folder("Course Notes/Mechanics")
        XCTAssertTrue(folder.canMove(into: ""))
        XCTAssertTrue(folder.canMove(into: "Physics"))
        XCTAssertFalse(folder.canMove(into: "Course Notes"))
        XCTAssertFalse(folder.canMove(into: "Course Notes/Mechanics"))
        XCTAssertFalse(folder.canMove(into: "Course Notes/Mechanics/Waves"))
        XCTAssertTrue(folder.canMove(into: "Course Notes/Mechanics 2"))
    }

    func testNotesInAMovedFolderFollowIt() {
        let follow = { VaultPaths.path($0, afterMovingFolder: "Physics", to: "Science/Physics") }
        XCTAssertEqual(follow("Physics"), "Science/Physics")
        XCTAssertEqual(follow("Physics/Rigid Bodies.md"), "Science/Physics/Rigid Bodies.md")
        XCTAssertEqual(follow("Physics 2/Waves.md"), "Physics 2/Waves.md")
        XCTAssertEqual(follow("Lemma.md"), "Lemma.md")
    }

    func testTheOutlinePutsEachFolderUnderItsParent() {
        let rows = FolderOutline.rows(["Physics", "A B", "A/B", "A", "A/B/C", "Note 10", "Note 2"])
        XCTAssertEqual(rows.map(\.folder), ["A", "A/B", "A/B/C", "A B", "Note 2", "Note 10", "Physics"])
        XCTAssertEqual(rows.map(\.depth), [0, 1, 2, 0, 0, 0, 0])
        XCTAssertEqual(rows[2].name, "C")
    }
}

final class NumberLimitsTests: XCTestCase {
    func testTypedValuesAreKeptInRange() {
        let fontSize = NumberLimits.forSetting("appearance.base-font-size")
        XCTAssertEqual(fontSize.value(fromTyped: "2", wholeNumbers: true), 6)
        XCTAssertEqual(fontSize.value(fromTyped: "400", wholeNumbers: true), 72)
        XCTAssertEqual(fontSize.value(fromTyped: " 15 ", wholeNumbers: true), 15)
        XCTAssertEqual(fontSize.value(fromTyped: "14.6", wholeNumbers: true), 15)
        XCTAssertNil(fontSize.value(fromTyped: "", wholeNumbers: true))
        XCTAssertNil(fontSize.value(fromTyped: "twelve", wholeNumbers: true))
    }

    func testOtherNumbersStopAtZeroOrTheirFloor() {
        XCTAssertEqual(NumberLimits.forSetting("recovery.keep-days").value(fromTyped: "0", wholeNumbers: true), 1)
        XCTAssertEqual(NumberLimits.forSetting("some.count").value(fromTyped: "-3", wholeNumbers: true), 0)
        XCTAssertEqual(NumberLimits.forSetting("some.ratio").value(fromTyped: "1,5", wholeNumbers: false), 1.5)
    }
}
