import UIKit

/// Builds the paragraphs TextKit 2 lays out from the storage: characters
/// marked with a substitute are drawn as it, and collapsed lines are left
/// out. Substitutes keep each paragraph's length, so offsets in the view
/// stay offsets in the source.
final class DisplayParagraphs: NSObject, NSTextContentStorageDelegate {
    func textContentStorage(
        _ textContentStorage: NSTextContentStorage, textParagraphWith range: NSRange
    ) -> NSTextParagraph? {
        guard let storage = textContentStorage.textStorage, NSMaxRange(range) <= storage.length else { return nil }
        let source = storage.attributedSubstring(from: range)
        guard let display = Self.substituted(source) else { return nil }
        return NSTextParagraph(attributedString: display)
    }

    /// `source` as it's drawn, its substitutes in place.
    static func displayed(_ source: NSAttributedString) -> NSAttributedString {
        substituted(source) ?? source
    }

    private static func substituted(_ source: NSAttributedString) -> NSAttributedString? {
        var substitutes: [(NSRange, DisplaySubstitute)] = []
        let whole = NSRange(location: 0, length: source.length)
        source.enumerateAttribute(.displaySubstitute, in: whole) { value, run, _ in
            if let substitute = value as? DisplaySubstitute { substitutes.append((run, substitute)) }
        }
        guard !substitutes.isEmpty else { return nil }
        let display = NSMutableAttributedString(attributedString: source)
        for (run, substitute) in substitutes {
            let attributes = source.attributes(at: run.location, effectiveRange: nil)
            display.replaceCharacters(in: run, with: Self.drawn(substitute, length: run.length, attributes: attributes))
        }
        return display
    }

    func textContentManager(
        _ textContentManager: NSTextContentManager,
        shouldEnumerate textElement: NSTextElement,
        options: NSTextContentManager.EnumerationOptions = []
    ) -> Bool {
        // Read from the storage rather than the paragraph, which TextKit
        // may build afresh for every element it walks past.
        guard let storage = (textContentManager as? NSTextContentStorage)?.textStorage,
              let location = textElement.elementRange?.location else { return true }
        let offset = textContentManager.offset(from: textContentManager.documentRange.location, to: location)
        guard offset >= 0, offset < storage.length else { return true }
        return storage.attribute(.collapsedLine, at: offset, effectiveRange: nil) == nil
    }

    private static func drawn(
        _ substitute: DisplaySubstitute, length: Int, attributes: [NSAttributedString.Key: Any]
    ) -> NSAttributedString {
        switch substitute.content {
        case .text(let text):
            let padded = text.utf16.count == length ? text : String(repeating: " ", count: length)
            return NSAttributedString(string: padded, attributes: attributes)
        case .symbol(let name, let color):
            let symbol = NSMutableAttributedString(attachment: attachment(name, color: color, attributes: attributes))
            symbol.addAttributes(attributes, range: NSRange(location: 0, length: symbol.length))
            let padding = String(repeating: " ", count: max(length - 1, 0))
            let rest = NSAttributedString(string: padding, attributes: attributes)
            symbol.append(rest)
            return symbol
        case .attachment(let attachment):
            let picture = NSMutableAttributedString(attachment: attachment)
            picture.addAttributes(attributes, range: NSRange(location: 0, length: picture.length))
            let padding = String(repeating: " ", count: max(length - 1, 0))
            picture.append(NSAttributedString(string: padding, attributes: attributes))
            return picture
        }
    }

    static func symbolImage(_ name: String, color: UIColor, font: UIFont) -> UIImage {
        let configuration = UIImage.SymbolConfiguration(font: font)
        let image = UIImage(systemName: name, withConfiguration: configuration)
        return image?.withTintColor(color, renderingMode: .alwaysOriginal) ?? UIImage()
    }

    private static func attachment(
        _ name: String, color: UIColor, attributes: [NSAttributedString.Key: Any]
    ) -> NSTextAttachment {
        let font = attributes[.font] as? UIFont ?? .preferredFont(forTextStyle: .body)
        let image = symbolImage(name, color: color, font: font)
        let attachment = NSTextAttachment(image: image)
        let size = image.size
        attachment.bounds = CGRect(x: 0, y: (font.capHeight - size.height) / 2, width: size.width, height: size.height)
        return attachment
    }
}
