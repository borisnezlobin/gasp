import Foundation

/// The plan the text view shows, kept from the core's updates. Lines that
/// changed arrive with their offsets now; the others stay as they arrived
/// and remember how far edits have moved them since, so a keystroke costs
/// what it changed rather than the note's length. Reading a line gives it
/// with its offsets now.
struct ShownPlan {
    private var lines: [LinePlan] = []
    /// How far each line has moved since it arrived, in UTF-16 units.
    private var moved: [Int32] = []
    /// The lines that are rows of a table.
    private(set) var tableRows = IndexSet()
    /// The lines holding a math block or a link card, which the phone may
    /// draw from their source.
    private(set) var blockLines = IndexSet()

    var count: Int { lines.count }
    var indices: Range<Int> { lines.indices }

    subscript(index: Int) -> LinePlan {
        let shift = moved[index]
        return shift == 0 ? lines[index] : lines[index].moved(by: shift)
    }

    /// The line as it arrived, for what doesn't depend on where it is:
    /// its decorations, its widgets' kinds, whether it takes room.
    func unmoved(_ index: Int) -> LinePlan {
        lines[index]
    }

    func start(of index: Int) -> Int {
        Int(lines[index].range.start) + Int(moved[index])
    }

    func end(of index: Int) -> Int {
        Int(lines[index].range.end) + Int(moved[index])
    }

    func range(of index: Int) -> NSRange {
        NSRange(location: start(of: index), length: end(of: index) - start(of: index))
    }

    /// Where the table the line is a row of starts.
    func tableStart(of index: Int) -> UInt32? {
        lines[index].tableRow.map { UInt32(Int($0.tableStart) + Int(moved[index])) }
    }

    /// The lines `range` overlaps or touches at either end.
    func lines(touching range: NSRange) -> Range<Int> {
        var low = 0
        var high = count
        while low < high {
            let middle = (low + high) / 2
            if end(of: middle) < range.location { low = middle + 1 } else { high = middle }
        }
        var last = low
        while last < count, start(of: last) <= NSMaxRange(range) { last += 1 }
        return low..<max(last, low)
    }

    /// Takes the core's update, answering the lines whose plan differs from
    /// the one shown before. Answers `nil` when the update doesn't fit the
    /// copy, which then holds nothing, so the whole plan is asked for.
    mutating func apply(_ update: PlanUpdate) -> IndexSet? {
        if update.whole {
            return replaceAll(with: update.lines)
        }
        var unfilled = IndexSet()
        if let splice = update.splice {
            guard let spliced = follow(splice) else { return forgetAll() }
            unfilled = spliced
        }
        guard count == Int(update.lineCount) else { return forgetAll() }
        var changed = IndexSet()
        for line in update.lines {
            let index = Int(line.line)
            guard lines.indices.contains(index) else { return forgetAll() }
            let isNew = unfilled.remove(index) != nil
            if isNew || self[index] != line {
                store(line, at: index)
                changed.insert(index)
            }
        }
        return unfilled.isEmpty ? changed : forgetAll()
    }

    /// Replaces every line, answering the ones that differ from before.
    private mutating func replaceAll(with fresh: [LinePlan]) -> IndexSet {
        let old = self
        lines = fresh
        moved = Array(repeating: 0, count: fresh.count)
        tableRows = IndexSet(fresh.indices.filter { fresh[$0].tableRow != nil })
        blockLines = IndexSet(fresh.indices.filter { fresh[$0].holdsBlock })
        return IndexSet(fresh.indices.filter { $0 >= old.count || old[$0] != fresh[$0] })
    }

    private mutating func forgetAll() -> IndexSet? {
        self = ShownPlan()
        return nil
    }

    /// Makes room for the lines an edit put in, answering where they go,
    /// and moves the lines after them.
    private mutating func follow(_ splice: PlanSplice) -> IndexSet? {
        let (first, removed, inserted) = (Int(splice.at), Int(splice.removed), Int(splice.inserted))
        guard first + removed <= count else { return nil }
        let placeholder = LinePlan(
            line: 0, range: TextRange(start: 0, end: 0), decorations: [], runs: [], hidden: [], widgets: [],
            collapsed: false, tableRow: nil, headingFold: nil
        )
        lines.replaceSubrange(first..<first + removed, with: repeatElement(placeholder, count: inserted))
        moved.replaceSubrange(first..<first + removed, with: repeatElement(0, count: inserted))
        let growth = inserted - removed
        let lineShift = Int32(growth)
        let shift = splice.shift
        moved.withUnsafeMutableBufferPointer { moved in
            for index in (first + inserted)..<moved.count { moved[index] += shift }
        }
        if lineShift != 0 {
            for index in (first + inserted)..<lines.count {
                lines[index].line = UInt32(Int(lines[index].line) + growth)
            }
        }
        for rows in [\ShownPlan.tableRows, \ShownPlan.blockLines] {
            self[keyPath: rows].remove(integersIn: first..<first + removed)
            self[keyPath: rows].shift(startingAt: first + removed, by: growth)
        }
        return IndexSet(integersIn: first..<first + inserted)
    }

    private mutating func store(_ line: LinePlan, at index: Int) {
        lines[index] = line
        moved[index] = 0
        if line.tableRow != nil { tableRows.insert(index) } else { tableRows.remove(index) }
        if line.holdsBlock { blockLines.insert(index) } else { blockLines.remove(index) }
    }
}

extension LinePlan {
    /// Whether the line holds a math block or a link card.
    var holdsBlock: Bool {
        widgets.contains { widget in
            switch widget.kind {
            case .mathBlock, .linkCard: true
            default: false
            }
        }
    }

    /// The line with every offset in it `shift` units on.
    func moved(by shift: Int32) -> LinePlan {
        var line = self
        line.range = range.moved(by: shift)
        line.runs = runs.map { StyledRun(range: $0.range.moved(by: shift), styles: $0.styles) }
        line.hidden = hidden.map { $0.moved(by: shift) }
        line.widgets = widgets.map { $0.moved(by: shift) }
        line.tableRow = tableRow.map { row in
            var row = row
            row.cells = row.cells.map { $0.moved(by: shift) }
            row.tableStart = UInt32(Int(row.tableStart) + Int(shift))
            return row
        }
        return line
    }
}

extension Widget {
    func moved(by shift: Int32) -> Widget {
        var widget = self
        widget.range = range.moved(by: shift)
        switch kind {
        case let .calloutHeader(kind, typeName, title, defaultTitle, folded):
            widget.kind = .calloutHeader(
                kind: kind, typeName: typeName, title: title?.moved(by: shift), defaultTitle: defaultTitle,
                folded: folded
            )
        case let .codeBlock(language, title, content):
            widget.kind = .codeBlock(language: language, title: title, content: content.moved(by: shift))
        default:
            break
        }
        return widget
    }
}

extension TextRange {
    func moved(by shift: Int32) -> TextRange {
        TextRange(start: UInt32(Int(start) + Int(shift)), end: UInt32(Int(end) + Int(shift)))
    }

    /// Where the range is after the text in `replaced` became `change`
    /// units longer, or `nil` when the edit reached it.
    func following(replaced: NSRange, change: Int) -> TextRange? {
        if Int(end) < replaced.location { return self }
        guard Int(start) > NSMaxRange(replaced) else { return nil }
        return moved(by: Int32(change))
    }
}
