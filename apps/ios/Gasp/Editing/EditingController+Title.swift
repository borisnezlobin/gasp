import UIKit

/// The note's name, large, above its text, as the Mac shows it when
/// `editor.show-inline-title` is on. It wraps onto as many lines as the name
/// needs and scrolls with the text. Editing it and pressing Done, or leaving
/// it, renames the note; Return never starts a new line, since a name has
/// only one.
extension EditingController {
    /// Whether the title shows, from the vault's settings.
    var showsTitle: Bool {
        !titleHidden && vault.showsInlineTitle()
    }

    func configureTitle() {
        titleView.isScrollEnabled = false
        titleView.textContainerInset = .zero
        titleView.textContainer.lineFragmentPadding = 0
        titleView.backgroundColor = .clear
        titleView.returnKeyType = .done
        titleView.autocapitalizationType = .sentences
        titleView.autocorrectionType = .no
        titleView.accessibilityLabel = "Note title"
        titleEditor.changed = { [weak self] in self?.titleHeightMayHaveChanged() }
        titleEditor.ended = { [weak self] in self?.titleEditingEnded() }
        titleView.delegate = titleEditor
        textView.addSubview(titleView)
        styleTitle()
        showTitle()
    }

    func styleTitle() {
        titleView.font = tokens.textFont(size: tokens.titleSize, bold: true)
        titleView.textColor = tokens.color(\.textStrong)
        titleView.tintColor = tokens.color(\.accent)
    }

    /// Puts the note's name in the title, unless it's being typed in.
    func showTitle() {
        guard !titleView.isFirstResponder else { return }
        titleView.text = Self.title(of: path)
        titleHeightMayHaveChanged()
    }

    /// Lays the title out across the column at `top` and answers the room
    /// it takes above the text, none when it's hidden.
    @discardableResult
    func placeTitle(top: CGFloat, side: CGFloat) -> CGFloat {
        titleView.isHidden = !showsTitle
        guard showsTitle else { return 0 }
        let width = max(textView.bounds.width - side * 2, 0)
        let height = titleView.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude)).height
        titleView.frame = CGRect(x: side, y: top, width: width, height: height)
        return height + CGFloat(tokens.spacing.md)
    }

    static func title(of path: String) -> String {
        ((path as NSString).lastPathComponent as NSString).deletingPathExtension
    }

    private func titleEditingEnded() {
        let typed = titleView.text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !typed.isEmpty, typed != Self.title(of: path) else {
            showTitle()
            return
        }
        host?.rename(self, to: typed)
        showTitle()
    }
}

/// The title's text view delegate: Return ends editing rather than
/// breaking the line, pasted line breaks become spaces, and every change
/// lets the note make room for the title's new height.
final class TitleEditor: NSObject, UITextViewDelegate {
    var changed: () -> Void = {}
    var ended: () -> Void = {}

    func textView(_ textView: UITextView, shouldChangeTextIn range: NSRange, replacementText text: String) -> Bool {
        if text == "\n" {
            textView.resignFirstResponder()
            return false
        }
        guard text.contains(where: \.isNewline) else { return true }
        let flattened = text.split(whereSeparator: \.isNewline).joined(separator: " ")
        textView.textStorage.replaceCharacters(in: range, with: flattened)
        textView.selectedRange = NSRange(location: range.location + (flattened as NSString).length, length: 0)
        changed()
        return false
    }

    func textViewDidChange(_ textView: UITextView) {
        changed()
    }

    func textViewDidEndEditing(_ textView: UITextView) {
        ended()
    }
}
