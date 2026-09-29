import UIKit

/// Applies the core's render plan to the text storage as attributes: fonts
/// and colours for styled runs, hidden markup, widgets and block
/// decorations. The core sends only the lines that may have changed, and
/// only those whose plan did change, or that an edit touched, are
/// restyled.
final class PlanStyler {
    private let tokens: Tokens
    private(set) var shown = ShownPlan()
    /// Whether the next update restyles every line whatever it says.
    private var restylesAll = true
    /// Lines to restyle next time whatever their plan says.
    private var forcedLines = IndexSet()
    /// The text column's width, which decides whether a table fits as a grid.
    private var columnWidth: CGFloat = .greatestFiniteMagnitude
    /// Rendered math and the vault's images for the note.
    var media: NoteMedia?
    /// Wide tables drawn as a scrolling grid, by where each starts.
    private(set) var gridTables: [UInt32: TableGridModel] = [:]
    /// Grids the last update moved, from where they started to where they
    /// start now, and the ones it laid out again.
    private(set) var movedGrids: [UInt32: UInt32] = [:]
    private(set) var laidOutGrids: Set<UInt32> = []

    init(tokens: Tokens) {
        self.tokens = tokens
    }

    /// Whether the styler holds no plan, so the core should send all of it.
    var needsWholePlan: Bool { shown.count == 0 }

    /// A new column width changes how tables lay out, so the next plan
    /// restyles every line.
    func setColumnWidth(_ width: CGFloat) {
        columnWidth = width
        media?.columnWidth = width
        forgetApplied()
    }

    /// Makes the next plan restyle every line.
    func forgetApplied() {
        restylesAll = true
    }

    /// Makes the next plan restyle the lines `matching` says, and every
    /// line their widgets reach, such as a `$$` block's.
    func forgetLines(matching: (LinePlan) -> Bool) {
        for index in shown.indices where matching(shown.unmoved(index)) {
            forcedLines.insert(index)
            for widget in shown[index].widgets {
                forcedLines.insert(integersIn: shown.lines(touching: widget.range.nsRange))
            }
        }
    }

    /// What newly typed text looks like until the next plan arrives.
    var typingAttributes: [NSAttributedString.Key: Any] {
        let look = RunLook(tokens: tokens)
        return look.attributes(tokens).merging([.paragraphStyle: paragraphStyle(LineShape([]), look)]) { $1 }
    }

    /// Takes the core's update and restyles the lines whose plan changed
    /// or that `edited` touches, then tints the sentences, marks the
    /// grammar flags and colours the code on those lines. Answers false,
    /// restyling nothing, when the update doesn't fit what's shown, so the
    /// whole plan should be asked for.
    func apply(
        _ update: PlanUpdate, prose: ProseMarks, code: CodeColors, to storage: NSTextStorage, edited: NSRange?
    ) -> Bool {
        guard let changed = shown.apply(update) else { return false }
        guard shown.count > 0, shown.end(of: shown.count - 1) <= storage.length else {
            shown = ShownPlan()
            return false
        }
        var lines = restylesAll ? IndexSet(shown.indices) : changed.union(forcedLines)
        followGrids(update.splice)
        if let edited { lines.insert(integersIn: shown.lines(touching: edited)) }
        restyle(withWholeTables(lines), prose: prose, code: code, storage: storage)
        restylesAll = false
        forcedLines = []
        return true
    }

    private func restyle(_ restyled: IndexSet, prose: ProseMarks, code: CodeColors, storage: NSTextStorage) {
        let text = storage.string as NSString
        let fallbacks = BlockFallbacks(plan: shown, text: text) { [media, tokens] tex in
            media?.math(MathKey(tex: tex, display: true, fontSize: tokens.bodySize)) != nil
        }
        for index in restyled {
            let line = shown[index]
            let paragraph = paragraphRange(index, length: storage.length)
            var styler = LineStyler(tokens: tokens, storage: storage, text: text, line: line)
            styler.media = media
            styler.fold = line.headingFold
            styler.cardImage = fallbacks.cardImages[index]
            styler.style(paragraph: paragraph, presentation: fallbacks.presentation(of: index))
        }
        decorateBlocks(fallbacks: fallbacks, restyled: restyled, storage: storage)
        let tables = TableLayout(tokens: tokens, storage: storage, columnWidth: columnWidth)
        keepGrids(tables.layOut(shown, restyled: restyled), restyled: restyled)
        let restyledRanges = restyled.map { shown.range(of: $0) }
        let codeLines = restyled.filter { LineShape(shown.unmoved($0).decorations).isCode }
        code.paint(within: codeLines.map { shown.range(of: $0) }, storage: storage)
        SentenceTinter(tokens: tokens, storage: storage).tint(prose.tints, within: restyledRanges)
        prose.mark(within: restyledRanges, storage: storage)
    }

    /// A wide table's grid holds the offsets of its cells, so the grids of
    /// tables an edit moved move with them.
    private func followGrids(_ splice: PlanSplice?) {
        movedGrids = [:]
        guard let splice, splice.shift != 0, !gridTables.isEmpty else { return }
        let starts = Set(shown.tableRows.compactMap { shown.tableStart(of: $0) })
        for (start, model) in gridTables where !starts.contains(start) {
            let moved = UInt32(Int(start) + Int(splice.shift))
            guard starts.contains(moved) else { continue }
            gridTables[start] = nil
            gridTables[moved] = model.moved(by: Int(splice.shift))
            movedGrids[start] = moved
        }
    }

    /// `lines` and every row of a table one of them is in, since a table's
    /// columns are laid out from all its rows at once.
    private func withWholeTables(_ lines: IndexSet) -> IndexSet {
        let tables = Set(lines.intersection(shown.tableRows).compactMap { shown.tableStart(of: $0) })
        guard !tables.isEmpty else { return lines }
        let rows = shown.tableRows.filter { index in shown.tableStart(of: index).map(tables.contains) ?? false }
        return lines.union(IndexSet(rows))
    }

    /// The grids of tables laid out again replace their old ones; tables
    /// that are gone or fit now lose theirs.
    private func keepGrids(_ laidOut: [UInt32: TableGridModel], restyled: IndexSet) {
        let redone = Set(restyled.intersection(shown.tableRows).compactMap { shown.tableStart(of: $0) })
        let present = Set(shown.tableRows.compactMap { shown.tableStart(of: $0) })
        gridTables = gridTables
            .filter { present.contains($0.key) && !redone.contains($0.key) }
            .merging(laidOut) { $1 }
        laidOutGrids = Set(laidOut.keys)
    }

    private func paragraphRange(_ index: Int, length: Int) -> NSRange {
        let start = shown.start(of: index)
        let next = index + 1 < shown.count ? shown.start(of: index + 1) : length
        let end = min(max(next, start), length)
        return NSRange(location: min(start, length), length: end - min(start, length))
    }

    /// Code blocks and callouts draw rounded ends, so a line's decoration
    /// depends on its neighbours; lines next to a restyled one are redone.
    private func decorateBlocks(fallbacks: BlockFallbacks, restyled: IndexSet, storage: NSTextStorage) {
        let touched = Set(restyled.flatMap { [$0 - 1, $0, $0 + 1] })
        for index in touched.sorted() where shown.indices.contains(index) {
            let paragraph = paragraphRange(index, length: storage.length)
            guard paragraph.length > 0 else { continue }
            let neighbours = BlockNeighbours(plan: shown, fallbacks: fallbacks, index: index)
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
