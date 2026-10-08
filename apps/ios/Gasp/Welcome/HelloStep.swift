import SwiftUI
import UIKit

/// The first step, made like the website's homepage: the name set huge in
/// soft glyphs, the lines of a note with a humpback gliding beneath them
/// seen through moving water, and the one sentence about the app ending
/// in the icon's red caret.
struct HelloStep: View {
    let tokens: Tokens
    let isShowing: Bool
    let onward: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            LiquidHero(tokens: tokens, isShowing: isShowing)
            WelcomeFooter(primary: "Show me around", tokens: tokens, action: onward)
        }
    }
}

/// The first step's words, proportions and motion, in one place.
enum LiquidHeroMetrics {
    static let word = "Gasp"
    static let lede = "A Markdown editor optimized for speed and efficiency."
    /// Each note line's length, as a share of the content's width.
    static let seaLines: [CGFloat] = [1, 0.96, 0.88, 0.64, 0.92, 0.4]
    /// The whale art is 1000 by 353.
    static let whaleHeightForWidth: CGFloat = 0.353
    static let whaleMostWidth: CGFloat = 460
    /// How much of the screen's width the whale takes, swimming out past
    /// the lines' ends.
    static let whaleShareOfWidth: CGFloat = 0.96
    /// How much of the hero's height the whale may be wide.
    static let whaleShareOfHeight: CGFloat = 0.62
    /// Where the whale's middle sits across the lines.
    static let whaleCross: CGFloat = 0.46
    /// How far below the first line the whale's back is, in whale widths.
    static let whaleDive: CGFloat = 0.07
    /// How much of the page lies over the whale.
    static let whaleDepth: Double = 0.65
    /// One slow swim from side to side and back, in seconds, and how far
    /// either way it goes, in whale widths.
    static let glidePeriod = 28.0
    static let glideReach: CGFloat = 0.05
    static let bobPeriod = 11.0
    static let bobReach: CGFloat = 0.012
    /// The distance between one note line's centre and the next, and the
    /// lines' thickness, in content widths and pitches.
    static let pitchForContent: CGFloat = 0.08
    static let thicknessForPitch: CGFloat = 0.36
    /// Room above the first line for a ring to lift it.
    static let liftRoom: CGFloat = 10
    /// The most the water bends the whale, in points, and how stretched
    /// its waves are: long across, short down.
    static let waterStrength: CGFloat = 5
    static let waterFrequency = (across: 0.008, down: 0.05)
    /// How much of the water's bend reaches the lines over the whale.
    static let lineBend: CGFloat = 0.3
    static let waterReach = CGSize(width: 24, height: 24)
    static let ringSlots = 4
    /// How far a dragging finger goes before it leaves another ring.
    static let trailSpacing: CGFloat = 48
    /// How much of the content's width and the hero's height the name may
    /// take.
    static let wordShareOfWidth: CGFloat = 0.94
    static let wordShareOfHeight: CGFloat = 0.26
    /// How blurred, then sharpened, the glyphs are, in their own sizes.
    static let glyphSoftness: CGFloat = 0.016
    /// How strongly still and stirred water bends the glyphs, in their own
    /// sizes.
    static let calmGlyphWater: CGFloat = 0.015
    static let mostGlyphWater: CGFloat = 0.055
    static let ledeScale: CGFloat = 1.2
    static let ledeMostWidth: CGFloat = 420
    static let caretBlink = 0.53
}

/// Where everything on the first step goes, worked out once for the room
/// it has and the reader's text size.
struct LiquidHeroLayout {
    let width: CGFloat
    let margin = WelcomeMetrics.margin
    let content: CGFloat
    let wordFont: UIFont
    let glyphs: [String]
    /// Each glyph's centre across the hero.
    let glyphCentres: [CGFloat]
    /// Each glyph's centre in the hero, where its spring pulls it back to.
    let glyphHomes: [CGPoint]
    /// Room around the name for its glyphs to move and soften into.
    let wordPad: CGFloat
    let wordBoxTop: CGFloat
    let whaleWidth: CGFloat
    let pitch: CGFloat
    let seaTop: CGFloat
    let seaHeight: CGFloat
    let ledeFont: UIFont
    let ledeWidth: CGFloat
    let ledeTop: CGFloat

