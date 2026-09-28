import UIKit

/// Hands TextKit 2 a fragment that draws a block's background, bar or rule
/// for paragraphs the styler marked with a `BlockDecoration`.
final class BlockFragments: NSObject, NSTextLayoutManagerDelegate {
    let tokens: Tokens

    init(tokens: Tokens) {
        self.tokens = tokens
    }

    func textLayoutManager(
        _ textLayoutManager: NSTextLayoutManager,
        textLayoutFragmentFor location: NSTextLocation,
        in textElement: NSTextElement
    ) -> NSTextLayoutFragment {
        let range = textElement.elementRange
        guard let paragraph = textElement as? NSTextParagraph, paragraph.attributedString.length > 0,
              let decoration = paragraph.attributedString.attribute(.blockDecoration, at: 0, effectiveRange: nil)
                as? BlockDecoration
        else { return NSTextLayoutFragment(textElement: textElement, range: range) }
        return BlockLayoutFragment(textElement: textElement, range: range, decoration: decoration, tokens: tokens)
    }
}

final class BlockLayoutFragment: NSTextLayoutFragment {
    private let decoration: BlockDecoration
    private let tokens: Tokens

    init(textElement: NSTextElement, range: NSTextRange?, decoration: BlockDecoration, tokens: Tokens) {
        self.decoration = decoration
        self.tokens = tokens
        super.init(textElement: textElement, range: range)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("Fragments aren't decoded")
    }

    /// The whole width of the text column, so backgrounds reach both edges
    /// whatever the line's indent and length.
    private var blockBounds: CGRect {
        let frame = layoutFragmentFrame
        let columnWidth = textLayoutManager?.textContainer?.size.width ?? frame.maxX
        return CGRect(x: -frame.minX, y: 0, width: columnWidth, height: frame.height)
    }

    override var renderingSurfaceBounds: CGRect {
        super.renderingSurfaceBounds.union(blockBounds)
    }

    override func draw(at point: CGPoint, in context: CGContext) {
        let bounds = blockBounds.offsetBy(dx: point.x, dy: point.y)
        UIGraphicsPushContext(context)
        decoration.color.setFill()
        drawDecoration(in: bounds)
        UIGraphicsPopContext()
        super.draw(at: point, in: context)
    }

    private func drawDecoration(in bounds: CGRect) {
        switch decoration.kind {
        case .code(let first, let last), .callout(let first, let last):
            fillBlock(bounds, roundTop: first, roundBottom: last)
        case .quote(let depth):
            drawQuoteBars(bounds, depth: depth)
        case .rule:
            UIRectFill(CGRect(x: bounds.minX, y: bounds.midY, width: bounds.width, height: hairline))
        case .tableRow:
            UIRectFill(CGRect(x: bounds.minX, y: bounds.maxY - hairline, width: bounds.width, height: hairline))
        }
    }

    private var hairline: CGFloat {
        1 / UIScreen.main.scale
    }

    private func fillBlock(_ bounds: CGRect, roundTop: Bool, roundBottom: Bool) {
        var corners: UIRectCorner = []
        if roundTop { corners.formUnion([.topLeft, .topRight]) }
        if roundBottom { corners.formUnion([.bottomLeft, .bottomRight]) }
        let radius = CGFloat(tokens.spacing.radiusMd)
        let radii = CGSize(width: radius, height: radius)
        UIBezierPath(roundedRect: bounds, byRoundingCorners: corners, cornerRadii: radii).fill()
    }

    private func drawQuoteBars(_ bounds: CGRect, depth: Int) {
        let step = CGFloat(tokens.spacing.lg)
        let width = CGFloat(tokens.spacing.xs)
        for level in 0..<depth {
            let left = bounds.minX + CGFloat(level) * step
            UIRectFill(CGRect(x: left, y: bounds.minY, width: width, height: bounds.height))
        }
    }
}
