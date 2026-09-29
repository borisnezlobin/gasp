import UIKit

/// A picture drawn in room a line makes above or below itself, such as the
/// live math preview above an equation being edited, or an image below its
/// revealed embed. The styler makes the room with paragraph spacing and
/// the line's layout fragment draws into it.
final class FragmentExtra: NSObject {
    enum Place {
        case above
        case below
    }

    enum Content {
        case math(RenderedMath, color: UIColor)
        case image(ImageAttachment)
    }

    let place: Place
    let content: Content
    let centered: Bool
    /// The room kept between the picture and the line.
    let gap: CGFloat

    init(place: Place, content: Content, centered: Bool, gap: CGFloat) {
        self.place = place
        self.content = content
        self.centered = centered
        self.gap = gap
    }

    var size: CGSize {
        switch content {
        case .math(let math, _): math.size
        case .image(let attachment): attachment.bounds.size
        }
    }

    /// The paragraph spacing the line needs for the picture.
    var room: CGFloat {
        size.height + gap * 2
    }

    /// Draws the picture with its top-left corner at `origin`.
    func draw(at origin: CGPoint, in context: CGContext) {
        let rect = CGRect(origin: origin, size: size)
        switch content {
        case .math(let math, let color):
            math.draw(in: rect, color: color.resolvedColor(with: .current), context: context)
        case .image(let attachment):
            let image = attachment.image(
                for: rect, attributes: [:], location: PlainLocation(), textContainer: nil
            )
            image?.draw(in: rect)
        }
    }
}
