import UIKit

/// The grammar checker's underlines: the paragraphs around what's on
/// screen are checked a moment after typing or scrolling stops, off the
/// main thread, and the flags that changed are redrawn.
extension EditingController {
    /// How long typing or scrolling pauses before a check.
    private static let checkDelay: TimeInterval = 0.5
    /// Text checked beyond each edge of the screen, in UTF-16 units.
    private static let checkMargin = 3000

    func scheduleGrammarCheck() {
        guard grammar.isEnabled() else { return }
        grammarCheck?.cancel()
        let check = DispatchWorkItem { [weak self] in self?.checkGrammar() }
        grammarCheck = check
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.checkDelay, execute: check)
    }

    private func checkGrammar() {
        let visible = visibleCharacters()
        let length = textView.textStorage.length
        let start = max(visible.location - Self.checkMargin, 0)
        let end = min(NSMaxRange(visible) + Self.checkMargin, length)
        let within = TextRange(start: UInt32(start), end: UInt32(end))
        let (checker, document, version) = (grammar, document, editCount)
        GrammarService.queue.async { [weak self] in
            let flags = checker.check(document: document, within: within)
            DispatchQueue.main.async {
                guard let self, self.editCount == version else { return }
                self.show(flags: flags)
            }
        }
    }

    /// The text on screen.
    func visibleCharacters() -> NSRange {
        let bounds = textView.bounds
        let start = textView.closestPosition(to: CGPoint(x: bounds.minX, y: bounds.minY))
        let end = textView.closestPosition(to: CGPoint(x: bounds.maxX, y: bounds.maxY))
        guard let start, let end else { return NSRange(location: 0, length: 0) }
        let first = textView.offset(from: textView.beginningOfDocument, to: start)
        let last = textView.offset(from: textView.beginningOfDocument, to: end)
        return NSRange(location: min(first, last), length: abs(last - first))
    }

    /// Draws `flags` in place of the ones shown, restyling only the
    /// paragraphs whose underlines changed. The word being typed isn't
    /// flagged until the cursor moves on.
    private func show(flags found: [GrammarFlag]) {
        let cursor = textView.selectedRange
        let flags = found.filter { cursor.length > 0 || Int($0.range.end) != cursor.location }
        let old = Set(prose.flags.map(FlagIdentity.init))
        let new = Set(flags.map(FlagIdentity.init))
        guard old != new else { return }
        let changed = old.symmetricDifference(new).map(\.range)
        prose.flags = flags
        let storage = textView.textStorage
        let text = storage.string as NSString
        let whole = NSRange(location: 0, length: text.length)
        let paragraphs = changed.map { text.paragraphRange(for: NSIntersectionRange($0, whole)) }
        textView.textLayoutManager?.textContentManager?.performEditingTransaction {
            for paragraph in paragraphs where NSMaxRange(paragraph) <= storage.length {
                storage.removeAttribute(.grammarFlag, range: paragraph)
            }
            prose.mark(within: paragraphs, storage: storage)
        }
    }

    /// The flag under `offset`, if one is drawn there.
    func flag(at offset: Int) -> GrammarFlag? {
        guard offset < textView.textStorage.length else { return nil }
        let mark = textView.textStorage.attribute(.grammarFlag, at: offset, effectiveRange: nil) as? GrammarMark
        return mark?.flag
    }

    /// Replaces the flagged text with `replacement`, if it's still what
    /// was flagged, as one undo step.
    func accept(_ flag: GrammarFlag, replacement: String) {
        guard prose.flags.contains(where: { $0.range == flag.range }) else { return }
        let range = flag.range.nsRange
        guard NSMaxRange(range) <= textView.textStorage.length else { return }
        textView.selectedRange = range
        insert(replacement)
    }

    /// Stops flagging the flagged phrase anywhere in the vault.
    func ignore(_ flag: GrammarFlag) {
        let range = flag.range.nsRange
        guard NSMaxRange(range) <= textView.textStorage.length else { return }
        let phrase = (textView.text as NSString).substring(with: range)
        try? grammar.ignore(phrase: phrase)
        show(flags: prose.flags.filter {
            (textView.text as NSString).substring(with: $0.range.nsRange).lowercased() != phrase.lowercased()
        })
    }
}

/// What tells two flags apart when deciding what to redraw.
private struct FlagIdentity: Hashable {
    let start: UInt32
    let end: UInt32
    let message: String

    init(_ flag: GrammarFlag) {
        start = flag.range.start
        end = flag.range.end
        message = flag.message
    }

    var range: NSRange {
        NSRange(location: Int(start), length: Int(end) - Int(start))
    }
}