    init(size: CGSize, tokens: Tokens) {
        width = size.width
        content = max(size.width - WelcomeMetrics.margin * 2, 1)
        wordFont = Self.wordFont(fitting: CGSize(width: content, height: size.height), tokens: tokens)
        glyphs = LiquidHeroMetrics.word.map(String.init)
        glyphCentres = Self.centres(of: LiquidHeroMetrics.word, font: wordFont, from: WelcomeMetrics.margin)
        wordPad = wordFont.pointSize * 0.12
        whaleWidth = min(
            size.width * LiquidHeroMetrics.whaleShareOfWidth, LiquidHeroMetrics.whaleMostWidth,
            size.height * LiquidHeroMetrics.whaleShareOfHeight
        )
        pitch = min(content, LiquidHeroMetrics.ledeMostWidth) * LiquidHeroMetrics.pitchForContent
        seaHeight = whaleWidth * 0.5 + tokens.spacing.xxl
        ledeFont = tokens.uiFont(size: tokens.bodySize * LiquidHeroMetrics.ledeScale)
        ledeWidth = min(content, LiquidHeroMetrics.ledeMostWidth)

        let inkAbove = wordFont.ascender - wordFont.capHeight
        let inkHeight = wordFont.capHeight - wordFont.descender
        let wordToSea = tokens.spacing.xxl
        let seaToLede = tokens.spacing.xxl + tokens.spacing.md
        let linesHeight = LiquidHeroMetrics.liftRoom + pitch * CGFloat(LiquidHeroMetrics.seaLines.count)
        let block = inkHeight + wordToSea + linesHeight + seaToLede + Self.height(of: ledeFont, width: ledeWidth)
        let inkTop = max((size.height - block) * 0.5, tokens.spacing.xl)
        wordBoxTop = inkTop - inkAbove - wordPad
        seaTop = inkTop + inkHeight + wordToSea
        ledeTop = seaTop + linesHeight + seaToLede
        let homeY = wordBoxTop + wordPad + wordFont.lineHeight / 2
        glyphHomes = glyphCentres.map { CGPoint(x: $0, y: homeY) }
    }

    var wordSize: CGFloat { wordFont.pointSize }
    var wordBoxHeight: CGFloat { wordFont.lineHeight + wordPad * 2 }
    var glyphMiddle: CGFloat { wordPad + wordFont.lineHeight / 2 }
    var seaFrame: CGRect { CGRect(x: 0, y: seaTop, width: width, height: seaHeight) }
    var whaleHeight: CGFloat { whaleWidth * LiquidHeroMetrics.whaleHeightForWidth }
    var whaleLeft: CGFloat { margin + content * LiquidHeroMetrics.whaleCross - whaleWidth / 2 }
    var whaleTop: CGFloat { LiquidHeroMetrics.liftRoom + whaleWidth * LiquidHeroMetrics.whaleDive }
    var lineThickness: CGFloat { pitch * LiquidHeroMetrics.thicknessForPitch }
    var glyphSoftness: CGFloat { wordSize * LiquidHeroMetrics.glyphSoftness }
    var glyphReach: CGSize {
        let reach = wordSize * (LiquidHeroMetrics.mostGlyphWater + LiquidHeroMetrics.glyphSoftness)
        return CGSize(width: reach, height: reach)
    }

    /// Where the whale's head is on the water, for the ring that greets it.
    var whaleHead: CGPoint {
        CGPoint(x: whaleLeft + whaleWidth * 0.9, y: whaleTop + whaleHeight * 0.3)
    }

    func lineTop(_ index: Int) -> CGFloat {
        LiquidHeroMetrics.liftRoom + pitch * CGFloat(index) + (pitch - lineThickness) / 2
    }

    /// The name's font, as large as the room allows.
    private static func wordFont(fitting room: CGSize, tokens: Tokens) -> UIFont {
        let probe = tokens.uiFont(size: 100, bold: true)
        let probeWidth = (LiquidHeroMetrics.word as NSString).size(withAttributes: [.font: probe]).width
        let byWidth = room.width * LiquidHeroMetrics.wordShareOfWidth / max(probeWidth, 1) * 100
        let byHeight = room.height * LiquidHeroMetrics.wordShareOfHeight
        return tokens.uiFont(size: floor(min(byWidth, byHeight)), bold: true)
    }

    /// Each letter's centre across `word` set in `font`, kerning included.
    private static func centres(of word: String, font: UIFont, from left: CGFloat) -> [CGFloat] {
        let attributes: [NSAttributedString.Key: Any] = [.font: font]
        let edges = (0...word.count).map { length in
            (String(word.prefix(length)) as NSString).size(withAttributes: attributes).width
        }
        return (0..<word.count).map { left + (edges[$0] + edges[$0 + 1]) / 2 }
    }

    private static func height(of font: UIFont, width: CGFloat) -> CGFloat {
        let text = LiquidHeroMetrics.lede + "\u{2009}|" as NSString
        return ceil(text.boundingRect(
            with: CGSize(width: width, height: .greatestFiniteMagnitude),
            options: [.usesLineFragmentOrigin, .usesFontLeading],
            attributes: [.font: font], context: nil
        ).height)
    }
}
