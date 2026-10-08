import SwiftUI
import UIKit

/// The website's hero on the phone. A finger dragged over the water rings
/// it, and the name's glyphs lean toward a finger and part around it up
/// close. With Reduce Motion everything holds still.
struct LiquidHero: View {
    let tokens: Tokens
    let isShowing: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var motion = LiquidMotion(glyphCount: LiquidHeroMetrics.word.count)

    var body: some View {
        GeometryReader { proxy in
            let layout = LiquidHeroLayout(size: proxy.size, tokens: tokens)
            ZStack(alignment: .topLeading) {
                TimelineView(.animation(minimumInterval: nil, paused: reduceMotion || !isShowing)) { context in
                    LiquidScene(
                        layout: layout, colors: HeroColors(tokens: tokens), motion: motion,
                        time: reduceMotion ? 0 : motion.advance(
                            to: context.date, homes: layout.glyphHomes, size: layout.wordSize
                        ),
                        still: reduceMotion
                    )
                }
                if !reduceMotion {
                    FingerTrail { point, phase in motion.touch(point, phase: phase, sea: layout.seaFrame) }
                        .accessibilityHidden(true)
                }
                LedeLine(layout: layout, tokens: tokens)
                    .offset(x: layout.margin, y: layout.ledeTop)
            }
            .frame(width: proxy.size.width, height: proxy.size.height, alignment: .topLeading)
            .task(id: isShowing) { await greet(layout) }
        }
        .clipped()
    }

    private func greet(_ layout: LiquidHeroLayout) async {
        guard isShowing, !reduceMotion else { return }
        try? await Task.sleep(for: .milliseconds(700))
        guard !Task.isCancelled else { return }
        motion.greet(at: layout.whaleHead)
    }
}

/// The hero's colours, read once rather than every frame.
private struct HeroColors {
    let ink: Color
    let page: Color
    let sea: Color

    init(tokens: Tokens) {
        ink = tokens.swiftUIColor(\.textStrong)
        page = tokens.swiftUIColor(\.background)
        sea = tokens.swiftUIColor(\.divider)
    }
}

/// One frame of the hero at `time`: the soft name over the sea.
private struct LiquidScene: View {
    let layout: LiquidHeroLayout
    let colors: HeroColors
    let motion: LiquidMotion
    let time: Double
    let still: Bool

