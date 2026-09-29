import UIKit

/// Folding headings, and keeping where the reader was in the note on this
/// device: the cursor, the line at the top of the screen and the folded
/// headings, so the note opens the same way after a relaunch.
extension EditingController {
    // MARK: Folding

    /// Folds or unfolds the heading or callout at `offset`. Answers whether
    /// there was one.
    @discardableResult
    func toggleFold(at offset: Int) -> Bool {
        guard document.toggleFold(offset: UInt32(offset)) else { return false }
        let folds = document.headingFolds()
        if let heading = folds.first(where: { $0.range.nsRange.contains(offset) || Int($0.range.end) == offset }),
           heading.folded {
            keepCursorOutOf(heading, among: folds)
        }
        restyle(edited: nil)
        return true
    }

    /// A cursor in a section being folded would keep it open, so it moves
    /// to the end of the heading's line.
    private func keepCursorOutOf(_ heading: HeadingFold, among folds: [HeadingFold]) {
        let next = folds.first { $0.range.start > heading.range.start && $0.level <= heading.level }
        let end = next.map { Int($0.range.start) } ?? textView.textStorage.length
        let cursor = textView.selectedRange.location
        guard cursor > Int(heading.range.end), cursor <= end else { return }
        textView.selectedRange = NSRange(location: Int(heading.range.end), length: 0)
    }

    /// Folds the heading the cursor is in: its own line, or the nearest
    /// heading above it with something under it.
    func toggleFoldAtCursor() -> Bool {
        let cursor = UInt32(textView.selectedRange.location)
        let heading = document.headingFolds().last { $0.range.start <= cursor }
        if toggleFold(at: Int(cursor)) { return true }
        guard let heading else { return false }
        let folded = toggleFold(at: Int(heading.range.start))
        if folded {
            textView.selectedRange = NSRange(location: Int(heading.range.end), length: 0)
            textView.scrollRangeToVisible(textView.selectedRange)
        }
        return folded
    }

    func foldAllHeadings() {
        document.foldAllHeadings()
        let cursor = textView.selectedRange.location
        if let heading = document.headingFolds().last(where: { Int($0.range.start) <= cursor }) {
            textView.selectedRange = NSRange(location: Int(heading.range.end), length: 0)
        }
        restyle(edited: nil)
        textView.scrollRangeToVisible(textView.selectedRange)
    }

    func unfoldAllHeadings() {
        document.unfoldAllHeadings()
        restyle(edited: nil)
    }

    // MARK: Reading position

    /// Puts the cursor, the scroll and the folds back where this device
    /// left the note.
    func restorePosition() {
        guard let saved = vault.readingPosition(path: path, document: document) else { return }
        document.restoreFoldedHeadings(lines: saved.foldedLines)
        let length = textView.textStorage.length
        textView.selectedRange = NSRange(location: min(Int(saved.cursor), length), length: 0)
        pendingTop = min(Int(saved.top), length)
    }

    /// Scrolls the line the reader had at the top back to the top, once
    /// the text view has a size to lay out in.
    func scrollToPendingTop() {
        guard let top = pendingTop, textView.bounds.width > 0,
              let manager = textView.textLayoutManager,
              let storage = manager.textContentManager,
              let location = storage.location(storage.documentRange.location, offsetBy: top)
        else { return }
        pendingTop = nil
        let upToTop = NSTextRange(location: storage.documentRange.location, end: location)
        manager.ensureLayout(for: upToTop ?? storage.documentRange)
        guard let fragment = manager.textLayoutFragment(for: location) else { return }
        let insets = textView.adjustedContentInset
        let most = max(textView.contentSize.height - textView.bounds.height + insets.bottom, 0)
        let offset = min(fragment.layoutFragmentFrame.minY + textView.textContainerInset.top - insets.top, most)
        textView.setContentOffset(CGPoint(x: 0, y: max(offset, -insets.top)), animated: false)
    }

    /// Where the line at the top of the screen starts.
    private func topLineStart() -> Int {
        let point = CGPoint(x: textView.textContainerInset.left,
                            y: textView.contentOffset.y + textView.adjustedContentInset.top)
        guard let position = textView.closestPosition(to: point) else { return 0 }
        let offset = textView.offset(from: textView.beginningOfDocument, to: position)
        let text = textView.text as NSString
        return text.lineRange(for: NSRange(location: min(offset, text.length), length: 0)).location
    }

    func savePosition() {
        let position = ReadingPosition(
            cursor: UInt32(textView.selectedRange.location),
            top: UInt32(topLineStart()),
            foldedLines: document.foldedHeadingLines()
        )
        try? vault.saveReadingPosition(path: path, document: document, position: position)
    }
}
