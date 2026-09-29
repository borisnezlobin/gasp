import UIKit

/// Commands on the note: the core's edits applied as one undo step, and
/// the text view's own editing, find and clipboard.
extension EditingController {
    /// Runs `id` if it's one of the core's note commands. Answers whether
    /// it was.
    func runNoteCommand(_ id: String) -> CommandOutcome? {
        guard document.handles(id: id) else { return nil }
        let outcome = document.runCommand(id: id, selection: selection)
        apply(outcome)
        return outcome
    }

    /// Applies what a command came to. Edits go through the text view so
    /// they undo like typing.
    func apply(_ outcome: CommandOutcome) {
        switch outcome {
        case .edit(let replacements, let selection):
            replace(replacements, thenSelect: selection)
        case .select(let selection):
            placeCursor(at: Int(selection.start))
            textView.selectedRange = selection.nsRange
        case .copy(let text):
            UIPasteboard.general.string = text
        case .redraw:
            redrawAll()
        case .notice, .notANoteCommand:
            break
        }
    }

    private func replace(_ replacements: [TextReplacement], thenSelect selection: TextRange) {
        textView.undoManager?.beginUndoGrouping()
        for replacement in replacements.reversed() {
            guard let range = textRange(replacement.range.nsRange) else { continue }
            textView.replace(range, withText: replacement.text)
        }
        textView.undoManager?.endUndoGrouping()
        textView.selectedRange = selection.nsRange
        textViewDidChange(textView)
    }

    /// Types `text` over the selection, as pasting would.
    func insert(_ text: String) {
        guard let range = textView.selectedTextRange else { return }
        textView.replace(range, withText: text)
        textViewDidChange(textView)
    }

    private func textRange(_ range: NSRange) -> UITextRange? {
        let document = textView.beginningOfDocument
        guard let start = textView.position(from: document, offset: range.location),
              let end = textView.position(from: start, offset: range.length) else { return nil }
        return textView.textRange(from: start, to: end)
    }

    // MARK: The text view's own commands

    func undo() {
        textView.undoManager?.undo()
        textViewDidChange(textView)
    }

    func redo() {
        textView.undoManager?.redo()
        textViewDidChange(textView)
    }

    func showFind(replacing: Bool) {
        textView.becomeFirstResponder()
        textView.findInteraction?.presentFindNavigator(showingReplace: replacing)
    }

    func findNext() {
        guard let find = textView.findInteraction else { return }
        if !find.isFindNavigatorVisible { find.presentFindNavigator(showingReplace: false) }
        find.findNext()
    }

    func findPrevious() {
        guard let find = textView.findInteraction else { return }
        if !find.isFindNavigatorVisible { find.presentFindNavigator(showingReplace: false) }
        find.findPrevious()
    }

    /// Copies the selection, or the whole line with its break when nothing
    /// is selected, as the desktop does.
    func copySelectionOrLine() {
        UIPasteboard.general.string = (textView.text as NSString).substring(with: selectionOrLine())
    }

    func cutSelectionOrLine() {
        let range = selectionOrLine()
        UIPasteboard.general.string = (textView.text as NSString).substring(with: range)
        textView.selectedRange = range
        insert("")
    }

    func paste() {
        textView.paste(nil)
        textViewDidChange(textView)
    }

    /// Pastes the clipboard's text only, without links or styles.
    func pastePlain() {
        guard let text = UIPasteboard.general.string else { return }
        insert(text)
    }

    func selectAll() {
        textView.selectedRange = NSRange(location: 0, length: textView.textStorage.length)
    }

    /// The word under the cursor, or the selection, for Look up.
    func lookUpTerm() -> String? {
        let text = textView.text as NSString
        var range = textView.selectedRange
        if range.length == 0 {
            guard let position = textView.selectedTextRange?.start,
                  let word = textView.tokenizer.rangeEnclosingPosition(
                      position, with: .word, inDirection: .storage(.backward)
                  ) else { return nil }
            range = NSRange(
                location: textView.offset(from: textView.beginningOfDocument, to: word.start),
                length: textView.offset(from: word.start, to: word.end)
            )
        }
        let term = text.substring(with: range).trimmingCharacters(in: .whitespacesAndNewlines)
        return term.isEmpty ? nil : term
    }

    private func selectionOrLine() -> NSRange {
        let selected = textView.selectedRange
        guard selected.length == 0 else { return selected }
        return (textView.text as NSString).lineRange(for: selected)
    }
}
