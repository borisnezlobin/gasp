import SwiftUI
import UIKit

/// A TextKit 2 text view that draws the core's render plan: markup hides
/// away from the cursor and shows again around it, as on the desktop.
struct MarkdownEditor: UIViewRepresentable {
    let text: String
    let saver: NoteSaver
    let tokens: Tokens
    var cursorLine: Int?

    func makeCoordinator() -> EditingController {
        EditingController(text: text, saver: saver, tokens: tokens)
    }

    func makeUIView(context: Context) -> UITextView {
        if let cursorLine {
            context.coordinator.placeCursor(onLine: cursorLine)
        }
        return context.coordinator.textView
    }

    func updateUIView(_ textView: UITextView, context: Context) {}

    static func dismantleUIView(_ textView: UITextView, coordinator: EditingController) {
        coordinator.saveNow()
    }
}

/// Keeps the core's copy of the note in step with the text view, restyles
/// it from each new plan and saves edits.
final class EditingController: NSObject, UITextViewDelegate, UIGestureRecognizerDelegate {
    let textView = EditorTextView(usingTextLayoutManager: true)
    private let document: NoteDocument
    private let styler: PlanStyler
    private let saver: NoteSaver
    private let displayParagraphs = DisplayParagraphs()
    private let blockFragments: BlockFragments
    private var styledLength = 0
    private var isRestyling = false
    private var scrollsToCursorAfterLayout = false

    init(text: String, saver: NoteSaver, tokens: Tokens) {
        document = NoteDocument(text: text)
        styler = PlanStyler(tokens: tokens)
        blockFragments = BlockFragments(tokens: tokens)
        self.saver = saver
        super.init()
        configure(tokens)
        textView.text = text
        restyle(edited: nil)
        NotificationCenter.default.addObserver(
            self, selector: #selector(saveNow), name: UIApplication.didEnterBackgroundNotification, object: nil
        )
    }

    private func configure(_ tokens: Tokens) {
        textView.delegate = self
        textView.widthDidChange = { [weak self] in self?.columnWidthChanged() }
        textView.textLayoutManager?.delegate = blockFragments
        textView.textLayoutManager?.textContentManager?.delegate = displayParagraphs
        textView.backgroundColor = tokens.color(\.background)
        textView.tintColor = tokens.color(\.accent)
        textView.keyboardDismissMode = .interactive
        textView.alwaysBounceVertical = true
        textView.autocapitalizationType = .sentences
        textView.smartQuotesType = .no
        textView.smartDashesType = .no
        let spacing = tokens.spacing
        textView.textContainerInset = UIEdgeInsets(
            top: CGFloat(spacing.lg), left: CGFloat(spacing.xl),
            bottom: CGFloat(spacing.xxl), right: CGFloat(spacing.xl)
        )
        textView.textContainer.lineFragmentPadding = 0
        textView.typingAttributes = styler.typingAttributes
        let tap = UITapGestureRecognizer(target: self, action: #selector(toggleTappedTask(_:)))
        tap.delegate = self
        textView.addGestureRecognizer(tap)
    }

    /// Puts the cursor at the start of `line` (from 0), showing that line's
    /// markup, and scrolls to it without raising the keyboard.
    func placeCursor(onLine line: Int) {
        let text = textView.text as NSString
        var start = 0
        for _ in 0..<line {
            let rest = NSRange(location: start, length: text.length - start)
            let newline = text.range(of: "\n", options: [], range: rest)
            guard newline.location != NSNotFound else { break }
            start = NSMaxRange(newline)
        }
        textView.selectedRange = NSRange(location: start, length: 0)
        scrollsToCursorAfterLayout = true
    }

    private func columnWidthChanged() {
        let insets = textView.textContainerInset
        styler.setColumnWidth(textView.bounds.width - insets.left - insets.right)
        restyle(edited: nil)
        guard scrollsToCursorAfterLayout else { return }
        scrollsToCursorAfterLayout = false
        DispatchQueue.main.async { [textView] in
            textView.scrollRangeToVisible(textView.selectedRange)
        }
    }

    @objc func saveNow() {
        saver.flush()
    }

    func textViewDidChange(_ textView: UITextView) {
        guard textView.markedTextRange == nil else { return }
        let text = textView.text ?? ""
        document.update(text: text)
        restyle(edited: textView.selectedRange)
        saver.schedule(text)
    }

    func textViewDidChangeSelection(_ textView: UITextView) {
        guard !isRestyling, textView.markedTextRange == nil,
              textView.textStorage.length == styledLength else { return }
        restyle(edited: nil)
    }

    private func restyle(edited: NSRange?) {
        let storage = textView.textStorage
        let plan = document.plan(selection: TextRange(textView.selectedRange))
        isRestyling = true
        let selection = textView.selectedRange
        textView.textLayoutManager?.textContentManager?.performEditingTransaction {
            styler.apply(plan, to: storage, edited: edited)
        }
        if textView.selectedRange != selection { textView.selectedRange = selection }
        textView.typingAttributes = styler.typingAttributes
        styledLength = storage.length
        isRestyling = false
    }

    // MARK: Tasks

    func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
        taskMarker(at: gestureRecognizer.location(in: textView)) != nil
    }

    func gestureRecognizer(
        _ gestureRecognizer: UIGestureRecognizer,
        shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer
    ) -> Bool {
        true
    }

    @objc private func toggleTappedTask(_ tap: UITapGestureRecognizer) {
        guard let marker = taskMarker(at: tap.location(in: textView)) else { return }
        let mark = NSRange(location: marker + 1, length: 1)
        let storage = textView.textStorage
        guard NSMaxRange(mark) <= storage.length,
              let start = textView.position(from: textView.beginningOfDocument, offset: mark.location),
              let end = textView.position(from: start, offset: 1),
              let range = textView.textRange(from: start, to: end)
        else { return }
        let checked = (storage.string as NSString).substring(with: mark) != " "
        textView.replace(range, withText: checked ? " " : "x")
        textViewDidChange(textView)
    }

    /// Where the `[` of the task whose checkbox is at `point` sits.
    private func taskMarker(at point: CGPoint) -> Int? {
        guard let position = textView.closestPosition(to: point) else { return nil }
        let offset = textView.offset(from: textView.beginningOfDocument, to: position)
        let storage = textView.textStorage
        for candidate in [offset, offset - 1] where candidate >= 0 && candidate < storage.length {
            if let marker = storage.attribute(.taskCheckbox, at: candidate, effectiveRange: nil) as? Int {
                return marker
            }
        }
        return nil
    }
}

/// A text view that says when its width changes, since tables lay out
/// against it.
final class EditorTextView: UITextView {
    var widthDidChange: (() -> Void)?
    private var laidOutWidth: CGFloat = 0

    override func layoutSubviews() {
        super.layoutSubviews()
        guard bounds.width != laidOutWidth else { return }
        laidOutWidth = bounds.width
        widthDidChange?()
    }
}
