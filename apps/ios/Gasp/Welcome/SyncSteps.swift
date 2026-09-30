import SwiftUI

/// How sync works: the iPhone and the Mac keep the vault in a private
/// GitHub repository, drawn as what a repository is, a history of
/// commits. Each note travels from a device to the end of that history
/// and on to the other device.
struct SyncHowStep: View {
    let tokens: Tokens
    let onward: () -> Void
    let notNow: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.md) {
            WelcomeHeading(text: "How sync works", tokens: tokens)
            WelcomeSentence(
                text: "Each device saves its changes to a private GitHub repository and picks up the others'.",
                tokens: tokens
            )
            SyncDiagram(tokens: tokens)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .padding(.horizontal, WelcomeMetrics.margin)
        .safeAreaInset(edge: .bottom) {
            WelcomeFooter(primary: "Continue", tokens: tokens, action: onward) {
                WelcomeQuietButton(title: "Not now", symbol: nil, tokens: tokens, action: notNow)
            }
        }
    }
}

/// What sync needs, and the way into its setup.
struct SyncSetUpStep: View {
    let tokens: Tokens
    let setUp: () -> Void
    let notNow: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.xl) {
            WelcomeHeading(text: "Set up sync", tokens: tokens)
            VStack(alignment: .leading, spacing: tokens.spacing.lg) {
                Need(symbol: "lock", text: "A private repository on GitHub for your notes", tokens: tokens)
                Need(
                    symbol: "key",
                    text: "A fine-grained token with read and write access to that repository's contents",
                    tokens: tokens
                )
                Link(destination: SyncSetupView.newTokenLink) {
                    Label("Make a token on GitHub", systemImage: "arrow.up.right")
                        .labelStyle(TrailingIconLabel())
                        .font(Font(tokens.uiFont(size: tokens.bodySize, bold: true)))
                        .foregroundStyle(tokens.swiftUIColor(\.link))
                        .multilineTextAlignment(.leading)
                        .frame(minHeight: 44, alignment: .leading)
                }
                .padding(.leading, needIndent)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, WelcomeMetrics.margin)
        .safeAreaInset(edge: .bottom) {
            WelcomeFooter(primary: "Set up sync", tokens: tokens, action: setUp) {
                WelcomeQuietButton(title: "Not now", symbol: nil, tokens: tokens, action: notNow)
            }
        }
    }

    private var needIndent: CGFloat { tokens.bodySize * 1.6 + tokens.spacing.md }
}

private struct Need: View {
    let symbol: String
    let text: String
    let tokens: Tokens

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: tokens.spacing.md) {
            Image(systemName: symbol)
                .font(tokens.symbolFont(1.1, weight: .medium))
                .foregroundStyle(tokens.swiftUIColor(\.icon))
                .frame(width: tokens.bodySize * 1.6)
            Text(text)
                .font(Font(tokens.uiFont(size: tokens.bodySize * 1.1)))
                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

private struct TrailingIconLabel: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 4) {
            configuration.title
            configuration.icon.imageScale(.small)
        }
    }
}

/// The iPhone, the repository and the Mac on one line, with a note going
/// round. With motion reduced it holds still mid-trip.
struct SyncDiagram: View {
    let tokens: Tokens
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var started = Date.now

    var body: some View {
        GeometryReader { proxy in
            TimelineView(.animation(paused: reduceMotion)) { context in
                let seconds = reduceMotion ? SyncHistory.tripSeconds * 0.2 : context.date.timeIntervalSince(started)
                SyncDrawing(
                    moment: SyncHistory.moment(at: seconds),
                    geometry: SyncGeometry(width: proxy.size.width, tokens: tokens),
                    tokens: tokens
                )
                .frame(width: proxy.size.width, height: proxy.size.height)
            }
        }
        .accessibilityElement()
        .accessibilityLabel("Notes go from your iPhone to a GitHub repository's history, then on to your Mac, and back.")
    }
}

