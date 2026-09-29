import UIKit

/// Blocks the phone draws from their source until it has their widgets:
/// a math block shows its TeX centred until its render is ready, and a
/// link card shows its title, description and address. Their fence lines
/// take no room.
struct BlockFallbacks {
    private var presentations: [Int: LinePresentation] = [:]
    /// Each card line's preview image, by line, and whether the line
    /// draws it: the card's first line does.
    private(set) var cardImages: [Int: CardImage] = [:]

    /// `isRendered` says whether a `$$` block's TeX is ready to draw.
    init(plan: ShownPlan, text: NSString, isRendered: (String) -> Bool = { _ in false }) {
        for index in plan.blockLines {
            for widget in plan[index].widgets {
                switch widget.kind {
                case .mathBlock(let tex) where !isRendered(tex):
                    mark(widget.range, from: index, in: plan) { _ in .mathSource }
                case .linkCard:
                    mark(widget.range, from: index, in: plan) { Self.cardField(of: $0, in: text) }
                    noteCardImage(widget.range, from: index, in: plan, text: text)
                default: continue
                }
            }
        }
    }

    func presentation(of index: Int) -> LinePresentation {
        presentations[index] ?? .asPlanned
    }

    /// Whether the line takes room, whatever the plan said.
    func isShown(_ index: Int, in plan: ShownPlan) -> Bool {
        switch presentation(of: index) {
        case .asPlanned: !plan.unmoved(index).collapsed
        case .collapsed: false
        default: true
        }
    }

    func isCardLine(_ index: Int) -> Bool {
        if case .cardField = presentation(of: index) { return true }
        return false
    }

    private mutating func mark(
        _ range: TextRange, from first: Int, in plan: ShownPlan, inside: (LinePlan) -> LinePresentation
    ) {
        let covered = Self.lines(of: range, from: first, in: plan)
        guard covered.count > 1 else { return }
        for index in covered {
            let isFence = index == covered.lowerBound || index == covered.upperBound - 1
            presentations[index] = isFence ? .collapsed : inside(plan[index])
        }
    }

    /// The lines from `first` that start before `range` ends.
    private static func lines(of range: TextRange, from first: Int, in plan: ShownPlan) -> Range<Int> {
        var end = first
        while end < plan.count, plan.start(of: end) < Int(range.end) { end += 1 }
        return first..<end
    }

    /// Finds the card's `image:` line and gives its address to the
    /// card's shown lines.
    private mutating func noteCardImage(_ range: TextRange, from first: Int, in plan: ShownPlan, text: NSString) {
        let covered = Self.lines(of: range, from: first, in: plan)
        let address = covered.lazy.compactMap { Self.value(of: "image", in: plan[$0], text: text) }.first
        guard let address, let url = URL(string: address), url.scheme?.hasPrefix("http") == true else { return }
        let shown = covered.indices.filter(isCardLine)
        for index in shown {
            cardImages[index] = CardImage(url: url, drawsIt: index == shown.first)
        }
    }

    /// The quoted value of `key: "…"` on a card's line.
    private static func value(of key: String, in line: LinePlan, text: NSString) -> String? {
        let source = text.substring(with: line.range.nsRange)
        let parts = source.split(separator: ":", maxSplits: 1)
        guard parts.count == 2, parts[0].trimmingCharacters(in: .whitespaces) == key else { return nil }
        let quotes = CharacterSet(charactersIn: "\"")
        let value = parts[1].trimmingCharacters(in: .whitespaces).trimmingCharacters(in: quotes)
        return value.isEmpty ? nil : value
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

/// A link card's preview image, as a line of the card sees it.
struct CardImage {
    let url: URL
    let drawsIt: Bool
}

/// The decoration a line's layout fragment draws, which depends on whether
/// the lines around it belong to the same block.
struct BlockNeighbours {
    let plan: ShownPlan
    let fallbacks: BlockFallbacks
    let index: Int

    func decoration(tokens: Tokens) -> BlockDecoration? {
        let line = plan.unmoved(index)
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
        while plan.indices.contains(neighbour), !fallbacks.isShown(neighbour, in: plan) {
            neighbour += step
        }
        return plan.indices.contains(neighbour) && fallbacks.isCardLine(neighbour)
    }

    private func shownLine(_ index: Int) -> LinePlan? {
        guard plan.indices.contains(index), fallbacks.isShown(index, in: plan) else { return nil }
        return plan.unmoved(index)
    }

    private func isCode(_ index: Int) -> Bool {
        shownLine(index).map { LineShape($0.decorations).isCode } ?? false
    }

    private func calloutKindAt(_ index: Int) -> String? {
        shownLine(index).flatMap { LineShape($0.decorations).calloutKind }
    }
}
