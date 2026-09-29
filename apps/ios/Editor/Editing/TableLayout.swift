import UIKit

/// Lays a table's rows out as columns: the pipes between cells become tabs,
/// with tab stops at the widest cell of each column, and the header is bold.
/// A table too wide for the column is drawn by a grid that scrolls
/// sideways instead: its rows keep their place in the text, one line each,
/// with their text hidden, and the grid takes each cell's styled text.
struct TableLayout {
    let tokens: Tokens
    let storage: NSTextStorage
    let columnWidth: CGFloat

    /// Lays out the tables with a restyled row, and answers the wide ones'
    /// grids by where each table starts.
    func layOut(_ plan: NotePlan, restyled: Set<Int>) -> [UInt32: TableGridModel] {
        let rows = plan.lines.enumerated().filter { $0.element.tableRow != nil }
        let tables = Dictionary(grouping: rows) { $0.element.tableRow?.tableStart ?? 0 }
        var grids: [UInt32: TableGridModel] = [:]
        for (start, table) in tables where table.contains(where: { restyled.contains($0.offset) }) {
            let header = table.first { $0.element.tableRow?.index == 0 && restyled.contains($0.offset) }
            if let header { embolden(header.element) }
            grids[start] = layOut(table: table.map(\.element))
        }
        return grids
    }

    private var text: NSString { storage.string as NSString }

    private func layOut(table: [LinePlan]) -> TableGridModel? {
        let widths = columnWidths(table)
        let stops = tabStops(widths)
        guard (stops.last ?? 0) + (widths.last ?? 0) <= columnWidth else { return grid(table) }
        for line in table {
            innerPipes(line).forEach(showAsTab)
            setTabStops(stops, on: line)
        }
        return nil
    }

    /// Takes each cell's styled text for the grid, then hides the rows.
    private func grid(_ table: [LinePlan]) -> TableGridModel {
        let model = TableGridModel(rows: table.map(gridRow))
        for line in table {
            let range = line.range.nsRange
            guard range.length > 0, NSMaxRange(range) <= storage.length else { continue }
            let paragraph = storage.attribute(.paragraphStyle, at: range.location, effectiveRange: nil)
            storage.setAttributes([
                .font: tokens.textFont(size: 0.01),
                .foregroundColor: UIColor.clear,
                .paragraphStyle: paragraph ?? NSParagraphStyle.default,
                .gridRow: true
            ], range: range)
            storage.removeAttribute(.blockDecoration, range: (text as NSString).paragraphRange(for: range))
        }
        return model
    }

    private func gridRow(_ line: LinePlan) -> TableGridModel.Row {
        let cells = (line.tableRow?.cells ?? []).map { cell -> TableGridModel.Cell in
            let range = cell.nsRange
            let valid = NSMaxRange(range) <= storage.length
            let styled = NSMutableAttributedString(
                attributedString: valid ? storage.attributedSubstring(from: range) : NSAttributedString()
            )
            styled.removeAttribute(.paragraphStyle, range: NSRange(location: 0, length: styled.length))
            return TableGridModel.Cell(range: range, text: DisplayParagraphs.displayed(styled))
        }
        return TableGridModel.Row(
            line: line.range.nsRange, header: line.tableRow?.index == 0, cells: cells,
            alignments: (line.tableRow?.alignments ?? []).map(\.nsTextAlignment)
        )
    }

    /// The pipe between each pair of cells, by where it sits.
    private func innerPipes(_ line: LinePlan) -> [Int] {
        guard let cells = line.tableRow?.cells, cells.count > 1 else { return [] }
        return zip(cells, cells.dropFirst()).compactMap { left, right in
            let gap = NSRange(location: Int(left.end), length: Int(right.start) - Int(left.end))
            let pipe = text.range(of: "|", options: [], range: gap)
            return pipe.location == NSNotFound ? nil : pipe.location
        }
    }

    private func showAsTab(_ pipe: Int) {
        storage.addAttributes([
            .displaySubstitute: DisplaySubstitute(.text("\t")),
            .foregroundColor: UIColor.clear
        ], range: NSRange(location: pipe, length: 1))
    }

    private func embolden(_ line: LinePlan) {
        let range = line.range.nsRange
        storage.enumerateAttribute(.font, in: range) { value, run, _ in
            guard let font = value as? UIFont, font.pointSize >= 1,
                  let bold = font.fontDescriptor.withSymbolicTraits(
                      font.fontDescriptor.symbolicTraits.union(.traitBold)
                  )
            else { return }
            storage.addAttribute(.font, value: UIFont(descriptor: bold, size: font.pointSize), range: run)
        }
    }

    /// Each column's widest text in any row, its padding included.
    private func columnWidths(_ table: [LinePlan]) -> [CGFloat] {
        var widths: [CGFloat] = []
        for line in table {
            let bounds = [Int(line.range.start)] + innerPipes(line)
            for (column, start) in bounds.enumerated() {
                let end = column + 1 < bounds.count ? bounds[column + 1] : Int(line.range.end)
                let width = measure(NSRange(location: start, length: max(end - start, 0)))
                if column < widths.count { widths[column] = max(widths[column], width) } else { widths.append(width) }
            }
        }
        return widths
    }

    /// Where each column after the first starts.
    private func tabStops(_ widths: [CGFloat]) -> [CGFloat] {
        let gap = CGFloat(tokens.spacing.xl)
        return widths.dropLast().reduce(into: [CGFloat]()) { stops, width in
            stops.append((stops.last ?? 0) + width + gap)
        }
    }

    private func measure(_ range: NSRange) -> CGFloat {
        guard NSMaxRange(range) <= storage.length, range.length > 0 else { return 0 }
        return ceil(storage.attributedSubstring(from: range).size().width)
    }

    private func setTabStops(_ stops: [CGFloat], on line: LinePlan) {
        let range = line.range.nsRange
        guard range.length > 0, NSMaxRange(range) <= storage.length,
              let current = storage.attribute(.paragraphStyle, at: range.location, effectiveRange: nil)
                as? NSParagraphStyle,
              let style = current.mutableCopy() as? NSMutableParagraphStyle
        else { return }
        style.tabStops = stops.map { NSTextTab(textAlignment: .left, location: $0) }
        style.defaultTabInterval = CGFloat(tokens.spacing.xl)
        storage.addAttribute(.paragraphStyle, value: style, range: range)
    }
}
