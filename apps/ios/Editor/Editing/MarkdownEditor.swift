import SwiftUI
import UIKit

/// Shows a tab's editing session: a TextKit 2 text view drawing the core's
/// render plan, with markup hidden away from the cursor as on the desktop.
struct MarkdownEditor: UIViewRepresentable {
    let session: EditingController

    func makeUIView(context: Context) -> UIView {
        let container = UIView()
        container.backgroundColor = .clear
        embed(session.textView, in: container)
        return container
    }

    func updateUIView(_ container: UIView, context: Context) {
        guard session.textView.superview !== container else { return }
        container.subviews.forEach { $0.removeFromSuperview() }
        embed(session.textView, in: container)
    }

    private func embed(_ textView: UITextView, in container: UIView) {
        textView.removeFromSuperview()
        textView.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(textView)
        NSLayoutConstraint.activate([
            textView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            textView.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            textView.topAnchor.constraint(equalTo: container.topAnchor),
            textView.bottomAnchor.constraint(equalTo: container.bottomAnchor)
        ])
    }
}

/// A text view that says when its width changes, since tables and the
/// readable line length lay out against it, and when it has laid out, for
/// the table grids that sit over its text.
final class EditorTextView: UITextView {
    var widthDidChange: (() -> Void)?
    /// Runs after every layout pass, as the text scrolls or changes.
    var didLayout: (() -> Void)?
    /// The keymap's keys, answered here first while the note is edited.
    var boundKeys: [UIKeyCommand] = []
    var runBoundCommand: ((String) -> Void)?
    private var laidOutWidth: CGFloat = 0

    override var keyCommands: [UIKeyCommand]? {
        (super.keyCommands ?? []) + boundKeys
    }

    @objc func runBoundKey(_ command: UIKeyCommand) {
        guard let id = command.propertyList as? String else { return }
        runBoundCommand?(id)
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        didLayout?()
        guard bounds.width != laidOutWidth else { return }
        laidOutWidth = bounds.width
        widthDidChange?()
    }
}
