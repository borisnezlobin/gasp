import UIKit

/// A wide table as the grid draws it: each row's cells with their styled
/// text and where their source is.
struct TableGridModel {
    struct Cell {
        /// The cell's text in the note, without its padding.
        let range: NSRange
        let text: NSAttributedString
    }

    struct Row {
        /// The row's line in the note.
        let line: NSRange
        let header: Bool
        let cells: [Cell]
        let alignments: [NSTextAlignment]
    }

    let rows: [Row]

    var columnCount: Int {
        rows.map(\.cells.count).max() ?? 0
    }

    /// The same grid for its table after text before it grew by `shift`.
    func moved(by shift: Int) -> TableGridModel {
        let rows = rows.map { row in
            Row(
                line: NSRange(location: row.line.location + shift, length: row.line.length),
                header: row.header,
                cells: row.cells.map { cell in
                    let range = NSRange(location: cell.range.location + shift, length: cell.range.length)
                    return Cell(range: range, text: cell.text)
                },
                alignments: row.alignments
            )
        }
        return TableGridModel(rows: rows)
    }

    func cell(_ row: Int, _ column: Int) -> Cell? {
        guard rows.indices.contains(row), rows[row].cells.indices.contains(column) else { return nil }
        return rows[row].cells[column]
    }
}

/// A cell of a grid, by row (the header is 0) and column.
struct GridCell: Equatable {
    var row: Int
    var column: Int
}

/// What the grid asks of the note it's in.
protocol TableGridHost: AnyObject {
    func gridTapped(_ grid: TableGridView, cell: GridCell)
    func gridMenu(_ grid: TableGridView, cell: GridCell) -> UIMenu?
    func gridCommitted(_ grid: TableGridView, cell: GridCell, text: String, then move: CellMove)
}

/// Where the cell editor goes after its text is written back.
enum CellMove {
    case stay
    case next
    case previous
    case below
    case leave
}

/// A table too wide for the screen, drawn as a grid that scrolls sideways
/// over the rows it stands for. A tap on a cell edits it in place; a long
/// press offers the table editor's row, column and table commands.
final class TableGridView: UIScrollView, UIContextMenuInteractionDelegate {
    /// Where the table starts, which moves as text before it changes.
    var tableStart: UInt32
    weak var host: TableGridHost?
    /// What the grid draws; it's drawn at its rows' place by `update`.
    var model: TableGridModel
    private let tokens: Tokens
    private let content: TableGridContent
    private var editor: CellEditor?
    private(set) var editing: GridCell?
    /// The cell a long press opened the menu on.
    private var menuCell: GridCell?

