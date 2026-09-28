import UIKit

/// How a run of text looks: the planner's semantic styles turned into a
/// font, colours and decorations, following the desktop app's rules.
struct RunLook {
    enum Typeface {
        case text
        case code
    }

    var typeface: Typeface = .text
    var headingLevel: UInt8?
    var sizeFactor: CGFloat = 1
    var bold = false
    var italic = false
    var ink: KeyPath<Palette, ThemeColor> = \.text
    var explicitInk: UIColor?
    var background: UIColor?
    var underlined = false
    /// The underline's colour when it isn't the text's.
    var underlineColor: UIColor?
    var strikethrough = false
    var baselineRise: CGFloat = 0
    /// The callout the run's line is in, whose colour its title takes.
    var calloutKind: String?
    var usesCalloutInk = false

    /// Sub- and superscript text, as a share of the text around it; the
    /// desktop app uses the same values.
    static let scriptScale: CGFloat = 0.7
    static let superscriptRise: CGFloat = 0.4
    static let subscriptDrop: CGFloat = -0.2

    init(styles: [InlineStyle] = [], base: RunLook? = nil, tokens: Tokens) {
        self = base ?? RunLook()
        for style in styles {
            applyShape(style)
            applyDecoration(style, tokens: tokens)
        }
        applyInk(styles, tokens: tokens)
    }

    private init() {}

    func size(_ tokens: Tokens) -> CGFloat {
        let base = headingLevel.map(tokens.headingSize) ?? tokens.bodySize
        let code = typeface == .code ? CGFloat(tokens.typography.codeScale) : 1
        return base * code * sizeFactor
    }

    func font(_ tokens: Tokens) -> UIFont {
        switch typeface {
        case .text: tokens.textFont(size: size(tokens), bold: bold, italic: italic)
        case .code: tokens.codeFont(size: size(tokens), bold: bold, italic: italic)
        }
    }

    func attributes(_ tokens: Tokens) -> [NSAttributedString.Key: Any] {
        let font = font(tokens)
        let color = explicitInk ?? calloutInk(tokens) ?? tokens.color(ink)
        var attributes: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: color]
        attributes[.backgroundColor] = background
        if underlined {
            attributes[.underlineStyle] = NSUnderlineStyle.single.rawValue
            attributes[.underlineColor] = underlineColor ?? color
        }
        if strikethrough {
            attributes[.strikethroughStyle] = NSUnderlineStyle.single.rawValue
            attributes[.strikethroughColor] = color
        }
        if baselineRise != 0 {
            attributes[.baselineOffset] = baselineRise * font.pointSize
        }
        return attributes
    }

    private func calloutInk(_ tokens: Tokens) -> UIColor? {
        guard usesCalloutInk, let calloutKind else { return nil }
        return tokens.calloutColor(calloutKind)
    }

    private mutating func applyShape(_ style: InlineStyle) {
        switch style {
        case .strong: bold = true
        case .emphasis: italic = true
        case .heading(let level):
            headingLevel = level
            bold = true
        case .code, .codeBlock, .mathSource, .mathBracket, .kbd: typeface = .code
        case .superscript, .footnoteRef: raise(by: Self.superscriptRise)
        case .subscript: raise(by: Self.subscriptDrop)
        case .fontScale(let percent): sizeFactor *= CGFloat(percent) / 100
        case .calloutTitle: bold = true
        default: break
        }
    }

    private mutating func raise(by share: CGFloat) {
        sizeFactor *= Self.scriptScale
        baselineRise = share
    }

    private mutating func applyDecoration(_ style: InlineStyle, tokens: Tokens) {
        switch style {
        case .strikethrough, .taskDone: strikethrough = true
        case .underline: underlined = true
        case .link:
            underlined = true
            underlineColor = tokens.color(\.linkUnderline)
        case .highlight: background = tokens.color(\.highlight)
        case .code: background = tokens.color(\.codeBackground)
        case .tag, .propertyChip: background = tokens.color(\.fill)
        case .kbd: background = tokens.color(\.fillStrong)
        case .textBackground(let rgba): background = UIColor(rgba: rgba)
        default: break
        }
    }

    /// The first style that sets a colour wins: shown symbols, then a colour
    /// the note's HTML asks for, then the rest.
    private mutating func applyInk(_ styles: [InlineStyle], tokens: Tokens) {
        let ranked = styles.compactMap { style in Self.ink(style).map { (style, $0) } }
        guard let (style, choice) = ranked.min(by: { $0.1.rank < $1.1.rank }) else { return }
        ink = choice.token
        if case .textColor(let rgba) = style {
            explicitInk = UIColor(rgba: rgba)
        }
        if case .calloutTitle = style {
            usesCalloutInk = true
        }
    }

    private static func ink(_ style: InlineStyle) -> (rank: Int, token: KeyPath<Palette, ThemeColor>)? {
        switch style {
        case .markupDimmed, .html, .comment: (0, \.textFaint)
        case .textColor: (1, \.text)
        case .taskDone: (2, \.textFaint)
        case .link, .footnoteRef, .tag: (3, \.link)
        case .frontmatterKey, .kbd, .frontmatter: (4, \.textMuted)
        case .mathSource, .mathBracket: (5, \.math)
        case .calloutTitle: (6, \.text)
        default: nil
        }
    }
}

extension UIColor {
    /// A colour packed as `0xRRGGBBAA`.
    convenience init(rgba: UInt32) {
        self.init(
            red: CGFloat((rgba >> 24) & 0xFF) / 255,
            green: CGFloat((rgba >> 16) & 0xFF) / 255,
            blue: CGFloat((rgba >> 8) & 0xFF) / 255,
            alpha: CGFloat(rgba & 0xFF) / 255
        )
    }
}
