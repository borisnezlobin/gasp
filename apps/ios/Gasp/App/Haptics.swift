import UIKit

/// The taps the phone gives back, each for one kind of moment, so the same
/// kind of thing always feels the same. They confirm something physical
/// happening, never typing or scrolling.
enum Haptics {
    /// A note or folder lifts off the file tree.
    static func pickedUp() {
        UIImpactFeedbackGenerator(style: .medium).impactOccurred()
    }

    /// A drag comes over somewhere it could land, a closed folder springs
    /// open, a tab swipe passes the point where it'll switch, or a number
    /// steps.
    static func tick() {
        UISelectionFeedbackGenerator().selectionChanged()
    }

    /// Something dragged has landed.
    static func dropped() {
        UINotificationFeedbackGenerator().notificationOccurred(.success)
    }

    /// A small thing changed under the finger: a checkbox, a fold, undo or
    /// redo.
    static func tap() {
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
    }

    /// Something that slides in from the side has arrived, such as the
    /// sidebar.
    static func arrived() {
        UIImpactFeedbackGenerator(style: .soft).impactOccurred()
    }

    /// The end of a range: a number at its limit, or swiping past the last
    /// tab onto the start page.
    static func limit() {
        UIImpactFeedbackGenerator(style: .rigid).impactOccurred()
    }

    /// Something needs the owner's attention, such as a sync conflict.
    static func warning() {
        UINotificationFeedbackGenerator().notificationOccurred(.warning)
    }
}
