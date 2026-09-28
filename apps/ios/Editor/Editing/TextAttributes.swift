import UIKit

/// Attributes the styler leaves in the text storage for the display and
/// layout delegates to act on. The storage always holds the note's exact
/// source; these only change how it's drawn.
extension NSAttributedString.Key {
    /// Characters drawn as something else, of the same length.
    static let displaySubstitute = NSAttributedString.Key("editor.displaySubstitute")
    /// A line that takes no room, such as a table's delimiter row.
    static let collapsedLine = NSAttributedString.Key("editor.collapsedLine")
    /// What a line's layout fragment draws behind its text.
    static let blockDecoration = NSAttributedString.Key("editor.blockDecoration")
    /// A task's checkbox, holding where its `[` is.
    static let taskCheckbox = NSAttributedString.Key("editor.taskCheckbox")
}

/// What one or more characters are drawn as instead of themselves. Each is
/// its own object, so neighbouring substitutes never merge into one run.
final class DisplaySubstitute: NSObject {
    enum Content {
        /// Text of the same UTF-16 length as the characters it covers.
        case text(String)
        /// An SF Symbol in place of a single character.
        case symbol(name: String, color: UIColor)
    }

    let content: Content

    init(_ content: Content) {
        self.content = content
    }
}

/// A block's look behind its text, drawn by `BlockLayoutFragment`.
final class BlockDecoration: NSObject {
    enum Kind: Equatable {
        case code(first: Bool, last: Bool)
        case quote(depth: Int)
        case callout(first: Bool, last: Bool)
        case rule
        case tableRow(header: Bool)
    }

    let kind: Kind
    let color: UIColor

    init(_ kind: Kind, color: UIColor) {
        self.kind = kind
        self.color = color
    }
}
