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
    /// Pictures a line draws in room it makes above or below itself.
    static let fragmentExtras = NSAttributedString.Key("editor.fragmentExtras")
    /// A heading line that folds, and whether it's folded.
    static let headingFold = NSAttributedString.Key("editor.headingFold")
    /// Text the grammar checker flagged.
    static let grammarFlag = NSAttributedString.Key("editor.grammarFlag")
    /// A footnote reference, holding its label.
    static let footnoteLabel = NSAttributedString.Key("editor.footnoteLabel")
    /// A row of a wide table, which its grid draws instead.
    static let gridRow = NSAttributedString.Key("editor.gridRow")
    /// A foldable callout's icon, holding where its header starts.
    static let calloutFold = NSAttributedString.Key("editor.calloutFold")
}

/// What one or more characters are drawn as instead of themselves. Each is
/// its own object, so neighbouring substitutes never merge into one run.
final class DisplaySubstitute: NSObject {
    enum Content {
        /// Text of the same UTF-16 length as the characters it covers.
        case text(String)
        /// An SF Symbol in place of a single character.
        case symbol(name: String, color: UIColor)
        /// A picture, such as rendered math, in place of a single character.
        case attachment(NSTextAttachment)
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

/// The pictures a line draws above and below itself.
final class FragmentExtras: NSObject {
    let above: FragmentExtra?
    let below: FragmentExtra?
    let trailing: FragmentExtra?
    /// Where the line's text starts, for pictures that aren't centred.
    let indent: CGFloat

    init(
        above: FragmentExtra? = nil, below: FragmentExtra? = nil, trailing: FragmentExtra? = nil, indent: CGFloat = 0
    ) {
        self.above = above
        self.below = below
        self.trailing = trailing
        self.indent = indent
    }
}

/// A heading that folds: its line's fold control shows which way it is.
final class HeadingFoldMark: NSObject {
    let folded: Bool

    init(folded: Bool) {
        self.folded = folded
    }
}

/// A grammar flag on the text it covers, in its underline's colour.
final class GrammarMark: NSObject {
    let flag: GrammarFlag
    let color: UIColor

    init(flag: GrammarFlag, color: UIColor) {
        self.flag = flag
        self.color = color
    }
}
