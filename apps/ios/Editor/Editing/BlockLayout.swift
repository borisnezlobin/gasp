import UIKit

/// Blocks the phone draws from their source until it has their widgets:
/// a math block shows its TeX centred until its render is ready, and a
/// link card shows its title, description and address. Their fence lines
/// take no room.
struct BlockFallbacks {
    private var presentations: [Int: LinePresentation] = [:]

    /// `isRendered` says whether a `$$` block's TeX is ready to draw.
    init(plan: NotePlan, text: NSString, isRendered: (String) -> Bool = { _ in false }) {
        for (index, line) in plan.lines.enumerated() {
            for widget in line.widgets {
                switch widget.kind {
                case .mathBlock(let tex) where !isRendered(tex):
                    mark(widget.range, from: index, in: plan) { _ in .mathSource }
                case .linkCard: mark(widget.range, from: index, in: plan) { Self.cardField(of: $0, in: text) }
                default: continue
                }
            }
        }
    }

    func presentation(of index: Int) -> LinePresentation {
        presentations[index] ?? .asPlanned
    }

    /// Whether the line takes room, whatever the plan said.
    func isShown(_ index: Int, in plan: NotePlan) -> Bool {
        switch presentation(of: index) {
        case .asPlanned: !plan.lines[index].collapsed
        case .collapsed: false
        default: true
        }
    }

    func isCardLine(_ index: Int) -> Bool {
        if case .cardField = presentation(of: index) { return true }
        return false
    }

    private mutating func mark(
        _ range: TextRange, from first: Int, in plan: NotePlan, inside: (LinePlan) -> LinePresentation
    ) {
        let covered = plan.lines[first...].prefix { $0.range.start < range.end }
        guard covered.count > 1 else { return }
        for index in covered.indices {
            let isFence = index == covered.startIndex || index == covered.endIndex - 1
            presentations[index] = isFence ? .collapsed : inside(plan.lines[index])
        }
    }

    private static let cardFields: [String: CardField] = [
        "title": .title, "description": .detail, "url": .address
    ]

    /// `title: "…"` shows its value as the card's title, and so on; the
    /// image and icon lines take no room.
    private static func cardField(of line: LinePlan, in text: NSString) -> LinePresentation {
        let source = text.substring(with: line.range.nsRange)
        let key = source.split(separator: ":", maxSplits: 1).first.map { $0.trimmingCharacters(in: .whitespaces) }
        guard let key, let field = cardFields[key] else { return .collapsed }
        return .cardField(field)
    }
}

/// The decoration a line's layout fragment draws, which depends on whether
/// the lines around it belong to the same block.
struct BlockNeighbours {
    let plan: NotePlan
    let fallbacks: BlockFallbacks
    let index: Int

    func decoration(tokens: Tokens) -> BlockDecoration? {
        let line = plan.lines[index]
        let shape = LineShape(line.decorations)
        if line.widgets.contains(where: { $0.kind == .horizontalRule }) {
            return BlockDecoration(.rule, color: tokens.color(\.divider))
        }
        if fallbacks.isCardLine(index) {
            let kind = BlockDecoration.Kind.code(first: !isCardNeighbour(step: -1), last: !isCardNeighbour(step: 1))
            return BlockDecoration(kind, color: tokens.color(\.card))
        }
        if shape.isCode {
            let kind = BlockDecoration.Kind.code(first: !isCode(index - 1), last: !isCode(index + 1))
            return BlockDecoration(kind, color: tokens.color(\.codeBackground))
        }
        if let calloutKind = shape.calloutKind {
            let kind = BlockDecoration.Kind.callout(
                first: calloutKindAt(index - 1) != calloutKind,
                last: calloutKindAt(index + 1) != calloutKind
            )
            return BlockDecoration(kind, color: tokens.calloutFill(calloutKind))
        }
        if shape.quoteDepth > 0 {
            return BlockDecoration(.quote(depth: shape.quoteDepth), color: tokens.color(\.divider))
        }
        if line.tableRow?.index == 0 {
            return BlockDecoration(.tableRow(header: true), color: tokens.color(\.divider))
        }
        return nil
    }

    /// Whether the nearest line that takes room, `step` away, is part of
    /// a card too.
    private func isCardNeighbour(step: Int) -> Bool {
        var neighbour = index + step
        while plan.lines.indices.contains(neighbour), !fallbacks.isShown(neighbour, in: plan) {
            neighbour += step
        }
        return plan.lines.indices.contains(neighbour) && fallbacks.isCardLine(neighbour)
    }

    private func shownLine(_ index: Int) -> LinePlan? {
        guard plan.lines.indices.contains(index), fallbacks.isShown(index, in: plan) else { return nil }
        return plan.lines[index]
    }

    private func isCode(_ index: Int) -> Bool {
        shownLine(index).map { LineShape($0.decorations).isCode } ?? false
    }

    private func calloutKindAt(_ index: Int) -> String? {
        shownLine(index).flatMap { LineShape($0.decorations).calloutKind }
    }
}
