import UIKit

/// Widgets drawn in the text itself. The planner hides each widget's
/// source; these bring back the parts worth showing, draw a symbol in place
/// of a marker, or leave the drawing to the line's layout fragment.
extension LineStyler {
    /// Draws a widget that replaces its source. Answers whether it drew a
    /// picture, which may be taller than the line.
    func present(_ widget: Widget, _ paragraph: NSParagraphStyle) -> Bool {
        guard widget.placement == .replace else { return false }
        let range = widget.range.nsRange
        switch widget.kind {
        case .listBullet(let ordered, _, _): presentBullet(range, ordered: ordered, paragraph)
        case .checkbox(let checked): presentCheckbox(range, checked: checked, paragraph)
        case .footnoteSuperscript(let label): presentFootnote(range, label: label, paragraph)
        case .inlineMath(let tex, let display): return presentMath(range, tex: tex, display: display, paragraph)
        case .mathBlock(let tex): return presentMath(range, tex: tex, display: true, paragraph)
        case .image(let target, let alt, let width, let height, _):
            return presentImage(range, target: target, name: alt.isEmpty ? target : alt,
                                requested: (width, height), paragraph)
        case .codeBlock(let language, _, _): presentCodeFence(range, language: language, paragraph)
        case .calloutHeader(let kind, let typeName, let title, _, _):
            presentCalloutHeader(range, kind: kind, typeName: typeName, hasTitle: title != nil, paragraph)
        default: break
        }
        return false
    }

    private func markerLook() -> RunLook {
        var look = baseLook
        look.ink = \.textMuted
        return look
    }

    private func presentBullet(_ range: NSRange, ordered: Bool, _ paragraph: NSParagraphStyle) {
        let isTask = line.widgets.contains { if case .checkbox = $0.kind { true } else { false } }
        guard !isTask else { return }
        set(attributes(markerLook(), paragraph), on: range)
        if !ordered {
            substitute(NSRange(location: range.location, length: 1), with: .text("•"))
        }
    }

    private func presentCheckbox(_ range: NSRange, checked: Bool, _ paragraph: NSParagraphStyle) {
        let box = NSRange(location: range.location, length: 1)
        set(attributes(baseLook, paragraph), on: box)
        let symbol = checked ? "checkmark.square.fill" : "square"
        let color = tokens.color(checked ? \.accent : \.textMuted)
        substitute(box, with: .symbol(name: symbol, color: color))
        if let box = clamped(box) {
            storage.addAttribute(.taskCheckbox, value: range.location, range: box)
        }
        revealTrailingSpace(range, paragraph)
    }

    private func presentFootnote(_ range: NSRange, label: String, _ paragraph: NSParagraphStyle) {
        let found = text.range(of: label, options: [], range: range)
        guard found.location != NSNotFound else { return }
        let look = RunLook(styles: [.footnoteRef], base: baseLook, tokens: tokens)
        set(attributes(look, paragraph), on: found)
        storage.addAttribute(.footnoteLabel, value: label, range: found)
        revealTrailingSpace(range, paragraph)
    }

    /// Rendered math in place of its source, on the text's baseline. Until
    /// it's rendered it shows as its TeX in the math colour, without its
    /// dollar signs.
    private func presentMath(_ range: NSRange, tex: String, display: Bool, _ paragraph: NSParagraphStyle) -> Bool {
        let key = MathKey(tex: tex, display: display, fontSize: baseLook.size(tokens))
        guard let rendered = media?.math(key) else {
            presentMathSource(range, paragraph)
            return false
        }
        let attachment = MathAttachment(rendered, color: tokens.color(\.text))
        substitute(NSRange(location: range.location, length: 1), with: .attachment(attachment))
        return true
    }

    private func presentMathSource(_ range: NSRange, _ paragraph: NSParagraphStyle) {
        let source = text.substring(with: range)
        let delimiter = source.prefix { $0 == "$" }.count
        guard range.length > delimiter * 2 else { return }
        let tex = NSRange(location: range.location + delimiter, length: range.length - delimiter * 2)
        var look = baseLook
        look.italic = true
        look.ink = \.math
        set(attributes(look, paragraph), on: tex)
    }

