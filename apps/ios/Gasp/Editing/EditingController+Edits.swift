import UIKit

/// Edits reach the core as they happen in the text view: each change to
/// the storage's characters, whatever made it, is gathered into one edit
/// that the core takes as a range and its replacement.
extension EditingController {
    /// Notes each change to the text's characters, however it came, for
    /// the core to hear about once the change is done.
    @objc func storageEdited(_ notification: Notification) {
        let storage = textView.textStorage
        guard storage.editedMask.contains(.editedCharacters) else { return }
        moveMarks(past: storage.editedRange, change: storage.changeInLength)
        if pendingEdit == nil {
            pendingEdit = PendingEdit(edited: storage.editedRange, change: storage.changeInLength)
        } else {
            pendingEdit?.join(edited: storage.editedRange, change: storage.changeInLength)
        }
    }

    /// Hands the core the text an edit replaced, or the whole text when
    /// the core's copy doesn't come out the same length as the view's.
    func sendToCore(_ edit: PendingEdit) {
        let storage = textView.textStorage
        let replacement = (storage.string as NSString).substring(with: edit.newRange)
        let length = document.replace(range: edit.oldRange, replacement: replacement)
        if Int(length) != storage.length { document.update(text: storage.string) }
    }

    /// Moves the grammar flags and code colours along with an edit,
    /// dropping the ones it touches until they're worked out again.
    private func moveMarks(past edited: NSRange, change: Int) {
        let replaced = NSRange(location: edited.location, length: edited.length - change)
        prose.flags = prose.flags.compactMap { flag in
            flag.range.following(replaced: replaced, change: change).map { range in
                var moved = flag
                moved.range = range
                return moved
            }
        }
        code.spans = code.spans.compactMap { span in
            span.range.following(replaced: replaced, change: change).map { range in
                var moved = span
                moved.range = range
                return moved
            }
        }
    }
}

/// The text view's edits since the core last heard, joined into one: the
/// text in `oldRange` became the text in `newRange`.
struct PendingEdit {
    private var location: Int
    private var oldLength: Int
    private var newLength: Int

    /// An edit as `NSTextStorage` reports it: `edited` is where the new
    /// text is, and `change` how much longer the text got.
    init(edited: NSRange, change: Int) {
        location = edited.location
        newLength = edited.length
        oldLength = edited.length - change
    }

    var oldRange: TextRange {
        TextRange(start: UInt32(location), end: UInt32(location + oldLength))
    }

    var newRange: NSRange {
        NSRange(location: location, length: newLength)
    }

    /// Takes in a later edit, reported the same way.
    mutating func join(edited: NSRange, change: Int) {
        let removed = edited.length - change
        let start = min(location, edited.location)
        let endBefore = max(location + newLength, edited.location + removed)
        oldLength += (location - start) + (endBefore - (location + newLength))
        newLength = endBefore - start + change
        location = start
    }
}

extension UITextView {
    /// Changes attributes of the text as one edit, so TextKit invalidates
    /// its layout once for all of them rather than once for each.
    func changeAttributes(_ change: () -> Void) {
        let storage = textStorage
        textLayoutManager?.textContentManager?.performEditingTransaction {
            storage.beginEditing()
            change()
            storage.endEditing()
        }
    }
}
