import UIKit

/// Wide tables as scrolling grids over their rows. A cell is edited in
/// place and written back through the core, which pads the table; the
/// menu's row, column and table commands are the core's table commands,
/// run with the cursor in the cell.
extension EditingController: TableGridHost {
    /// Adds, updates and removes grids to match the tables the styler found
    /// too wide, then places them.
    func updateTableGrids() {
        let models = styler.gridTables
        for (old, new) in styler.movedGrids {
            guard let grid = tableGrids.removeValue(forKey: old) else { continue }
            grid.tableStart = new
            tableGrids[new] = grid
        }
        for (start, grid) in tableGrids where models[start] == nil {
            grid.removeFromSuperview()
            tableGrids[start] = nil
        }
        let moved = Set(styler.movedGrids.values)
        for (start, model) in models {
            guard let grid = tableGrids[start] else {
                let grid = makeGrid(start, model)
                tableGrids[start] = grid
                place(grid)
                continue
            }
            if styler.laidOutGrids.contains(start) {
                grid.model = model
                place(grid)
            } else if moved.contains(start) {
                // Placed with the rest once the text is laid out again.
                grid.follow(model)
            }
        }
    }

    /// Moves every grid onto its rows, after the text was laid out again.
    func placeTableGrids() {
        tableGrids.values.forEach(place)
    }

    /// The text the viewport shows, in UTF-16 offsets, or `nil` before any
    /// layout.
    private var viewportCharacters: NSRange? {
        guard let manager = textView.textLayoutManager, let storage = manager.textContentManager,
              let viewport = manager.textViewportLayoutController.viewportRange else { return nil }
        let start = storage.offset(from: storage.documentRange.location, to: viewport.location)
        let end = storage.offset(from: storage.documentRange.location, to: viewport.endLocation)
        return NSRange(location: start, length: max(end - start, 0))
    }

    /// Whether any of the grid's rows are where the viewport lays text out.
    /// Asking TextKit where rows elsewhere are would lay the text out up to
    /// them.
    private func isInViewport(_ grid: TableGridView) -> Bool {
        guard let viewport = viewportCharacters, let first = grid.model.rows.first,
              let last = grid.model.rows.last else { return false }
        let table = NSRange(location: first.line.location, length: NSMaxRange(last.line) - first.line.location)
        return NSIntersectionRange(table, viewport).length > 0 || NSLocationInRange(table.location, viewport)
    }

    private func makeGrid(_ start: UInt32, _ model: TableGridModel) -> TableGridView {
        let grid = TableGridView(tableStart: start, model: model, tokens: tokens)
        grid.host = self
        textView.addSubview(grid)
        return grid
    }

    /// Lines the grid up with its rows' lines, or hides it while they
    /// aren't laid out.
    private func place(_ grid: TableGridView) {
        guard isInViewport(grid) else {
            grid.isHidden = true
            return
        }
        let frames = grid.model.rows.map { rowFrame($0.line.location) }
        guard let first = frames.first ?? nil, frames.allSatisfy({ $0 != nil }) else {
            grid.isHidden = true
            return
        }
        grid.isHidden = false
        let top = first.minY
        let rows = frames.compactMap { $0 }.map { (top: $0.minY - top, height: $0.height) }
        let bottom = rows.last.map { $0.top + $0.height } ?? 0
        let origin = CGPoint(x: 0, y: top + textView.textContainerInset.top)
        grid.frame = CGRect(origin: origin, size: CGSize(width: textView.bounds.width, height: bottom))
        grid.update(rows: rows, sideInset: textView.textContainerInset.left)
    }

    /// The laid-out frame of the line starting at `offset`, in the text
    /// container.
    private func rowFrame(_ offset: Int) -> CGRect? {
        guard let manager = textView.textLayoutManager, let storage = manager.textContentManager,
              let location = storage.location(storage.documentRange.location, offsetBy: offset),
              let fragment = manager.textLayoutFragment(for: location),
              fragment.state == .layoutAvailable
        else { return nil }
        return fragment.layoutFragmentFrame
    }

    // MARK: Redirecting the cursor

    /// A cursor that lands in a wide table's hidden rows edits the cell
    /// it's in instead. Answers whether it did.
    func editGridCell(at location: Int) -> Bool {
        guard !placingCursorForTable, location < textView.textStorage.length,
              textView.textStorage.attribute(.gridRow, at: location, effectiveRange: nil) != nil
        else { return false }
        for grid in tableGrids.values {
            guard let cell = cell(in: grid.model, at: location) else { continue }
            textView.resignFirstResponder()
            gridTapped(grid, cell: cell)
            return true
        }
        return false
    }

    private func cell(in model: TableGridModel, at location: Int) -> GridCell? {
        guard let row = model.rows.firstIndex(where: { NSLocationInRange(location, $0.line)
            || NSMaxRange($0.line) == location }) else { return nil }
        let cells = model.rows[row].cells
        let column = cells.firstIndex { location <= NSMaxRange($0.range) } ?? max(cells.count - 1, 0)
        return GridCell(row: row, column: column)
    }

    // MARK: TableGridHost

    func gridTapped(_ grid: TableGridView, cell: GridCell) {
        guard let source = grid.model.cell(cell.row, cell.column) else { return }
        let text = (textView.text as NSString).substring(with: source.range)
        grid.beginEditing(cell, source: text)
    }

    func gridCommitted(_ grid: TableGridView, cell: GridCell, text: String, then move: CellMove) {
        let start = grid.tableStart
        guard let source = grid.model.cell(cell.row, cell.column) else { return }
        if text != (textView.text as NSString).substring(with: source.range) {
            placingCursorForTable = true
            apply(document.setTableCell(offset: UInt32(source.range.location), text: text))
            placingCursorForTable = false
        }
        follow(move, from: cell, inTable: start)
    }

