import UIKit

/// The note's name, large, above its text, as the Mac shows it when
/// `editor.show-inline-title` is on. It scrolls with the text; editing it
/// and pressing Done, or leaving it, renames the note.
extension EditingController: UITextFieldDelegate {
    /// Whether the title shows, from the vault's settings.
    var showsTitle: Bool {
        !titleHidden && vault.showsInlineTitle()
    }

    /// The room the title takes above the text.
    var titleRoom: CGFloat {
        guard showsTitle, let font = titleField.font else { return 0 }
        return font.lineHeight * 1.25 + CGFloat(tokens.spacing.md)
    }

    func configureTitle() {
        titleField.delegate = self
        titleField.returnKeyType = .done
        titleField.autocapitalizationType = .sentences
        titleField.autocorrectionType = .no
        titleField.adjustsFontSizeToFitWidth = false
        titleField.accessibilityLabel = "Note title"
        titleField.addTarget(self, action: #selector(titleEditingEnded), for: .editingDidEnd)
        textView.addSubview(titleField)
        styleTitle()
        showTitle()
    }

    func styleTitle() {
        titleField.font = tokens.textFont(size: tokens.titleSize, bold: true)
        titleField.textColor = tokens.color(\.textStrong)
        titleField.tintColor = tokens.color(\.accent)
    }

    /// Puts the note's name in the title, unless it's being typed in.
    func showTitle() {
        guard !titleField.isFirstResponder else { return }
        titleField.text = Self.title(of: path)
    }

    func placeTitle(top: CGFloat, side: CGFloat) {
        titleField.isHidden = !showsTitle
        let height = (titleField.font?.lineHeight ?? 0) * 1.25
        titleField.frame = CGRect(x: side, y: top, width: max(textView.bounds.width - side * 2, 0), height: height)
    }

    static func title(of path: String) -> String {
        ((path as NSString).lastPathComponent as NSString).deletingPathExtension
    }

    func textFieldShouldReturn(_ textField: UITextField) -> Bool {
        textField.resignFirstResponder()
        return false
    }

    @objc private func titleEditingEnded() {
        let typed = (titleField.text ?? "").trimmingCharacters(in: .whitespaces)
        guard !typed.isEmpty, typed != Self.title(of: path) else {
            showTitle()
            return
        }
        host?.rename(self, to: typed)
        showTitle()
    }
}
