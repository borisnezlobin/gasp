import SwiftUI

/// Choosing where notes live. Making a new vault is the one big choice;
/// opening a folder that's already there and trying the sample vault sit
/// under it.
struct VaultStep: View {
    let tokens: Tokens
    let makeNew: () -> Void
    let openFolder: () -> Void
    let trySample: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.md) {
            WelcomeHeading(text: "Where your notes live", tokens: tokens)
            WelcomeSentence(
                text: "A vault is a folder of Markdown files, so an Obsidian vault opens as it is.",
                tokens: tokens
            )
            NoteStack(tokens: tokens)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .padding(.horizontal, WelcomeMetrics.margin)
        .safeAreaInset(edge: .bottom) {
            WelcomeFooter(primary: "Make a new vault", tokens: tokens, action: makeNew) {
                WelcomeQuietButton(title: "Open a folder", symbol: "folder", tokens: tokens, action: openFolder)
                WelcomeQuietButton(title: "Try the sample vault", symbol: "book", tokens: tokens, action: trySample)
            }
        }
    }
}

/// A vault drawn as three notes fanned out, each a title and lines of
/// text, the top one with the icon's red caret at the end of its first
/// line.
private struct NoteStack: View {
    let tokens: Tokens

    private static let turns: [Double] = [-9, 5, -1.5]
    private static let shifts: [CGFloat] = [-0.16, 0.14, 0]

    var body: some View {
        GeometryReader { proxy in
            let width = min(proxy.size.width * 0.62, proxy.size.height * 0.7, 240)
            ZStack {
                ForEach(0..<3, id: \.self) { index in
                    NoteCard(width: width, isTop: index == 2, tokens: tokens)
                        .rotationEffect(.degrees(Self.turns[index]))
                        .offset(x: width * Self.shifts[index])
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .accessibilityHidden(true)
    }
}

private struct NoteCard: View {
    let width: CGFloat
    let isTop: Bool
    let tokens: Tokens

    private static let lines: [CGFloat] = [0.9, 0.78, 0.94, 0.52]

    var body: some View {
        let line = WelcomeMetrics.lineThickness(tokens) * 0.9
        VStack(alignment: .leading, spacing: line * 1.9) {
            HStack(spacing: line) {
                Capsule().fill(tokens.swiftUIColor(\.textMuted)).frame(width: width * 0.46, height: line * 1.6)
                if isTop { CaretMark(width: 2.5, height: line * 3.4, tokens: tokens) }
            }
            .frame(height: line * 3.4, alignment: .leading)
            ForEach(Array(Self.lines.enumerated()), id: \.offset) { _, share in
                Capsule().fill(tokens.swiftUIColor(\.fillStrong)).frame(width: (width - line * 8) * share, height: line)
            }
        }
        .padding(line * 4)
        .frame(width: width, height: width * 1.25, alignment: .topLeading)
        .background(PaperSurface(radius: CGFloat(tokens.spacing.radiusLg), tokens: tokens))
    }
}