    /// Opens the cell `move` goes to, adding a row when it runs off the end.
    private func follow(_ move: CellMove, from cell: GridCell, inTable start: UInt32) {
        guard let grid = tableGrids[start] else { return }
        let model = grid.model
        let columns = max(model.columnCount, 1)
        var target: GridCell
        switch move {
        case .stay: return
        case .leave: return leaveTable(model)
        case .below: target = GridCell(row: cell.row + 1, column: cell.column)
        case .previous:
            target = cell.column > 0 ? GridCell(row: cell.row, column: cell.column - 1)
                : GridCell(row: cell.row - 1, column: columns - 1)
        case .next:
            target = cell.column + 1 < columns ? GridCell(row: cell.row, column: cell.column + 1)
                : GridCell(row: cell.row + 1, column: 0)
        }
        if target.row >= model.rows.count {
            runTableCommand("table.insert-row-below", at: cell, in: model)
        }
        guard target.row >= 0, let fresh = tableGrids[start] else { return }
        gridTapped(fresh, cell: target)
    }

    /// Puts the cursor on the line after the table.
    private func leaveTable(_ model: TableGridModel) {
        guard let last = model.rows.last else { return }
        let after = min(NSMaxRange(last.line) + 1, textView.textStorage.length)
        placingCursorForTable = true
        textView.becomeFirstResponder()
        textView.selectedRange = NSRange(location: after, length: 0)
        placingCursorForTable = false
    }

    func gridMenu(_ grid: TableGridView, cell: GridCell) -> UIMenu? {
        guard let source = grid.model.cell(cell.row, cell.column) else { return nil }
        let available = Set(document.tableCommandsAt(offset: UInt32(source.range.location)))
        let action = { [weak self, weak grid] (item: TableMenuItem) -> UIAction in
            let command = UIAction(title: item.title, image: UIImage(systemName: item.symbol)) { _ in
                guard let self, let grid else { return }
                self.runTableCommand(item.command, at: cell, in: grid.model)
            }
            if item.needsCheck, !available.contains(item.command) { command.attributes = .disabled }
            if item.destructive { command.attributes.insert(.destructive) }
            return command
        }
        return TableMenuItem.menu(action)
    }

    /// Runs a table command from the registry with the cursor in `cell`.
    func runTableCommand(_ command: String, at cell: GridCell, in model: TableGridModel) {
        guard let source = model.cell(cell.row, cell.column) else { return }
        placingCursorForTable = true
        textView.selectedRange = NSRange(location: source.range.location, length: 0)
        placingCursorForTable = false
        host?.run(command)
    }
}

/// An item of a table cell's menu, in the desktop's order.
struct TableMenuItem {
    let command: String
    let title: String
    let symbol: String
    /// Whether it applies only to some cells, as the core decides.
    var needsCheck = true
    var destructive = false

    /// The menu: inserting rows and columns first, then Row, Column and
    /// Table submenus.
    static func menu(_ action: (TableMenuItem) -> UIAction) -> UIMenu {
        let inserts = UIMenu(options: .displayInline, children: [
            TableMenuItem(command: "table.insert-row-above", title: "Insert row above", symbol: "arrow.up"),
            TableMenuItem(command: "table.insert-row-below", title: "Insert row below", symbol: "arrow.down"),
            TableMenuItem(command: "table.insert-column-left", title: "Insert column left", symbol: "arrow.left"),
            TableMenuItem(command: "table.insert-column-right", title: "Insert column right", symbol: "arrow.right")
        ].map(action))
        let row = UIMenu(title: "Row", image: UIImage(systemName: "tablecells"), children: [
            TableMenuItem(command: "table.move-row-up", title: "Move up", symbol: "arrow.up.to.line"),
            TableMenuItem(command: "table.move-row-down", title: "Move down", symbol: "arrow.down.to.line"),
            TableMenuItem(command: "table.delete-row", title: "Delete row", symbol: "trash", destructive: true)
        ].map(action))
        let column = UIMenu(
            title: "Column", image: UIImage(systemName: "tablecells"), children: columnItems.map(action)
        )
        let table = UIMenu(title: "Table", image: UIImage(systemName: "tablecells"), children: [
            TableMenuItem(command: "table.copy-markdown", title: "Copy as Markdown", symbol: "doc.on.doc",
                          needsCheck: false),
            TableMenuItem(command: "table.copy-tsv", title: "Copy as tab-separated text", symbol: "doc.on.clipboard",
                          needsCheck: false),
            TableMenuItem(command: "table.edit-as-markdown", title: "Edit as Markdown",
                          symbol: "chevron.left.forwardslash.chevron.right", needsCheck: false),
            TableMenuItem(command: "table.delete", title: "Delete table", symbol: "trash", destructive: true)
        ].map(action))
        return UIMenu(children: [inserts, row, column, table])
    }

    private static let columnItems = [
        TableMenuItem(command: "table.move-column-left", title: "Move left", symbol: "arrow.left.to.line"),
        TableMenuItem(command: "table.move-column-right", title: "Move right", symbol: "arrow.right.to.line"),
        TableMenuItem(command: "table.align-left", title: "Align left", symbol: "text.alignleft"),
        TableMenuItem(command: "table.align-center", title: "Align center", symbol: "text.aligncenter"),
        TableMenuItem(command: "table.align-right", title: "Align right", symbol: "text.alignright"),
        TableMenuItem(command: "table.sort-ascending", title: "Sort A to Z", symbol: "arrow.up.arrow.down"),
        TableMenuItem(command: "table.sort-descending", title: "Sort Z to A", symbol: "arrow.up.arrow.down"),
        TableMenuItem(command: "table.delete-column", title: "Delete column", symbol: "trash", destructive: true)
    ]
}