    init(tableStart: UInt32, model: TableGridModel, tokens: Tokens) {
        self.tableStart = tableStart
        self.model = model
        self.tokens = tokens
        content = TableGridContent(tokens: tokens)
        super.init(frame: .zero)
        showsVerticalScrollIndicator = false
        alwaysBounceHorizontal = false
        backgroundColor = tokens.color(\.background)
        addSubview(content)
        content.addGestureRecognizer(UITapGestureRecognizer(target: self, action: #selector(tapped(_:))))
        content.addInteraction(UIContextMenuInteraction(delegate: self))
        isAccessibilityElement = false
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("Grids aren't decoded")
    }

    /// Draws the model with each row at its line's place, given as the top
    /// and height of each row relative to the grid.
    func update(rows: [(top: CGFloat, height: CGFloat)], sideInset: CGFloat) {
        content.lay(model, rows: rows)
        contentInset = UIEdgeInsets(top: 0, left: sideInset, bottom: 0, right: sideInset)
        contentSize = content.bounds.size
        if let editing { placeEditor(at: editing) }
    }

    // MARK: Cells

    @objc private func tapped(_ tap: UITapGestureRecognizer) {
        guard let cell = content.cell(at: tap.location(in: content)) else { return }
        host?.gridTapped(self, cell: cell)
    }

    /// Edits `cell` in place, with `source` as its text.
    func beginEditing(_ cell: GridCell, source: String) {
        finishEditing(commit: true, move: .stay)
        let editor = CellEditor(tokens: tokens)
        editor.text = source
        editor.onFinish = { [weak self] text, move in
            guard let self, let editing = self.editing else { return }
            self.editing = nil
            self.editor = nil
            editor.removeFromSuperview()
            self.content.highlighted = nil
            self.host?.gridCommitted(self, cell: editing, text: text, then: move)
        }
        editing = cell
        self.editor = editor
        content.highlighted = cell
        content.addSubview(editor)
        placeEditor(at: cell)
        scrollRectToVisible(content.rect(of: cell).insetBy(dx: -CGFloat(tokens.spacing.xl), dy: 0), animated: true)
        editor.becomeFirstResponder()
    }

    /// Writes back or drops what the cell editor holds.
    func finishEditing(commit: Bool, move: CellMove) {
        guard let editor else { return }
        if commit {
            editor.finish(move)
        } else {
            editor.onFinish = nil
            editor.removeFromSuperview()
            self.editor = nil
            editing = nil
            content.highlighted = nil
        }
    }

    private func placeEditor(at cell: GridCell) {
        editor?.frame = content.rect(of: cell).insetBy(dx: CGFloat(tokens.spacing.sm), dy: 1)
    }

    // MARK: UIContextMenuInteractionDelegate

    func contextMenuInteraction(
        _ interaction: UIContextMenuInteraction, configurationForMenuAtLocation location: CGPoint
    ) -> UIContextMenuConfiguration? {
        guard let cell = content.cell(at: location), let menu = host?.gridMenu(self, cell: cell) else { return nil }
        menuCell = cell
        return UIContextMenuConfiguration(identifier: nil, previewProvider: nil) { _ in menu }
    }

    /// The pressed cell lifts alone, rather than the whole grid.
    func contextMenuInteraction(
        _ interaction: UIContextMenuInteraction,
        configuration: UIContextMenuConfiguration,
        highlightPreviewForItemWithIdentifier identifier: any NSCopying
    ) -> UITargetedPreview? {
        guard let menuCell else { return nil }
        let rect = content.rect(of: menuCell)
        guard let snapshot = content.resizableSnapshotView(
            from: rect, afterScreenUpdates: false, withCapInsets: .zero
        ) else { return nil }
        let parameters = UIPreviewParameters()
        parameters.visiblePath = UIBezierPath(
            roundedRect: CGRect(origin: .zero, size: rect.size), cornerRadius: CGFloat(tokens.spacing.radiusMd)
        )
        parameters.backgroundColor = tokens.color(\.background)
        let target = UIPreviewTarget(container: content, center: CGPoint(x: rect.midX, y: rect.midY))
        return UITargetedPreview(view: snapshot, parameters: parameters, target: target)
    }
}

/// The grid's drawing: cells in shared columns, the header bold with a
/// line under it, hairlines between rows, and a ring round the cell being
/// edited.
final class TableGridContent: UIView {
    private let tokens: Tokens
    private var model = TableGridModel(rows: [])
    private var rows: [(top: CGFloat, height: CGFloat)] = []
    private var columnLefts: [CGFloat] = []
    private var columnWidths: [CGFloat] = []
    var highlighted: GridCell? {
        didSet { setNeedsDisplay() }
    }

    init(tokens: Tokens) {
        self.tokens = tokens
        super.init(frame: .zero)
        backgroundColor = .clear
        contentMode = .redraw
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("Grids aren't decoded")
    }

    private var padding: CGFloat { CGFloat(tokens.spacing.md) }

    func lay(_ model: TableGridModel, rows: [(top: CGFloat, height: CGFloat)]) {
        self.model = model
        self.rows = rows
        columnWidths = (0..<model.columnCount).map { column in
            let widest = model.rows.compactMap { $0.cells.indices.contains(column) ? $0.cells[column] : nil }
                .map { ceil($0.text.size().width) }
                .max() ?? 0
            return max(widest, CGFloat(tokens.spacing.xxl) * 2) + padding * 2
        }
        var left: CGFloat = 0
        columnLefts = columnWidths.map { width in
            defer { left += width }
            return left
        }
        let height = rows.last.map { $0.top + $0.height } ?? 0
        frame = CGRect(x: 0, y: 0, width: columnWidths.reduce(0, +), height: height)
        setNeedsDisplay()
    }

    func rect(of cell: GridCell) -> CGRect {
        guard rows.indices.contains(cell.row), columnLefts.indices.contains(cell.column) else { return .zero }
        let row = rows[cell.row]
        return CGRect(x: columnLefts[cell.column], y: row.top, width: columnWidths[cell.column], height: row.height)
    }

    func cell(at point: CGPoint) -> GridCell? {
        guard let row = rows.firstIndex(where: { point.y >= $0.top && point.y < $0.top + $0.height }),
              let column = columnLefts.lastIndex(where: { $0 <= point.x }) else { return nil }
        return GridCell(row: row, column: column)
    }

    override func draw(_ rect: CGRect) {
        let hairline = 1 / (window?.screen.scale ?? 2)
        for (index, row) in model.rows.enumerated() where rows.indices.contains(index) {
            let place = rows[index]
            let color = tokens.color(row.header ? \.divider : \.fill)
            color.setFill()
            UIRectFill(CGRect(x: 0, y: place.top + place.height - hairline, width: bounds.width, height: hairline))
            for (column, cell) in row.cells.enumerated() where columnLefts.indices.contains(column)
                && highlighted != GridCell(row: index, column: column) {
                draw(cell, alignment: row.alignments.indices.contains(column) ? row.alignments[column] : .natural,
                     in: self.rect(of: GridCell(row: index, column: column)))
            }
        }
        drawHighlight()
    }

    /// Draws a cell's text with its baseline on the row's, which sits
    /// where it would for plain text centred in the row.
    private func draw(_ cell: TableGridModel.Cell, alignment: NSTextAlignment, in rect: CGRect) {
        let size = cell.text.size()
        let inner = rect.insetBy(dx: padding, dy: 0)
        let left: CGFloat = switch alignment {
        case .center: inner.midX - size.width / 2
        case .right: inner.maxX - size.width
        default: inner.minX
        }
        let font = tokens.textFont(size: tokens.bodySize)
        let baseline = rect.midY + (font.ascender + font.descender) / 2
        cell.text.draw(at: CGPoint(x: left, y: baseline - Self.ascent(of: cell.text)))
    }

    /// How far the text reaches above its baseline: its tallest font's
    /// ascender, or a picture's top.
    private static func ascent(of text: NSAttributedString) -> CGFloat {
        var ascent: CGFloat = 0
        text.enumerateAttributes(in: NSRange(location: 0, length: text.length)) { attributes, _, _ in
            if let attachment = attributes[.attachment] as? NSTextAttachment {
                ascent = max(ascent, attachment.bounds.maxY)
            } else if let font = attributes[.font] as? UIFont, font.pointSize > 1 {
                ascent = max(ascent, font.ascender)
            }
        }
        return ascent
    }

    private func drawHighlight() {
        guard let highlighted else { return }
        let ring = UIBezierPath(
            roundedRect: rect(of: highlighted).insetBy(dx: 1, dy: 1), cornerRadius: CGFloat(tokens.spacing.radiusSm)
        )
        tokens.color(\.fill).setFill()
        ring.fill()
    }
}

/// The text field a cell is edited in. Return goes to the cell below and
/// Tab to the next, as on the desktop; Escape leaves the cell as it was.
final class CellEditor: UITextField, UITextFieldDelegate {
    var onFinish: ((String, CellMove) -> Void)?

    init(tokens: Tokens) {
        super.init(frame: .zero)
        font = tokens.textFont(size: tokens.bodySize)
        textColor = tokens.color(\.textStrong)
        tintColor = tokens.color(\.accent)
        autocapitalizationType = .sentences
        smartQuotesType = .no
        smartDashesType = .no
        returnKeyType = .next
        delegate = self
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("Editors aren't decoded")
    }

    func finish(_ move: CellMove) {
        let finish = onFinish
        onFinish = nil
        finish?(text ?? "", move)
    }

    override var keyCommands: [UIKeyCommand]? {
        let tab = UIKeyCommand(input: "\t", modifierFlags: [], action: #selector(nextCell))
        let backTab = UIKeyCommand(input: "\t", modifierFlags: .shift, action: #selector(previousCell))
        let escape = UIKeyCommand(input: UIKeyCommand.inputEscape, modifierFlags: [], action: #selector(leave))
        [tab, backTab, escape].forEach { $0.wantsPriorityOverSystemBehavior = true }
        return [tab, backTab, escape]
    }

    /// Escape leaves the table, whether or not a key command claims it.
    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        guard presses.contains(where: { $0.key?.keyCode == .keyboardEscape }) else {
            return super.pressesBegan(presses, with: event)
        }
        finish(.leave)
    }

    @objc private func nextCell() { finish(.next) }
    @objc private func previousCell() { finish(.previous) }
    @objc private func leave() { finish(.leave) }

    func textFieldShouldReturn(_ textField: UITextField) -> Bool {
        finish(.below)
        return false
    }

    func textFieldDidEndEditing(_ textField: UITextField) {
        finish(.stay)
    }
}