/// Where each part of the diagram goes on a row `width` wide, centred on
/// the line notes travel along.
struct SyncGeometry {
    let width: CGFloat
    let phoneSize: CGSize
    let macSize: CGSize
    let cardWidth: CGFloat
    let cardHeight: CGFloat
    let linkLength: CGFloat
    let dot: CGFloat
    let line: CGFloat

    init(width: CGFloat, tokens: Tokens) {
        self.width = width
        let unit = min(width / 330, 1.25)
        phoneSize = CGSize(width: 36 * unit, height: 68 * unit)
        macSize = CGSize(width: 86 * unit, height: 56 * unit)
        cardWidth = 136 * unit
        cardHeight = 84 * unit
        dot = 11 * unit
        line = 3 * unit
        linkLength = max((width - phoneSize.width - macSize.width - cardWidth) / 2, 12)
    }

    var phoneRight: CGFloat { phoneSize.width }
    var cardLeft: CGFloat { phoneRight + linkLength }
    var cardRight: CGFloat { cardLeft + cardWidth }
    var macLeft: CGFloat { cardRight + linkLength }
    var commitGap: CGFloat { (cardWidth - dot * 2) / CGFloat(SyncHistory.shownCommits) }

    /// Where a travelling note is, from its line and how far along it is.
    func x(of note: SyncHistory.Travelling) -> CGFloat {
        let start = note.line == .phone ? phoneRight : cardRight
        return start + linkLength * note.along
    }

    func commitX(slot: Int, slide: Double) -> CGFloat {
        cardLeft + dot + commitGap * (CGFloat(slot) + 0.5 + slide)
    }
}

private struct SyncDrawing: View {
    let moment: SyncHistory.Moment
    let geometry: SyncGeometry
    let tokens: Tokens

    var body: some View {
        GeometryReader { proxy in
            let lineY = proxy.size.height / 2
            let labelY = lineY + geometry.cardHeight / 2 + historyDrop + tokens.smallSize * 1.4
            ZStack(alignment: .topLeading) {
                link(from: geometry.phoneRight, at: lineY)
                link(from: geometry.cardRight, at: lineY)
                PhoneDrawing(size: geometry.phoneSize, tokens: tokens)
                    .position(x: geometry.phoneSize.width / 2, y: lineY)
                card.position(x: geometry.cardLeft + geometry.cardWidth / 2, y: lineY - historyDrop)
                MacDrawing(size: geometry.macSize, tokens: tokens)
                    .position(x: geometry.macLeft + geometry.macSize.width / 2, y: lineY)
                label("iPhone").position(x: geometry.phoneSize.width / 2, y: labelY)
                label("GitHub").position(x: geometry.cardLeft + geometry.cardWidth / 2, y: labelY)
                label("Mac").position(x: geometry.macLeft + geometry.macSize.width / 2, y: labelY)
                if let note = moment.travelling {
                    CommitDot(origin: note.origin, size: geometry.dot, tokens: tokens)
                        .position(x: geometry.x(of: note), y: lineY)
                }
            }
        }
    }

    private func link(from start: CGFloat, at lineY: CGFloat) -> some View {
        Rectangle()
            .fill(tokens.swiftUIColor(\.fillStrong))
            .frame(width: geometry.linkLength, height: geometry.line)
            .position(x: start + geometry.linkLength / 2, y: lineY)
    }

    private func label(_ name: String) -> some View {
        Text(name)
            .font(Font(tokens.uiFont(size: tokens.smallSize)))
            .foregroundStyle(tokens.swiftUIColor(\.textDetail))
            .fixedSize()
    }

