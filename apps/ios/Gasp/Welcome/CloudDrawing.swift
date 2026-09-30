import SwiftUI

/// A cloud in ink, as the iPhone and the Mac are drawn: a solid outline
/// around the page's colour, holding whatever sits inside it.
struct CloudDrawing<Inside: View>: View {
    let tokens: Tokens
    @ViewBuilder let inside: () -> Inside

    var body: some View {
        GeometryReader { proxy in
            let outline = min(proxy.size.width / CloudShape.aspect, proxy.size.height) * 0.07
            ZStack {
                CloudShape().fill(tokens.swiftUIColor(\.textStrong))
                CloudShape(inset: outline).fill(tokens.swiftUIColor(\.background))
                inside().offset(y: proxy.size.height * 0.12)
            }
        }
        .aspectRatio(CloudShape.aspect, contentMode: .fit)
        .accessibilityHidden(true)
    }
}

/// A cloud as a flat-bottomed band with three round tops. Each part is a
/// circle or a capsule, so shrinking every one by `inset` gives the same
/// cloud drawn smaller by exactly that much all round.
struct CloudShape: Shape {
    /// How much wider the cloud is than tall.
    static let aspect: CGFloat = 1.6
    var inset: CGFloat = 0

    private static let tops: [(x: CGFloat, y: CGFloat, radius: CGFloat)] = [
        (0.5, 0.56, 0.27), (0.92, 0.4, 0.36), (1.24, 0.6, 0.22)
    ]

    func path(in rect: CGRect) -> Path {
        let scale = min(rect.width / Self.aspect, rect.height)
        let origin = CGPoint(x: rect.midX - scale * Self.aspect / 2, y: rect.midY - scale / 2)
        func point(_ x: CGFloat, _ y: CGFloat) -> CGPoint {
            CGPoint(x: origin.x + x * scale, y: origin.y + y * scale)
        }
        var path = Path()
        let base = CGRect(origin: point(0.1, 0.5), size: CGSize(width: 1.4 * scale, height: 0.46 * scale))
            .insetBy(dx: inset, dy: inset)
        path.addRoundedRect(in: base, cornerSize: CGSize(width: base.height / 2, height: base.height / 2))
        for top in Self.tops {
            let radius = top.radius * scale - inset
            let centre = point(top.x, top.y)
            path.addEllipse(in: CGRect(x: centre.x - radius, y: centre.y - radius, width: radius * 2, height: radius * 2))
        }
        return path
    }
}
