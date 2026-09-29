import UIKit

/// Taps that act instead of placing the cursor: a task's checkbox toggles
/// it, and while the note is being read rather than edited, a link opens
/// where it goes.
extension EditingController: UIGestureRecognizerDelegate {
    func installTapHandling() {
        let tap = UITapGestureRecognizer(target: self, action: #selector(tapped(_:)))
        tap.delegate = self
        textView.addGestureRecognizer(tap)
    }

    func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
        let point = gestureRecognizer.location(in: textView)
        return taskMarker(at: point) != nil || linkTarget(at: point) != nil
    }

    func gestureRecognizer(
        _ gestureRecognizer: UIGestureRecognizer,
        shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer
    ) -> Bool {
        taskMarker(at: gestureRecognizer.location(in: textView)) != nil
    }

    @objc private func tapped(_ tap: UITapGestureRecognizer) {
        let point = tap.location(in: textView)
        if let marker = taskMarker(at: point) {
            toggleTask(at: marker)
        } else if let target = linkTarget(at: point) {
            host?.follow(link: target, from: self)
        }
    }

    private func toggleTask(at marker: Int) {
        let mark = NSRange(location: marker + 1, length: 1)
        guard NSMaxRange(mark) <= textView.textStorage.length else { return }
        let checked = (textView.text as NSString).substring(with: mark) != " "
        let keep = textView.selectedRange
        textView.selectedRange = mark
        insert(checked ? " " : "x")
        textView.selectedRange = keep
    }

    /// Where the `[` of the task whose checkbox is at `point` sits.
    private func taskMarker(at point: CGPoint) -> Int? {
        let storage = textView.textStorage
        return characterOffsets(at: point)
            .lazy
            .compactMap { storage.attribute(.taskCheckbox, at: $0, effectiveRange: nil) as? Int }
            .first
    }

    /// The link under `point`, while the keyboard is away.
    private func linkTarget(at point: CGPoint) -> String? {
        guard !textView.isFirstResponder else { return nil }
        return characterOffsets(at: point).first.flatMap { document.linkAt(offset: UInt32($0)) }
    }

    private func characterOffsets(at point: CGPoint) -> [Int] {
        guard let position = textView.closestPosition(to: point) else { return [] }
        let offset = textView.offset(from: textView.beginningOfDocument, to: position)
        let length = textView.textStorage.length
        return [offset, offset - 1].filter { $0 >= 0 && $0 < length }
    }
}
