import SwiftUI
import UIKit

/// The pieces every step of the welcome tour is built from: its heading
/// and sentence, the buttons along the bottom, and the icon's red caret.
enum WelcomeMetrics {
    /// The space kept from the screen's sides.
    static let margin: CGFloat = 24
    /// The thickness of a drawn line of text.
    static func lineThickness(_ tokens: Tokens) -> CGFloat {
        max(4, tokens.bodySize * 0.3)
    }
}

/// A step's heading.
struct WelcomeHeading: View {
    let text: String
    let tokens: Tokens

    var body: some View {
        Text(text)
            .font(Font(tokens.uiFont(size: tokens.bodySize * 1.65, bold: true)))
            .foregroundStyle(tokens.swiftUIColor(\.textStrong))
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityAddTraits(.isHeader)
    }
}

/// A step's one sentence under its heading.
struct WelcomeSentence: View {
    let text: String
    let tokens: Tokens

    var body: some View {
        Text(text)
            .font(Font(tokens.uiFont(size: tokens.bodySize)))
            .foregroundStyle(tokens.swiftUIColor(\.textDetail))
            .lineSpacing(tokens.bodySize * 0.2)
            .fixedSize(horizontal: false, vertical: true)
    }
}

/// The step's main button, filled, full width and within thumb reach.
struct WelcomePrimaryButton: View {
    let title: String
    let tokens: Tokens
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(Font(tokens.uiFont(size: tokens.bodySize, bold: true)))
                .foregroundStyle(tokens.swiftUIColor(\.onAccent))
                .frame(maxWidth: .infinity, minHeight: 52)
                .background(Capsule().fill(tokens.swiftUIColor(\.accent)))
                .contentShape(Capsule())
        }
        .buttonStyle(PressedStyle())
    }
}

/// A quieter choice under the main button: its icon and words, unfilled.
struct WelcomeQuietButton: View {
    let title: String
    let symbol: String?
    let tokens: Tokens
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: tokens.spacing.sm) {
                if let symbol {
                    Image(systemName: symbol)
                        .font(tokens.symbolFont(0.875))
                        .foregroundStyle(tokens.swiftUIColor(\.icon))
                }
                Text(title)
                    .font(Font(tokens.uiFont(size: tokens.bodySize)))
                    .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                    .lineLimit(2)
                    .multilineTextAlignment(.center)
            }
            .padding(.horizontal, tokens.spacing.sm)
            .frame(minHeight: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(PressedStyle())
    }
}

/// Sinks a little and dims while pressed.
private struct PressedStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .opacity(configuration.isPressed ? 0.75 : 1)
            .scaleEffect(configuration.isPressed ? 0.98 : 1)
            .animation(.easeOut(duration: 0.12), value: configuration.isPressed)
    }
}

/// The bottom of a step: its main button, and under it a row kept for its
/// quieter choices, so the main button sits in the same place on every
/// step. The row folds away while the keyboard is up.
struct WelcomeFooter<Quiet: View>: View {
    let primary: String
    let tokens: Tokens
    let keyboardShown: Bool
    let action: () -> Void
    let quiet: () -> Quiet

    init(
        primary: String, tokens: Tokens, keyboardShown: Bool = false,
        action: @escaping () -> Void, @ViewBuilder quiet: @escaping () -> Quiet
    ) {
        self.primary = primary
        self.tokens = tokens
        self.keyboardShown = keyboardShown
        self.action = action
        self.quiet = quiet
    }

    var body: some View {
        VStack(spacing: tokens.spacing.sm) {
            WelcomePrimaryButton(title: primary, tokens: tokens, action: action)
            if !keyboardShown {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: tokens.spacing.lg) { quiet() }
                    VStack(spacing: 0) { quiet() }
                }
                .frame(minHeight: 44)
            }
        }
        .padding(.horizontal, WelcomeMetrics.margin)
    }
}

extension WelcomeFooter where Quiet == EmptyView {
    init(primary: String, tokens: Tokens, keyboardShown: Bool = false, action: @escaping () -> Void) {
        self.init(primary: primary, tokens: tokens, keyboardShown: keyboardShown, action: action) { EmptyView() }
    }
}

/// The app icon's red caret, blinking as a text caret does, or steady
/// when motion is reduced.
struct CaretMark: View {
    let width: CGFloat
    let height: CGFloat
    let tokens: Tokens
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private static let blink = 0.53

    var body: some View {
        TimelineView(.periodic(from: .now, by: Self.blink)) { context in
            Capsule()
                .fill(tokens.swiftUIColor(\.caretMark))
                .frame(width: width, height: height)
                .opacity(isOn(at: context.date) ? 1 : 0)
        }
        .accessibilityHidden(true)
    }

    private func isOn(at date: Date) -> Bool {
        reduceMotion || Int(date.timeIntervalSinceReferenceDate / Self.blink).isMultiple(of: 2)
    }
}

/// The tour's whales, in ink for light mode and chalk for dark, from the
/// same renders as the Mac's tour.
enum WhaleArt {
    /// Frames in the swimming strip, laid side by side.
    static let swimFrameCount = 30
    /// How long each swimming frame shows.
    static let swimFrameTime = 0.066
    /// How wide the breaching whale is for its height.
    static let breachAspect: CGFloat = 900.0 / 873.0
    /// How wide a swimming frame is for its height.
    static let swimAspect: CGFloat = 440.0 / 170.0

    private static var swimCache: [UIUserInterfaceStyle: [UIImage]] = [:]

    /// The swimming strip cut into its frames, for `scheme`.
    static func swimFrames(for scheme: ColorScheme) -> [UIImage] {
        let style: UIUserInterfaceStyle = scheme == .dark ? .dark : .light
        if let frames = swimCache[style] { return frames }
        let traits = UITraitCollection(userInterfaceStyle: style)
        guard let strip = UIImage(named: "WhaleSwim", in: .main, compatibleWith: traits)?.cgImage else { return [] }
        let width = strip.width / swimFrameCount
        let frames = (0..<swimFrameCount).compactMap { index in
            strip.cropping(to: CGRect(x: index * width, y: 0, width: width, height: strip.height))
                .map { UIImage(cgImage: $0) }
        }
        swimCache[style] = frames
        return frames
    }
}

/// A drawn sheet of paper: raised a shade off the page, as popovers are,
/// with a hairline ring and a soft shadow under it.
struct PaperSurface: View {
    let radius: CGFloat
    let tokens: Tokens

    var body: some View {
        RoundedRectangle(cornerRadius: radius)
            .fill(tokens.swiftUIColor(\.popover))
            .shadow(color: tokens.swiftUIColor(\.ring), radius: 0.6)
            .shadow(color: tokens.swiftUIColor(\.shadow).opacity(0.45), radius: 10, y: 3)
    }
}
