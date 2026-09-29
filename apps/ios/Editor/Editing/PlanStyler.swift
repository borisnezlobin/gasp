import UIKit

/// Applies the core's render plan to the text storage as attributes: fonts
/// and colours for styled runs, hidden markup, widgets and block
/// decorations. Only lines whose plan changed are restyled.
final class PlanStyler {
    private let tokens: Tokens
    private var appliedLines: [LinePlan] = []
    /// The text column's width, which decides whether a table fits as a grid.
    private var columnWidth: CGFloat = .greatestFiniteMagnitude

    init(tokens: Tokens) {
        self.tokens = tokens
    }

    /// A new column width changes how tables lay out, so the next plan
    /// restyles every line.
    func setColumnWidth(_ width: CGFloat) {
        columnWidth = width
        forgetApplied()
    }

    /// Makes the next plan restyle every line.
    func forgetApplied() {
        appliedLines = []
    }

    /// What newly typed text looks like until the next plan arrives.
    var typingAttributes: [NSAttributedString.Key: Any] {
        let look = RunLook(tokens: tokens)
        return look.attributes(tokens).merging([.paragraphStyle: paragraphStyle(LineShape([]), look)]) { $1 }
    }

    /// Restyles every line whose plan differs from the last one applied, or
    /// that overlaps `edited`, and tints the sentences on those lines.
    func apply(_ plan: NotePlan, tints: [SentenceTint], to storage: NSTextStorage, edited: NSRange?) {
        let text = storage.string as NSString
        let overrides = MathBlockFallback(plan: plan)
        var restyled = Set<Int>()
        for (index, line) in plan.lines.enumerated() where needsStyling(index, line, edited) {
            let paragraph = paragraphRange(plan, index, length: storage.length)
            let styler = LineStyler(tokens: tokens, storage: storage, text: text, line: line)
            styler.style(paragraph: paragraph, presentation: overrides.presentation(of: index))
            restyled.insert(index)
        }
        decorateBlocks(plan, restyled: restyled, storage: storage)
        TableLayout(tokens: tokens, storage: storage, columnWidth: columnWidth).layOut(plan, restyled: restyled)
        let restyledRanges = restyled.map { plan.lines[$0].range.nsRange }
        SentenceTinter(tokens: tokens, storage: storage).tint(tints, within: restyledRanges)
        appliedLines = plan.lines
    }

    private func needsStyling(_ index: Int, _ line: LinePlan, _ edited: NSRange?) -> Bool {
        guard index < appliedLines.count, appliedLines[index] == line else { return true }
        guard let edited else { return false }
        let range = line.range.nsRange
        return NSIntersectionRange(range, edited).length > 0 || NSLocationInRange(edited.location, range)
            || edited.location == NSMaxRange(range)
    }

    private func paragraphRange(_ plan: NotePlan, _ index: Int, length: Int) -> NSRange {
        let start = Int(plan.lines[index].range.start)
        let next = index + 1 < plan.lines.count ? Int(plan.lines[index + 1].range.start) : length
        let end = min(max(next, start), length)
        return NSRange(location: min(start, length), length: end - min(start, length))
    }

    /// Code blocks and callouts draw rounded ends, so a line's decoration
    /// depends on its neighbours; lines next to a restyled one are redone.
    private func decorateBlocks(_ plan: NotePlan, restyled: Set<Int>, storage: NSTextStorage) {
        let touched = Set(restyled.flatMap { [$0 - 1, $0, $0 + 1] })
        for index in touched.sorted() where plan.lines.indices.contains(index) {
            let paragraph = paragraphRange(plan, index, length: storage.length)
            guard paragraph.length > 0 else { continue }
            let neighbours = BlockNeighbours(plan: plan, index: index)
            if let decoration = neighbours.decoration(tokens: tokens) {
                storage.addAttribute(.blockDecoration, value: decoration, range: paragraph)
            } else {
                storage.removeAttribute(.blockDecoration, range: paragraph)
            }
        }
    }

    func paragraphStyle(_ shape: LineShape, _ look: RunLook) -> NSMutableParagraphStyle {
        LineStyler.paragraphStyle(shape, look, tokens: tokens)
    }
}

/// What the decorations on a line add up to.
struct LineShape {
    var headingLevel: UInt8?
    var codeBlockIndex: UInt32?
    var quoteDepth = 0
    var calloutKind: String?
    var alignment: NSTextAlignment = .natural
    var isTable = false
    var isHorizontalRule = false

    init(_ decorations: [LineDecoration]) {
        decorations.forEach { absorb($0) }
    }

    var isCode: Bool { codeBlockIndex != nil }

    private mutating func absorb(_ decoration: LineDecoration) {
        switch decoration {
        case .heading(let level): headingLevel = level
        case .quote(let depth): quoteDepth = max(quoteDepth, Int(depth))
        case .callout(let kind, let depth):
            calloutKind = kind
            quoteDepth = max(quoteDepth, Int(depth))
        case .codeBlock(let index): codeBlockIndex = index
        case .align(let alignment): self.alignment = alignment.nsTextAlignment
        case .table: isTable = true
        default: break
        }
    }
}

extension TextAlign {
    var nsTextAlignment: NSTextAlignment {
        switch self {
        case .natural: .natural
        case .left: .left
        case .center: .center
        case .right: .right
        }
    }
}

extension TextRange {
    init(_ range: NSRange) {
        self.init(start: UInt32(range.location), end: UInt32(NSMaxRange(range)))
    }

    var nsRange: NSRange {
        NSRange(location: Int(start), length: Int(end) - Int(start))
    }
}
