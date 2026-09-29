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
        let base = UIFont(name: family, size: size) ?? .systemFont(ofSize: size)
        var traits = base.fontDescriptor.symbolicTraits
        if bold { traits.insert(.traitBold) }
        if italic { traits.insert(.traitItalic) }
        guard let styled = base.fontDescriptor.withSymbolicTraits(traits) else { return base }
        return UIFont(descriptor: styled, size: size)
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