    var body: some View {
        ZStack(alignment: .topLeading) {
            sea.offset(y: layout.seaTop)
            wordmark.offset(y: layout.wordBoxTop)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private var wordmark: some View {
        ZStack(alignment: .topLeading) {
            ForEach(layout.glyphs.indices, id: \.self, content: glyph)
        }
        .frame(width: layout.width, height: layout.wordBoxHeight, alignment: .topLeading)
        .layerEffect(
            ShaderLibrary.softGlyph(.float(time), .float(glyphWater), .float(layout.glyphSoftness)),
            maxSampleOffset: layout.glyphReach
        )
        .accessibilityElement()
        .accessibilityLabel(LiquidHeroMetrics.word)
        .accessibilityAddTraits(.isHeader)
    }

    private func glyph(_ index: Int) -> some View {
        let body = motion.glyphs[index]
        let stretch = still ? 1 : body.stretch(at: time)
        let bob = still ? 0 : body.bob(at: time, size: layout.wordSize)
        return Text(layout.glyphs[index])
            .font(Font(layout.wordFont))
            .foregroundStyle(colors.ink)
            .fixedSize()
            .rotationEffect(.radians(-body.angle))
            .scaleEffect(x: stretch, y: 1 / stretch)
            .rotationEffect(.radians(body.angle))
            .position(x: layout.glyphCentres[index], y: layout.glyphMiddle)
            .offset(x: body.offset.width, y: body.offset.height + bob)
    }

    /// How much the water bends the name: a little always, more while its
    /// glyphs are unsettled.
    private var glyphWater: CGFloat {
        let share = min(LiquidHeroMetrics.mostGlyphWater, LiquidHeroMetrics.calmGlyphWater + motion.stirred)
        return layout.wordSize * share
    }

    private var sea: some View {
        ZStack(alignment: .topLeading) {
            whale
                .frame(width: layout.width, height: layout.seaHeight, alignment: .topLeading)
                .distortionEffect(water(LiquidHeroMetrics.waterStrength), maxSampleOffset: LiquidHeroMetrics.waterReach)
                .overlay(colors.page.opacity(LiquidHeroMetrics.whaleDepth))
                .mask(fadingDown)
            lines
                .frame(width: layout.width, height: layout.seaHeight, alignment: .topLeading)
                .distortionEffect(
                    water(LiquidHeroMetrics.waterStrength * LiquidHeroMetrics.lineBend),
                    maxSampleOffset: LiquidHeroMetrics.waterReach
                )
        }
        .accessibilityHidden(true)
    }

    private var whale: some View {
        let glide = sin(2 * .pi * time / LiquidHeroMetrics.glidePeriod)
        let bob = sin(2 * .pi * time / LiquidHeroMetrics.bobPeriod)
        return Image("WhaleGlide")
            .resizable()
            .frame(width: layout.whaleWidth, height: layout.whaleHeight)
            .offset(
                x: layout.whaleLeft + layout.whaleWidth * LiquidHeroMetrics.glideReach * glide,
                y: layout.whaleTop + layout.whaleWidth * LiquidHeroMetrics.bobReach * bob
            )
    }

    private var lines: some View {
        ZStack(alignment: .topLeading) {
            ForEach(LiquidHeroMetrics.seaLines.indices, id: \.self) { index in
                Capsule()
                    .fill(colors.sea)
                    .frame(width: layout.content * LiquidHeroMetrics.seaLines[index], height: layout.lineThickness)
                    .offset(x: layout.margin, y: layout.lineTop(index))
            }
        }
    }

    private var fadingDown: LinearGradient {
        LinearGradient(
            stops: [.init(color: .black, location: 0.55), .init(color: .clear, location: 1)],
            startPoint: .top, endPoint: .bottom
        )
    }

    private func water(_ strength: CGFloat) -> Shader {
        ShaderLibrary.waterDistortion(
            .float(time), .float(strength),
            .float2(LiquidHeroMetrics.waterFrequency.across, LiquidHeroMetrics.waterFrequency.down),
            motion.ring(0, at: time), motion.ring(1, at: time), motion.ring(2, at: time), motion.ring(3, at: time)
        )
    }
}

/// The one sentence about the app, ending in the icon's red caret where
/// the next word would go.
private struct LedeLine: View {
    let layout: LiquidHeroLayout
    let tokens: Tokens
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TimelineView(.periodic(from: .now, by: LiquidHeroMetrics.caretBlink)) { context in
            Text(LiquidHeroMetrics.lede)
                + Text("\u{2009}")
                + Text(CaretGlyph.image(for: layout.ledeFont))
                    .foregroundStyle(caretColor(at: context.date))
                    .baselineOffset(-layout.ledeFont.pointSize * 0.18)
        }
        .font(Font(layout.ledeFont))
        .foregroundStyle(tokens.swiftUIColor(\.textStrong))
        .frame(width: layout.ledeWidth, alignment: .leading)
        .fixedSize(horizontal: false, vertical: true)
        .accessibilityElement()
        .accessibilityLabel(LiquidHeroMetrics.lede)
    }

    private func caretColor(at date: Date) -> Color {
        let beat = Int(date.timeIntervalSinceReferenceDate / LiquidHeroMetrics.caretBlink)
        return reduceMotion || beat.isMultiple(of: 2) ? tokens.swiftUIColor(\.caretMark) : .clear
    }
}

/// The caret drawn as a rounded bar, made once for each text size, so it
/// can sit inside a line of text.
private enum CaretGlyph {
    private static var cache: [CGFloat: Image] = [:]

    static func image(for font: UIFont) -> Image {
        if let image = cache[font.pointSize] { return image }
        let size = CGSize(width: max(3, font.pointSize * 0.14), height: font.pointSize * 1.05)
        let bar = UIGraphicsImageRenderer(size: size).image { _ in
            UIBezierPath(roundedRect: CGRect(origin: .zero, size: size), cornerRadius: size.width / 2).fill()
        }
        let image = Image(uiImage: bar.withRenderingMode(.alwaysTemplate))
        cache[font.pointSize] = image
        return image
    }
}