    /// The repository: its branch mark and name bar, and its history as a
    /// line of commits, the newest at the right.
    private var card: some View {
        let height = geometry.cardHeight
        return ZStack(alignment: .topLeading) {
            PaperSurface(radius: CGFloat(tokens.spacing.radiusMd), tokens: tokens)
            HStack(spacing: 6) {
                Image(systemName: "arrow.triangle.branch")
                    .font(.system(size: geometry.dot * 1.1, weight: .semibold))
                    .foregroundStyle(tokens.swiftUIColor(\.icon))
                Capsule().fill(tokens.swiftUIColor(\.textFaint)).frame(width: geometry.cardWidth * 0.36, height: geometry.line * 1.4)
            }
            .padding(geometry.dot)
            Rectangle()
                .fill(tokens.swiftUIColor(\.fillStrong))
                .frame(width: geometry.cardWidth - geometry.dot * 2, height: geometry.line)
                .offset(x: geometry.dot, y: height / 2 - geometry.line / 2 + historyDrop)
            ForEach(Array(moment.commits.enumerated()), id: \.offset) { slot, origin in
                CommitDot(origin: origin, size: geometry.dot, tokens: tokens)
                    .offset(
                        x: geometry.commitX(slot: slot, slide: moment.slide) - geometry.cardLeft - geometry.dot / 2,
                        y: height / 2 - geometry.dot / 2 + historyDrop
                    )
                    .opacity(slot == moment.commits.count - 1 ? moment.newestShown : 1)
            }
        }
        .frame(width: geometry.cardWidth, height: height)
        .clipShape(RoundedRectangle(cornerRadius: CGFloat(tokens.spacing.radiusMd)).inset(by: -12))
    }

    /// How far below the card's middle its history runs, under its name.
    private var historyDrop: CGFloat { geometry.dot * 0.9 }
}

/// A commit, or a note on its way: ink from the iPhone, a lighter grey
/// from the Mac.
private struct CommitDot: View {
    let origin: SyncHistory.Origin
    let size: CGFloat
    let tokens: Tokens

    var body: some View {
        Circle()
            .fill(tokens.swiftUIColor(origin == .phone ? \.textStrong : \.textFaint))
            .frame(width: size, height: size)
    }
}

/// An iPhone in ink: its body, and a screen holding lines of a note.
private struct PhoneDrawing: View {
    let size: CGSize
    let tokens: Tokens

    var body: some View {
        let inset = size.width * 0.09
        ZStack {
            RoundedRectangle(cornerRadius: size.width * 0.24).fill(tokens.swiftUIColor(\.textStrong))
            RoundedRectangle(cornerRadius: size.width * 0.17)
                .fill(tokens.swiftUIColor(\.background))
                .padding(inset)
            NoteLines(width: size.width - inset * 6, count: 5, tokens: tokens)
        }
        .frame(width: size.width, height: size.height)
    }
}

/// A Mac laptop in ink: the screen with lines of a note, and its base.
private struct MacDrawing: View {
    let size: CGSize
    let tokens: Tokens

    var body: some View {
        let inset = size.height * 0.07
        VStack(spacing: 0) {
            ZStack {
                UnevenRoundedRectangle(topLeadingRadius: inset * 1.6, topTrailingRadius: inset * 1.6)
                    .fill(tokens.swiftUIColor(\.textStrong))
                Rectangle()
                    .fill(tokens.swiftUIColor(\.background))
                    .padding(inset)
                NoteLines(width: size.width * 0.62, count: 4, tokens: tokens)
            }
            .frame(width: size.width * 0.84, height: size.height * 0.86)
            UnevenRoundedRectangle(bottomLeadingRadius: inset * 1.4, bottomTrailingRadius: inset * 1.4)
                .fill(tokens.swiftUIColor(\.textStrong))
                .frame(width: size.width, height: size.height * 0.1)
        }
        .frame(width: size.width, height: size.height)
    }
}

private struct NoteLines: View {
    let width: CGFloat
    let count: Int
    let tokens: Tokens

    private static let shares: [CGFloat] = [0.6, 1, 0.86, 1, 0.5]

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            ForEach(0..<count, id: \.self) { index in
                Capsule()
                    .fill(tokens.swiftUIColor(index == 0 ? \.textFaint : \.fillStrong))
                    .frame(width: width * Self.shares[index % Self.shares.count], height: 2.5)
            }
        }
        .frame(width: width, alignment: .leading)
    }
}
