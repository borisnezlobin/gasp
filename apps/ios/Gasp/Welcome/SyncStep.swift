import SwiftUI

/// Syncing with the person's other devices: iCloud keeps the vault the
/// same on the iPhone and the Mac, drawn as a note travelling through a
/// cloud between them. iCloud is the main way in; GitHub sits under it.
/// A vault that syncs already only has Done.
struct SyncStep: View {
    let tokens: Tokens
    let alreadySyncs: Bool
    let icloud: () -> Void
    let github: () -> Void
    let notNow: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.md) {
            WelcomeHeading(text: "Sync with your other devices", tokens: tokens)
            WelcomeSentence(
                text: "iCloud keeps your notes the same on your iPhone and Mac, with nothing to sign up for.",
                tokens: tokens
            )
            SyncDiagram(tokens: tokens)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .padding(.horizontal, WelcomeMetrics.margin)
        .safeAreaInset(edge: .bottom) { footer }
    }

    @ViewBuilder private var footer: some View {
        if alreadySyncs {
            WelcomeFooter(primary: "Done", tokens: tokens, action: notNow)
        } else {
            WelcomeFooter(primary: "Sync with iCloud", tokens: tokens, action: icloud) {
                WelcomeQuietButton(title: "Use GitHub instead", symbol: nil, tokens: tokens, action: github)
                WelcomeQuietButton(title: "Not now", symbol: nil, tokens: tokens, action: notNow)
            }
        }
    }
}

/// The iPhone, iCloud and the Mac on one line, with a note going round.
/// With motion reduced it holds still mid-trip.
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
                    geometry: SyncGeometry(width: proxy.size.width),
                    tokens: tokens
                )
                .frame(width: proxy.size.width, height: proxy.size.height)
            }
        }
        .accessibilityElement()
        .accessibilityLabel("Notes go from your iPhone to iCloud, then on to your Mac, and back.")
    }
}

/// Where each part of the diagram goes on a row `width` wide, centred on
/// the line notes travel along.
struct SyncGeometry {
    let width: CGFloat
    let phoneSize: CGSize
    let macSize: CGSize
    let cloudSize: CGSize
    let linkLength: CGFloat
    let dot: CGFloat
    let line: CGFloat

    init(width: CGFloat) {
        self.width = width
        let unit = min(width / 330, 1.25)
        phoneSize = CGSize(width: 36 * unit, height: 68 * unit)
        macSize = CGSize(width: 86 * unit, height: 56 * unit)
        cloudSize = CGSize(width: 120 * unit, height: 120 * unit / CloudShape.aspect)
        dot = 11 * unit
        line = 3 * unit
        linkLength = max((width - phoneSize.width - macSize.width - cloudSize.width) / 2, 12)
    }

    var phoneRight: CGFloat { phoneSize.width }
    var cloudLeft: CGFloat { phoneRight + linkLength }
    var cloudRight: CGFloat { cloudLeft + cloudSize.width }
    var cloudMiddle: CGFloat { cloudLeft + cloudSize.width / 2 }
    var macLeft: CGFloat { cloudRight + linkLength }

    /// Where a travelling note is, from its line and how far along it is.
    func x(of note: SyncHistory.Travelling) -> CGFloat {
        let start = note.line == .phone ? phoneRight : cloudRight
        return start + linkLength * note.along
    }
}

private struct SyncDrawing: View {
    let moment: SyncHistory.Moment
    let geometry: SyncGeometry
    let tokens: Tokens

    var body: some View {
        GeometryReader { proxy in
            let lineY = proxy.size.height / 2
            let labelY = lineY + max(geometry.phoneSize.height, geometry.cloudSize.height) / 2 + tokens.smallSize * 1.4
            ZStack(alignment: .topLeading) {
                link(from: geometry.phoneRight, at: lineY)
                link(from: geometry.cloudRight, at: lineY)
                PhoneDrawing(size: geometry.phoneSize, tokens: tokens)
                    .position(x: geometry.phoneSize.width / 2, y: lineY)
                cloud.position(x: geometry.cloudMiddle, y: lineY)
                MacDrawing(size: geometry.macSize, tokens: tokens)
                    .position(x: geometry.macLeft + geometry.macSize.width / 2, y: lineY)
                label("iPhone").position(x: geometry.phoneSize.width / 2, y: labelY)
                label("iCloud").position(x: geometry.cloudMiddle, y: labelY)
                label("Mac").position(x: geometry.macLeft + geometry.macSize.width / 2, y: labelY)
                if let note = moment.travelling {
                    NoteDot(origin: note.origin, size: geometry.dot, tokens: tokens)
                        .position(x: geometry.x(of: note), y: lineY)
                }
            }
        }
    }

    /// The cloud, with the note resting in it for a moment on its way.
    private var cloud: some View {
        CloudDrawing(tokens: tokens) {
            ZStack {
                NoteLines(width: geometry.cloudSize.width * 0.42, count: 3, tokens: tokens)
                if let resting = moment.resting {
                    NoteDot(origin: resting, size: geometry.dot, tokens: tokens)
                        .scaleEffect(1 + 0.35 * sin(moment.settled * .pi))
                }
            }
        }
        .frame(width: geometry.cloudSize.width, height: geometry.cloudSize.height)
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
}

/// A note on its way: ink from the iPhone, a lighter grey from the Mac.
private struct NoteDot: View {
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
