import SwiftUI
import UIKit

/// The first step: the app icon made large. The name sits on a paragraph
/// whose first line is the one sentence about the app, and the whale
/// breaches out of the paragraph beside it, rising in front of the end of
/// the name. It rises as the step appears and leaps when tapped.
struct HelloStep: View {
    let tokens: Tokens
    let onward: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            GeometryReader { proxy in
                BreachingParagraph(layout: HelloLayout(size: proxy.size, tokens: tokens), tokens: tokens)
            }
            WelcomeFooter(primary: "Show me around", tokens: tokens, action: onward)
        }
    }
}

/// Where everything on the first step goes, worked out from the room it
/// has and the tagline's height at the reader's text size.
struct HelloLayout {
    static let name = "Gasp"
    static let tagline = "Plain Markdown notes on your Mac and iPhone."
    /// How much of the paragraph's width each line under the tagline takes.
    static let lowerLines: [CGFloat] = [0.96, 0.84, 0.58]
    /// How much of the whale stays under the surface once it's up.
    static let underSurface: CGFloat = 0.34
    /// Where the whale's body crosses the surface, as a share of its width
    /// from its left edge: the drawing leans, tail low and head high.
    static let crossing: CGFloat = 0.24
    /// How high a tap makes the whale leap.
    static let leap: CGFloat = 36

    let margin = WelcomeMetrics.margin
    let nameSize: CGFloat
    let taglineFont: UIFont
    let taglineWidth: CGFloat
    let taglineHeight: CGFloat
    let paragraphWidth: CGFloat
    let whaleWidth: CGFloat
    let pitch: CGFloat
    let nameTop: CGFloat
    let taglineTop: CGFloat

    init(size: CGSize, tokens: Tokens) {
        let content = max(size.width - WelcomeMetrics.margin * 2, 1)
        paragraphWidth = content
        nameSize = min(tokens.bodySize * 4.4, 96)
        taglineFont = tokens.uiFont(size: tokens.bodySize * 1.05)
        taglineWidth = content * 0.56
        taglineHeight = ceil(
            (Self.tagline as NSString).boundingRect(
                with: CGSize(width: taglineWidth, height: .greatestFiniteMagnitude),
                options: [.usesLineFragmentOrigin, .usesFontLeading],
                attributes: [.font: taglineFont], context: nil
            ).height
        )
        whaleWidth = min(content * 0.58, 300)
        pitch = taglineFont.lineHeight * 1.25
        let whaleHeight = whaleWidth / WhaleArt.breachAspect
        let nameHeight = nameSize * 1.1
        let aboveSurface = max(whaleHeight * (1 - Self.underSurface) + Self.leap, nameHeight + taglineHeight)
        let belowSurface = pitch * CGFloat(Self.lowerLines.count + 1)
        let surfaceFromTop = max((size.height - aboveSurface - belowSurface) * 0.6, 0) + aboveSurface
        taglineTop = surfaceFromTop - taglineHeight + taglineFont.lineHeight / 2
        nameTop = taglineTop - nameHeight - 4
    }

    var whaleHeight: CGFloat { whaleWidth / WhaleArt.breachAspect }
    var nameHeight: CGFloat { nameSize * 1.1 }

    /// The middle of the tagline's last line, where the whale crosses it.
    var surface: CGFloat { taglineTop + taglineHeight - taglineFont.lineHeight / 2 }

    var whaleLeft: CGFloat {
        margin + taglineWidth + 12 - whaleWidth * Self.crossing
    }

    var whaleRestingTop: CGFloat {
        surface - whaleHeight * (1 - Self.underSurface)
    }

    func lineTop(_ index: Int) -> CGFloat {
        taglineTop + taglineHeight + pitch * CGFloat(index) + (pitch - lineThickness) / 2
    }

    var lineThickness: CGFloat { taglineFont.lineHeight * 0.32 }
}

private struct BreachingParagraph: View {
    let layout: HelloLayout
    let tokens: Tokens
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var risen = false
    @State private var leaping = false

    private static let underwater = 0.62

    var body: some View {
        ZStack(alignment: .topLeading) {
            name
            whale
            tokens.swiftUIColor(\.background)
                .opacity(Self.underwater)
                .padding(.top, layout.surface + layout.taglineFont.lineHeight / 2)
                .allowsHitTesting(false)
            Text(HelloLayout.tagline)
                .font(Font(layout.taglineFont))
                .foregroundStyle(tokens.swiftUIColor(\.textMuted))
                .frame(width: layout.taglineWidth, alignment: .leading)
                .fixedSize(horizontal: false, vertical: true)
                .offset(x: layout.margin, y: layout.taglineTop)
                .allowsHitTesting(false)
            ForEach(Array(HelloLayout.lowerLines.enumerated()), id: \.offset) { index, share in
                Capsule()
                    .fill(tokens.swiftUIColor(\.fillStrong))
                    .frame(width: layout.paragraphWidth * share, height: layout.lineThickness)
                    .offset(x: layout.margin, y: layout.lineTop(index))
                    .accessibilityHidden(true)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .clipped()
        .onAppear(perform: rise)
    }

    private var name: some View {
        HStack(alignment: .center, spacing: layout.nameSize * 0.05) {
            Text(HelloLayout.name)
                .font(Font(tokens.uiFont(size: layout.nameSize, bold: true)))
                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                .accessibilityAddTraits(.isHeader)
            CaretMark(width: max(layout.nameSize * 0.045, 3), height: layout.nameSize * 0.78, tokens: tokens)
        }
        .frame(height: layout.nameHeight)
        .offset(x: layout.margin, y: layout.nameTop)
    }

    private var whale: some View {
        Image("WhaleBreach")
            .resizable()
            .frame(width: layout.whaleWidth, height: layout.whaleHeight)
            .offset(x: layout.whaleLeft, y: whaleTop)
            .onTapGesture(perform: leap)
            .accessibilityLabel("A humpback whale breaching")
            .accessibilityAddTraits(.isImage)
    }

    private var whaleTop: CGFloat {
        let sunk = risen ? 0 : layout.whaleHeight * 0.8
        let lifted = leaping ? HelloLayout.leap : 0
        return layout.whaleRestingTop + sunk - lifted
    }

    private func rise() {
        guard !risen else { return }
        guard !reduceMotion else {
            risen = true
            return
        }
        withAnimation(.easeOut(duration: 1.4)) { risen = true }
    }

    private func leap() {
        guard !reduceMotion, !leaping else { return }
        UIImpactFeedbackGenerator(style: .soft).impactOccurred()
        withAnimation(.easeOut(duration: 0.35)) { leaping = true }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) {
            withAnimation(.spring(duration: 0.6, bounce: 0.35)) { leaping = false }
        }
    }
}