    /// The image from the vault at the width of the text, or a symbol and
    /// its name when the file isn't there.
    private func presentImage(
        _ range: NSRange, target: String, name: String, requested: (UInt32?, UInt32?),
        _ paragraph: NSParagraphStyle
    ) -> Bool {
        guard let attachment = imageAttachment(target: target, requested: requested, paragraph) else {
            presentMissingImage(range, name: name, paragraph)
            return false
        }
        substitute(NSRange(location: range.location, length: 1), with: .attachment(attachment))
        return true
    }

    /// The picture for an image widget, sized to the column less the
    /// line's indent, and queued to decode at that size.
    func imageAttachment(target: String, requested: (UInt32?, UInt32?), _ paragraph: NSParagraphStyle)
        -> ImageAttachment? {
        guard let media, let file = media.imageFile(target),
              let pixels = VaultImages.shared.pixelSize(of: file) else { return nil }
        let width = media.columnWidth - paragraph.headIndent + min(paragraph.tailIndent, 0)
        let size = ImageDrawing.displaySize(pixels: pixels, requested: requested, columnWidth: width)
        let attachment = ImageAttachment(
            file: file, size: size, placeholderColor: tokens.color(\.fill),
            cornerRadius: CGFloat(tokens.spacing.radiusMd), media: media
        )
        media.wantImage(file, pixels: attachment.pixels)
        return attachment
    }

    private func presentMissingImage(_ range: NSRange, name: String, _ paragraph: NSParagraphStyle) {
        let icon = NSRange(location: range.location, length: 1)
        set(attributes(markerLook(), paragraph), on: icon)
        substitute(icon, with: .symbol(name: "photo", color: tokens.color(\.textMuted)))
        let found = text.range(of: name, options: [], range: range)
        guard found.location != NSNotFound, found.location > range.location else { return }
        var look = markerLook()
        look.sizeFactor = CGFloat(tokens.typography.smallScale)
        let gap = NSRange(location: found.location - 1, length: 1)
        set(attributes(look, paragraph), on: gap)
        substitute(gap, with: .text(" "))
        set(attributes(look, paragraph), on: found)
    }

    private func presentCodeFence(_ range: NSRange, language: String?, _ paragraph: NSParagraphStyle) {
        guard let language, !language.isEmpty else { return }
        let found = text.range(of: language, options: [], range: range)
        guard found.location != NSNotFound else { return }
        var look = markerLook()
        look.typeface = .text
        look.sizeFactor = CGFloat(tokens.typography.smallScale)
        set(attributes(look, paragraph), on: found)
    }

    private func presentCalloutHeader(
        _ range: NSRange, kind: String, typeName: String, hasTitle: Bool, _ paragraph: NSParagraphStyle
    ) {
        let icon = NSRange(location: range.location, length: 1)
        set(attributes(baseLook, paragraph), on: icon)
        substitute(icon, with: .symbol(name: CalloutSymbols.name(for: kind), color: tokens.calloutColor(kind)))
        if let icon = clamped(icon) { storage.addAttribute(.calloutFold, value: range.location, range: icon) }
        revealTrailingSpace(range, paragraph)
        guard !hasTitle else { return }
        let found = text.range(of: typeName, options: [], range: range)
        guard found.location != NSNotFound else { return }
        let look = RunLook(styles: [.calloutTitle], base: baseLook, tokens: tokens)
        set(attributes(look, paragraph), on: found)
    }

    private func revealTrailingSpace(_ range: NSRange, _ paragraph: NSParagraphStyle) {
        let last = NSMaxRange(range) - 1
        guard last > range.location, last < text.length, text.character(at: last) == 0x20 else { return }
        set(attributes(baseLook, paragraph), on: NSRange(location: last, length: 1))
    }
}

/// The SF Symbol for each callout kind, after the icons Obsidian draws.
enum CalloutSymbols {
    private static let names: [String: String] = [
        "note": "pencil",
        "abstract": "list.bullet.clipboard",
        "info": "info.circle",
        "todo": "checkmark.circle",
        "tip": "flame",
        "success": "checkmark",
        "question": "questionmark.circle",
        "warning": "exclamationmark.triangle",
        "failure": "xmark",
        "danger": "bolt",
        "bug": "ladybug",
        "example": "list.bullet",
        "quote": "quote.opening"
    ]

    static func name(for kind: String) -> String {
        names[kind] ?? "pencil"
    }
}
