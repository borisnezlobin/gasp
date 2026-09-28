import UIKit

/// How a line is shown when the phone draws a widget differently from the
/// plan, such as a math block shown as its source until math renders here.
enum LinePresentation {
    case asPlanned
    case mathSource
    case collapsed
}

/// Styles one line of the plan into the text storage.
struct LineStyler {
    let tokens: Tokens
    let storage: NSTextStorage
    let text: NSString
    let line: LinePlan

    /// Hidden text is drawn this small and clear, so it takes no room while
    /// staying in the storage as the source.
    private static let hiddenSize: CGFloat = 0.01

    private var shape: LineShape { LineShape(line.decorations) }

    var baseLook: RunLook {
        var look = RunLook(tokens: tokens)
        let shape = shape
        look.headingLevel = shape.headingLevel
        look.typeface = shape.isCode ? .code : .text
        look.calloutKind = shape.calloutKind
        return look
    }

    func style(paragraph: NSRange, presentation: LinePresentation) {
        guard paragraph.length > 0, NSMaxRange(paragraph) <= storage.length else { return }
        let base = baseLook
        let paragraphStyle = Self.paragraphStyle(shape, base, tokens: tokens)
        storage.setAttributes(attributes(base, paragraphStyle), range: paragraph)
        for run in line.runs where !run.styles.isEmpty {
            let look = RunLook(styles: run.styles, base: base, tokens: tokens)
            set(attributes(look, paragraphStyle), on: run.range.nsRange)
        }
        switch presentation {
        case .asPlanned:
            line.hidden.forEach { hide($0.nsRange, paragraphStyle) }
            line.widgets.forEach { present($0, paragraphStyle) }
            hangListItem(paragraph, paragraphStyle)
            if line.collapsed { collapse(paragraph) }
        case .mathSource:
            showMathSource(paragraphStyle)
        case .collapsed:
            hide(line.range.nsRange, paragraphStyle)
            collapse(paragraph)
        }
    }

    static func paragraphStyle(_ shape: LineShape, _ look: RunLook, tokens: Tokens) -> NSMutableParagraphStyle {
        let style = NSMutableParagraphStyle()
        let size = look.size(tokens)
        let typography = tokens.typography
        let lineHeight = shape.headingLevel != nil
            ? typography.headingLineHeight
            : shape.isCode ? typography.codeLineHeight : typography.bodyLineHeight
        style.minimumLineHeight = size * CGFloat(lineHeight)
        style.maximumLineHeight = style.minimumLineHeight
        if shape.headingLevel != nil {
            style.paragraphSpacingBefore = size * CGFloat(typography.headingSpaceAbove)
        }
        let blockIndent = shape.isCode || shape.calloutKind != nil ? tokens.spacing.md : 0
        let indent = CGFloat(Double(shape.quoteDepth) * tokens.spacing.lg + blockIndent)
        style.firstLineHeadIndent = indent
        style.headIndent = indent
        style.tailIndent = -CGFloat(blockIndent)
        style.alignment = shape.alignment
        return style
    }

    func attributes(_ look: RunLook, _ paragraph: NSParagraphStyle) -> [NSAttributedString.Key: Any] {
        var attributes = look.attributes(tokens)
        attributes[.paragraphStyle] = paragraph
        return attributes
    }

    func set(_ attributes: [NSAttributedString.Key: Any], on range: NSRange) {
        guard let range = clamped(range) else { return }
        storage.setAttributes(attributes, range: range)
    }

    func hide(_ range: NSRange, _ paragraph: NSParagraphStyle) {
        set([
            .font: tokens.textFont(size: Self.hiddenSize),
            .foregroundColor: UIColor.clear,
            .paragraphStyle: paragraph
        ], on: range)
    }

    /// Draws `range`'s characters as `substitute` instead.
    func substitute(_ range: NSRange, with substitute: DisplaySubstitute.Content) {
        guard let range = clamped(range) else { return }
        storage.addAttribute(.displaySubstitute, value: DisplaySubstitute(substitute), range: range)
    }

    func clamped(_ range: NSRange) -> NSRange? {
        let clamped = NSIntersectionRange(range, NSRange(location: 0, length: storage.length))
        return clamped.length > 0 ? clamped : nil
    }

    /// Wrapped lines of a list item line up with its text, not its marker.
    private func hangListItem(_ paragraph: NSRange, _ style: NSParagraphStyle) {
        let markers = line.widgets.filter(\.isListMarker).map(\.range.end)
        guard let textStart = markers.max().map(Int.init), textStart > paragraph.location,
              let hanging = style.mutableCopy() as? NSMutableParagraphStyle
        else { return }
        let prefix = storage.attributedSubstring(from: NSRange(location: paragraph.location,
                                                               length: textStart - paragraph.location))
        hanging.headIndent = style.firstLineHeadIndent + ceil(prefix.size().width + checkboxWidening)
        storage.addAttribute(.paragraphStyle, value: hanging, range: paragraph)
    }

    /// How much wider a task's checkbox symbol draws than the `[` it
    /// stands in for.
    private var checkboxWidening: CGFloat {
        guard let box = line.widgets.first(where: { if case .checkbox = $0.kind { true } else { false } }),
              let font = storage.attribute(.font, at: Int(box.range.start), effectiveRange: nil) as? UIFont
        else { return 0 }
        let bracket = NSAttributedString(string: "[", attributes: [.font: font]).size().width
        return DisplayParagraphs.symbolImage("square", color: .black, font: font).size.width - bracket
    }

    private func collapse(_ paragraph: NSRange) {
        guard let range = clamped(paragraph) else { return }
        storage.addAttribute(.collapsedLine, value: true, range: range)
    }

    private func showMathSource(_ paragraph: NSMutableParagraphStyle) {
        let centered = paragraph.mutableCopy() as? NSMutableParagraphStyle ?? paragraph
        centered.alignment = .center
        var look = baseLook
        look.italic = true
        look.ink = \.math
        set(attributes(look, centered), on: line.range.nsRange)
    }
}

extension Widget {
    var isListMarker: Bool {
        switch kind {
        case .listBullet, .checkbox: true
        default: false
        }
    }
}
