import UIKit

/// Taps that act instead of placing the cursor: a task's checkbox toggles
/// it, a heading's fold control or a callout's icon folds it, and an
/// underlined word shows the grammar checker's card. While the note is
/// being read rather than edited, a link opens where it goes and a
/// footnote shows its text.
extension EditingController: UIGestureRecognizerDelegate {
    /// What a tap at a point does.
    enum TapAction {
        case toggleTask(Int)
        case fold(Int)
        case showFlag(GrammarFlag, NSRange)
        case showFootnote(String, NSRange)
        case follow(String)
    }

    func installTapHandling() {
        let tap = UITapGestureRecognizer(target: self, action: #selector(tapped(_:)))
        tap.delegate = self
        textView.addGestureRecognizer(tap)
    }

    func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
        tapAction(at: gestureRecognizer.location(in: textView)) != nil
    }

    func gestureRecognizer(
        _ gestureRecognizer: UIGestureRecognizer,
        shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer
    ) -> Bool {
        switch tapAction(at: gestureRecognizer.location(in: textView)) {
        case .toggleTask, .showFlag: true
        default: false
        }
    }

    @objc private func tapped(_ tap: UITapGestureRecognizer) {
        switch tapAction(at: tap.location(in: textView)) {
        case .toggleTask(let marker): toggleTask(at: marker)
        case .fold(let offset): toggleFold(at: offset)
        case .showFlag(let flag, let range): showCard(for: flag, at: range)
        case .showFootnote(let label, let range): showFootnote(label, at: range)
        case .follow(let target): host?.follow(link: target, from: self)
        case nil: break
        }
    }

    func tapAction(at point: CGPoint) -> TapAction? {
        if let heading = foldControl(at: point) { return .fold(heading) }
        let offsets = characterOffsets(at: point)
        if let action = offsets.lazy.compactMap(markedAction).first { return action }
        guard !textView.isFirstResponder else { return nil }
        if let footnote = offsets.lazy.compactMap(footnoteAction).first { return footnote }
        return offsets.first.flatMap { document.linkAt(offset: UInt32($0)) }.map(TapAction.follow)
    }

    /// A checkbox, a callout's fold or a grammar flag at `offset`.
    private func markedAction(_ offset: Int) -> TapAction? {
        let storage = textView.textStorage
        if let marker = storage.attribute(.taskCheckbox, at: offset, effectiveRange: nil) as? Int {
            return .toggleTask(marker)
        }
        if let header = storage.attribute(.calloutFold, at: offset, effectiveRange: nil) as? Int,
           document.canFold(offset: UInt32(header)) {
            return .fold(header)
        }
        var range = NSRange()
        if let mark = storage.attribute(.grammarFlag, at: offset, effectiveRange: &range) as? GrammarMark {
            return .showFlag(mark.flag, range)
        }
        return nil
    }

    private func footnoteAction(_ offset: Int) -> TapAction? {
        var range = NSRange()
        let label = textView.textStorage.attribute(.footnoteLabel, at: offset, effectiveRange: &range) as? String
        return label.map { .showFootnote($0, range) }
    }

    /// The heading whose fold control, in the margin beside its first
    /// line, is under `point`.
    private func foldControl(at point: CGPoint) -> Int? {
        let margin = textView.textContainerInset.left
        guard point.x < margin + 4, point.x > margin - CGFloat(tokens.spacing.xl) * 2,
              let position = textView.closestPosition(to: CGPoint(x: margin + 1, y: point.y))
        else { return nil }
        let offset = textView.offset(from: textView.beginningOfDocument, to: position)
        guard offset < textView.textStorage.length,
              textView.textStorage.attribute(.headingFold, at: offset, effectiveRange: nil) != nil
        else { return nil }
        return offset
    }

    private func toggleTask(at marker: Int) {
        let mark = NSRange(location: marker + 1, length: 1)
        guard NSMaxRange(mark) <= textView.textStorage.length else { return }
        let checked = (textView.text as NSString).substring(with: mark) != " "
        let keep = textView.selectedRange
        textView.selectedRange = mark
        insert(checked ? " " : "x")
        textView.selectedRange = keep
        Haptics.tap()
    }

    private func characterOffsets(at point: CGPoint) -> [Int] {
        guard let position = textView.closestPosition(to: point) else { return [] }
        let offset = textView.offset(from: textView.beginningOfDocument, to: position)
        let length = textView.textStorage.length
        return [offset, offset - 1].filter { $0 >= 0 && $0 < length }
    }
}
