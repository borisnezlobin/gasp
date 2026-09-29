import UIKit

/// Applies the core's render plan to the text storage as attributes: fonts
/// and colours for styled runs, hidden markup, widgets and block
/// decorations. Only lines whose plan changed are restyled.
final class PlanStyler {
    private let tokens: Tokens
    private var appliedLines: [LinePlan] = []
    private var appliedFolds: [UInt32: Bool] = [:]
    /// Lines to restyle next time whatever their plan says.
    private var forcedLines = IndexSet()
    /// The text column's width, which decides whether a table fits as a grid.
    private var columnWidth: CGFloat = .greatestFiniteMagnitude
    /// Rendered math and the vault's images for the note.
    var media: NoteMedia?
    /// Headings that fold, by line, and whether each is folded.
    var headingFolds: [UInt32: Bool] = [:]
    /// Wide tables drawn as a scrolling grid, by where each starts.
    private(set) var gridTables: [UInt32: TableGridModel] = [:]

    init(tokens: Tokens) {
        self.tokens = tokens
    }

    /// A new column width changes how tables lay out, so the next plan
    /// restyles every line.
    func setColumnWidth(_ width: CGFloat) {
        columnWidth = width
        media?.columnWidth = width
        forgetApplied()
    }

    /// Makes the next plan restyle every line.
    func forgetApplied() {
        appliedLines = []
    }

    /// Makes the next plan restyle the lines `matching` says, and every
    /// line their widgets reach, such as a `$$` block's.
    func forgetLines(matching: (LinePlan) -> Bool) {
        let reached = appliedLines.filter(matching).flatMap { $0.widgets.map(\.range.nsRange) }
        for (index, line) in appliedLines.enumerated() {
            let range = line.range.nsRange
            let touched = reached.contains { widget in
                NSIntersectionRange(widget, range).length > 0 || NSLocationInRange(range.location, widget)
            }
            if touched || matching(line) { forcedLines.insert(index) }
        }
    }

    /// What newly typed text looks like until the next plan arrives.
    var typingAttributes: [NSAttributedString.Key: Any] {
        let look = RunLook(tokens: tokens)
        return look.attributes(tokens).merging([.paragraphStyle: paragraphStyle(LineShape([]), look)]) { $1 }
    }

    /// Restyles every line whose plan differs from the last one applied, or
    /// that overlaps `edited`, and tints the sentences and marks the
    /// grammar flags on those lines.
    func apply(_ plan: NotePlan, prose: ProseMarks, to storage: NSTextStorage, edited: NSRange?) {
        let text = storage.string as NSString
        let fallbacks = BlockFallbacks(plan: plan, text: text) { [media, tokens] tex in
            media?.math(MathKey(tex: tex, display: true, fontSize: tokens.bodySize)) != nil
        }
        let restyled = withWholeTables(
            Set(plan.lines.indices.filter { needsStyling($0, plan.lines[$0], edited) }), plan: plan
        )
        for index in restyled.sorted() {
            let line = plan.lines[index]
            let paragraph = paragraphRange(plan, index, length: storage.length)
            var styler = LineStyler(tokens: tokens, storage: storage, text: text, line: line)
            styler.media = media
            styler.fold = headingFolds[line.line]
            styler.style(paragraph: paragraph, presentation: fallbacks.presentation(of: index))
        }
        decorateBlocks(plan, fallbacks: fallbacks, restyled: restyled, storage: storage)
        let tables = TableLayout(tokens: tokens, storage: storage, columnWidth: columnWidth)
        keepGrids(tables.layOut(plan, restyled: restyled), plan: plan, restyled: restyled)
        let restyledRanges = restyled.map { plan.lines[$0].range.nsRange }
        SentenceTinter(tokens: tokens, storage: storage).tint(prose.tints, within: restyledRanges)
        prose.mark(within: restyledRanges, storage: storage)
        appliedLines = plan.lines
        appliedFolds = headingFolds
        forcedLines = []
    }

    /// `lines` and every row of a table one of them is in, since a table's
    /// columns are laid out from all its rows at once.
    private func withWholeTables(_ lines: Set<Int>, plan: NotePlan) -> Set<Int> {
        let tables = Set(lines.compactMap { plan.lines[$0].tableRow?.tableStart })
        guard !tables.isEmpty else { return lines }
        let rows = plan.lines.indices.filter { index in
            plan.lines[index].tableRow.map { tables.contains($0.tableStart) } ?? false
        }
        return lines.union(rows)
    }

    /// The grids of tables laid out again replace their old ones; tables
    /// that are gone or fit now lose theirs.
    private func keepGrids(_ laidOut: [UInt32: TableGridModel], plan: NotePlan, restyled: Set<Int>) {
        let redone = Set(restyled.compactMap { plan.lines[$0].tableRow?.tableStart })
        let present = Set(plan.lines.compactMap { $0.tableRow?.tableStart })
        gridTables = gridTables
            .filter { present.contains($0.key) && !redone.contains($0.key) }
            .merging(laidOut) { $1 }
    }

    private func needsStyling(_ index: Int, _ line: LinePlan, _ edited: NSRange?) -> Bool {
        guard index < appliedLines.count, appliedLines[index] == line, !forcedLines.contains(index),
              appliedFolds[line.line] == headingFolds[line.line] else { return true }
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
    private func decorateBlocks(
        _ plan: NotePlan, fallbacks: BlockFallbacks, restyled: Set<Int>, storage: NSTextStorage
    ) {
        let touched = Set(restyled.flatMap { [$0 - 1, $0, $0 + 1] })
        for index in touched.sorted() where plan.lines.indices.contains(index) {
            let paragraph = paragraphRange(plan, index, length: storage.length)
            guard paragraph.length > 0 else { continue }
            let neighbours = BlockNeighbours(plan: plan, fallbacks: fallbacks, index: index)
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
