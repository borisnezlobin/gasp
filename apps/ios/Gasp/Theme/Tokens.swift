import SwiftUI
import UIKit

/// Every font, colour and spacing value the app draws with. They come from
/// the core's theme tokens, so a vault's `.gasp/theme.toml` restyles the
/// phone as it does the desktop.
struct Tokens {
    private(set) var typography: Typography
    let spacing: Spacing
    private let light: Palette
    private let dark: Palette

    init(theme: ThemeTokens) {
        typography = theme.typography
        spacing = theme.spacing
        light = theme.light
        dark = theme.dark
    }

    /// The same tokens with every text size multiplied by `factor`, for
    /// this device's zoom.
    func scaled(by factor: Double) -> Tokens {
        var copy = self
        copy.typography.bodySize = typography.bodySize * factor
        return copy
    }

    /// How much larger than the default the reader's text size setting
    /// (Dynamic Type) makes body text.
    static var textSizeFactor: Double {
        Double(UIFontMetrics(forTextStyle: .body).scaledValue(for: 100) / 100)
    }

    /// The tokens with text sized by the reader's text size setting.
    static func forReader(theme: ThemeTokens) -> Tokens {
        Tokens(theme: theme).scaled(by: textSizeFactor)
    }

    /// An SF Symbol's font, `scale` times the body text's size, so icons
    /// grow with the text.
    func symbolFont(_ scale: Double = 1, weight: Font.Weight = .regular) -> Font {
        .system(size: bodySize * CGFloat(scale), weight: weight)
    }

    /// A colour that follows light and dark mode.
    func color(_ token: KeyPath<Palette, ThemeColor>) -> UIColor {
        let light = UIColor(light[keyPath: token])
        let dark = UIColor(dark[keyPath: token])
        return UIColor { traits in traits.userInterfaceStyle == .dark ? dark : light }
    }

    func swiftUIColor(_ token: KeyPath<Palette, ThemeColor>) -> Color {
        Color(uiColor: color(token))
    }

    func calloutColor(_ kind: String) -> UIColor {
        let light = light.callouts[kind].map(UIColor.init) ?? color(\.textMuted)
        let dark = dark.callouts[kind].map(UIColor.init) ?? color(\.textMuted)
        return UIColor { traits in traits.userInterfaceStyle == .dark ? dark : light }
    }

    /// A callout's surface: its colour at the callout opacity.
    func calloutFill(_ kind: String) -> UIColor {
        let colors = [light, dark].map { palette in
            let color = palette.callouts[kind].map(UIColor.init) ?? UIColor(palette.textMuted)
            return color.withAlphaComponent(CGFloat(palette.calloutOpacity))
        }
        return UIColor { traits in traits.userInterfaceStyle == .dark ? colors[1] : colors[0] }
    }

    var bodySize: CGFloat {
        CGFloat(typography.bodySize)
    }

    func headingSize(_ level: UInt8) -> CGFloat {
        let scales = typography.headingScales
        let index = min(max(Int(level), 1), scales.count) - 1
        return bodySize * CGFloat(scales[index])
    }

    /// A note's title above its text.
    var titleSize: CGFloat {
        bodySize * CGFloat(typography.titleScale)
    }

    var codeSize: CGFloat {
        bodySize * CGFloat(typography.codeScale)
    }

    var smallSize: CGFloat {
        bodySize * CGFloat(typography.smallScale)
    }

    func textFont(size: CGFloat, bold: Bool = false, italic: Bool = false) -> UIFont {
        Self.font(family: typography.textFont, size: size, bold: bold, italic: italic)
    }

    func uiFont(size: CGFloat, bold: Bool = false) -> UIFont {
        Self.font(family: typography.uiFont, size: size, bold: bold, italic: false)
    }

    func codeFont(size: CGFloat, bold: Bool = false, italic: Bool = false) -> UIFont {
        Self.font(family: typography.codeFont, size: size, bold: bold, italic: italic)
    }

    private static func font(family: String, size: CGFloat, bold: Bool, italic: Bool) -> UIFont {
        FontCache.shared.font(FontCache.Key(family: family, size: size, bold: bold, italic: italic)) {
            makeFont(family: family, size: size, bold: bold, italic: italic)
        }
    }

    private static func makeFont(family: String, size: CGFloat, bold: Bool, italic: Bool) -> UIFont {
        let base = UIFont(name: family, size: size) ?? .systemFont(ofSize: size)
        var traits = base.fontDescriptor.symbolicTraits
        if bold { traits.insert(.traitBold) }
        if italic { traits.insert(.traitItalic) }
        guard let styled = base.fontDescriptor.withSymbolicTraits(traits) else { return base }
        return UIFont(descriptor: styled, size: size)
    }
}

/// Fonts made once for each family, size and style, since styling a note
/// asks for the same few thousands of times.
private final class FontCache {
    static let shared = FontCache()

    struct Key: Hashable {
        let family: String
        let size: CGFloat
        let bold: Bool
        let italic: Bool
    }

    private var fonts: [Key: UIFont] = [:]
    private let lock = NSLock()

    func font(_ key: Key, make: () -> UIFont) -> UIFont {
        lock.lock()
        defer { lock.unlock() }
        if let font = fonts[key] { return font }
        let font = make()
        fonts[key] = font
        return font
    }
}

extension UIColor {
    convenience init(_ color: ThemeColor) {
        self.init(
            red: CGFloat(color.red),
            green: CGFloat(color.green),
            blue: CGFloat(color.blue),
            alpha: CGFloat(color.alpha)
        )
    }
}

extension Font {
    init(_ font: UIFont) {
        self.init(font as CTFont)
    }
}

/// Light or dark as `appearance.theme` asks. Every colour above is dynamic,
/// so overriding the windows' interface style restyles the editor, the
/// sheets, menus and the keyboard at once; `system` hands it back to the
/// phone's own setting.
extension Appearance {
    var interfaceStyle: UIUserInterfaceStyle {
        switch self {
        case .light: .light
        case .dark: .dark
        case .system: .unspecified
        }
    }

    /// Applies this appearance to every window the app has open.
    @MainActor
    func apply() {
        for case let scene as UIWindowScene in UIApplication.shared.connectedScenes {
            for window in scene.windows {
                window.overrideUserInterfaceStyle = interfaceStyle
            }
        }
    }
}
