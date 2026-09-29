import UIKit

/// Hands TextKit 2 a fragment that draws what a paragraph has besides its
/// text: a block's background, bar or rule, pictures above or below it,
/// the grammar checker's underlines and a heading's fold control.
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
        guard let paragraph = textElement as? NSTextParagraph,
              let drawing = FragmentDrawing(paragraph.attributedString)
        else { return NSTextLayoutFragment(textElement: textElement, range: range) }
        return BlockLayoutFragment(textElement: textElement, range: range, drawing: drawing, tokens: tokens)
    }
}

/// What a paragraph's fragment draws besides its text, read from the
/// attributes the styler left on it.
struct FragmentDrawing {
    var decoration: BlockDecoration?
    var extras: FragmentExtras?
    var fold: HeadingFoldMark?
    /// The grammar flags, by where they are in the paragraph.
    var flags: [(NSRange, GrammarMark)] = []

    init?(_ text: NSAttributedString) {
        guard text.length > 0 else { return nil }
        decoration = text.attribute(.blockDecoration, at: 0, effectiveRange: nil) as? BlockDecoration
        extras = text.attribute(.fragmentExtras, at: 0, effectiveRange: nil) as? FragmentExtras
        fold = text.attribute(.headingFold, at: 0, effectiveRange: nil) as? HeadingFoldMark
        text.enumerateAttribute(.grammarFlag, in: NSRange(location: 0, length: text.length)) { value, range, _ in
            if let mark = value as? GrammarMark { flags.append((range, mark)) }
        }
        if decoration == nil, extras == nil, fold == nil, flags.isEmpty { return nil }
    }
}

final class BlockLayoutFragment: NSTextLayoutFragment {
    private let drawing: FragmentDrawing
    private let tokens: Tokens

    init(textElement: NSTextElement, range: NSTextRange?, drawing: FragmentDrawing, tokens: Tokens) {
        self.drawing = drawing
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
        var bounds = super.renderingSurfaceBounds.union(blockBounds)
        if drawing.fold != nil { bounds = bounds.union(foldControlRect()) }
        for (extra, origin) in extraOrigins() {
            bounds = bounds.union(CGRect(origin: origin, size: extra.size))
        }
        return bounds
    }

    override func draw(at point: CGPoint, in context: CGContext) {
        UIGraphicsPushContext(context)
        if let decoration = drawing.decoration {
            decoration.color.setFill()
            drawDecoration(decoration.kind, in: blockBounds.offsetBy(dx: point.x, dy: point.y))
        }
        UIGraphicsPopContext()
        super.draw(at: point, in: context)
        UIGraphicsPushContext(context)
        for (extra, origin) in extraOrigins() {
            extra.draw(at: CGPoint(x: origin.x + point.x, y: origin.y + point.y), in: context)
        }
        drawing.flags.forEach { drawSquiggle(under: $0.0, color: $0.1.color, at: point) }
        if let fold = drawing.fold { drawFoldControl(fold, at: point) }
        UIGraphicsPopContext()
    }

    // MARK: Blocks

    private func drawDecoration(_ kind: BlockDecoration.Kind, in bounds: CGRect) {
        switch kind {
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

    // MARK: Pictures above and below

    /// Each picture with its top-left corner, in the fragment's coordinates.
    private func extraOrigins() -> [(FragmentExtra, CGPoint)] {
        guard let extras = drawing.extras else { return [] }
        let lines = textLineFragments
        var placed: [(FragmentExtra, CGPoint)] = []
        if let above = extras.above {
            let top = lines.first?.typographicBounds.minY ?? 0
            placed.append((above, CGPoint(x: extraLeft(above, extras), y: top - above.gap - above.size.height)))
        }
        if let below = extras.below {
            let bottom = lines.last?.typographicBounds.maxY ?? layoutFragmentFrame.height
            placed.append((below, CGPoint(x: extraLeft(below, extras), y: bottom + below.gap)))
        }
        if let trailing = extras.trailing {
            let top = lines.first?.typographicBounds.minY ?? 0
            let left = blockBounds.maxX - trailing.gap - trailing.size.width
            placed.append((trailing, CGPoint(x: left, y: top + trailing.gap)))
        }
        return placed
    }

    private func extraLeft(_ extra: FragmentExtra, _ extras: FragmentExtras) -> CGFloat {
        let column = blockBounds
        guard extra.centered else { return column.minX + extras.indent }
        return column.midX - extra.size.width / 2
    }

    // MARK: Grammar underlines

    /// A wavy line under the characters in `range` on each line they reach.
    private func drawSquiggle(under range: NSRange, color: UIColor, at point: CGPoint) {
        color.setStroke()
        for line in textLineFragments {
            let covered = NSIntersectionRange(line.characterRange, range)
            guard covered.length > 0 else { continue }
            let start = line.locationForCharacter(at: covered.location).x
            let end = line.locationForCharacter(at: NSMaxRange(covered)).x
            let bounds = line.typographicBounds
            let baseline = bounds.minY + line.glyphOrigin.y
            let origin = CGPoint(x: point.x + bounds.minX + start, y: point.y + baseline + 2.5)
            Squiggle.path(from: origin, width: max(end - start, 4)).stroke()
        }
    }

    // MARK: Folding

    /// Where a heading's fold control sits: in the margin, on its first line.
    private func foldControlRect() -> CGRect {
        let line = textLineFragments.first?.typographicBounds ?? .zero
        let size = CGFloat(tokens.spacing.lg)
        let left = blockBounds.minX - CGFloat(tokens.spacing.xl) + (CGFloat(tokens.spacing.xl) - size) / 2 - 1
        return CGRect(x: left, y: line.midY - size / 2, width: size, height: size)
    }

    private func drawFoldControl(_ fold: HeadingFoldMark, at point: CGPoint) {
        let rect = foldControlRect().offsetBy(dx: point.x, dy: point.y)
        let name = fold.folded ? "chevron.right" : "chevron.down"
        let configuration = UIImage.SymbolConfiguration(pointSize: rect.height * 0.8, weight: .semibold)
        let color = tokens.color(fold.folded ? \.textMuted : \.textFaint)
        let image = UIImage(systemName: name, withConfiguration: configuration)?
            .withTintColor(color, renderingMode: .alwaysOriginal)
        guard let image else { return }
        let origin = CGPoint(x: rect.midX - image.size.width / 2, y: rect.midY - image.size.height / 2)
        image.draw(at: origin)
    }
}

/// The wavy underline the grammar checker draws, as on the desktop.
enum Squiggle {
    static func path(from origin: CGPoint, width: CGFloat) -> UIBezierPath {
        let path = UIBezierPath()
        let step: CGFloat = 2
        let height: CGFloat = 1.2
        path.move(to: origin)
        var drawn: CGFloat = 0
        var rising = true
        while drawn < width {
            let next = min(drawn + step, width)
            let control = CGPoint(x: origin.x + (drawn + next) / 2, y: origin.y + (rising ? -height : height) * 2)
            path.addQuadCurve(to: CGPoint(x: origin.x + next, y: origin.y), controlPoint: control)
            drawn = next
            rising.toggle()
        }
        path.lineWidth = 1
        return path
    }
}
