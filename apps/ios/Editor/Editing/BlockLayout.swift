import UIKit

/// Math blocks drawn as their TeX until the phone renders math: the fence
/// lines take no room and the lines between them show centred.
struct MathBlockFallback {
    private var presentations: [Int: LinePresentation] = [:]

    init(plan: NotePlan) {
        for (index, line) in plan.lines.enumerated() {
            for widget in line.widgets {
                guard case .mathBlock = widget.kind else { continue }
                markLines(of: widget.range, from: index, in: plan)
            }
        }
    }

    func presentation(of index: Int) -> LinePresentation {
        presentations[index] ?? .asPlanned
    }

    private mutating func markLines(of range: TextRange, from first: Int, in plan: NotePlan) {
        let covered = plan.lines[first...].prefix { $0.range.start < range.end }
        guard covered.count > 1 else { return }
        for index in covered.indices {
            let isFence = index == covered.startIndex || index == covered.endIndex - 1
            presentations[index] = isFence ? .collapsed : .mathSource
        }
    }
}

/// The decoration a line's layout fragment draws, which depends on whether
/// the lines around it belong to the same block.
struct BlockNeighbours {
    let plan: NotePlan
    let index: Int

    func decoration(tokens: Tokens) -> BlockDecoration? {
        let line = plan.lines[index]
        let shape = LineShape(line.decorations)
        if line.widgets.contains(where: { $0.kind == .horizontalRule }) {
            return BlockDecoration(.rule, color: tokens.color(\.divider))
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

    private func visibleLine(_ index: Int) -> LinePlan? {
        guard plan.lines.indices.contains(index), !plan.lines[index].collapsed else { return nil }
        return plan.lines[index]
    }

    private func isCode(_ index: Int) -> Bool {
        visibleLine(index).map { LineShape($0.decorations).isCode } ?? false
    }

    private func calloutKindAt(_ index: Int) -> String? {
        visibleLine(index).flatMap { LineShape($0.decorations).calloutKind }
    }
}
