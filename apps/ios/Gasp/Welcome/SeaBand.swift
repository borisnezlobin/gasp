import SwiftUI

/// The sea along the bottom of the tour: lines like the lines of text in
/// the app icon. The top line is the tour's progress, darker as far as the
/// red caret, and the whale swims just under it with its nose at the
/// caret, washed by the screen's colour because it's under the surface.
struct SeaBand: View {
    /// How far along the tour is, from 0 to 1.
    let tide: Double
    let tokens: Tokens
    @Environment(\.colorScheme) private var scheme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// Where the caret starts and stops along the top line, leaving the
    /// whale room behind it at the start.
    private static let caretRange = (start: 0.2, end: 1.0)
    /// How much of the line's width each lower line takes.
    private static let lowerLines: [CGFloat] = [0.93, 0.68]
    /// How much of the screen's colour lies over what's under the surface.
    private static let underwater = 0.62
    static let height: CGFloat = 56
    private static let surface: CGFloat = 8
    private static let pitch: CGFloat = 16
    private static let swimmerWidth: CGFloat = 84

    var body: some View {
        GeometryReader { proxy in
            let margin = WelcomeMetrics.margin
            let span = max(proxy.size.width - margin * 2, 0)
            let along = Self.caretRange.start + (Self.caretRange.end - Self.caretRange.start) * tide
            let caret = margin + span * along
            ZStack(alignment: .topLeading) {
                swimmer.offset(x: caret - Self.swimmerWidth, y: Self.surface + line * 1.5)
                tokens.swiftUIColor(\.background)
                    .opacity(Self.underwater)
                    .padding(.top, Self.surface + line)
                bar(from: margin, to: caret - gap, color: \.textFaint)
                bar(from: caret + gap, to: margin + span, color: \.fillStrong)
                ForEach(Array(Self.lowerLines.enumerated()), id: \.offset) { index, share in
                    bar(from: margin, to: margin + span * share, color: \.fillStrong)
                        .offset(y: Self.pitch * CGFloat(index + 1))
                }
                CaretMark(width: 2.5, height: line * 3.2, tokens: tokens)
                    .offset(x: caret - 1.25, y: Self.surface + line / 2 - line * 1.6)
            }
            .animation(reduceMotion ? nil : .easeInOut(duration: 1.1), value: tide)
        }
        .frame(height: Self.height)
        .clipped()
        .accessibilityElement()
        .accessibilityLabel("Step \(Int((tide * 4).rounded()) + 1) of 5")
    }

    private var line: CGFloat { WelcomeMetrics.lineThickness(tokens) * 0.8 }
    private var gap: CGFloat { 8 }

    private func bar(from start: CGFloat, to end: CGFloat, color: KeyPath<Palette, ThemeColor>) -> some View {
        Capsule()
            .fill(tokens.swiftUIColor(color))
            .frame(width: max(end - start, 0), height: line)
            .offset(x: start, y: Self.surface)
    }

    private var swimmer: some View {
        let frames = WhaleArt.swimFrames(for: scheme)
        return TimelineView(.periodic(from: .now, by: WhaleArt.swimFrameTime)) { context in
            if let frame = frame(of: frames, at: context.date) {
                Image(uiImage: frame)
                    .resizable()
                    .frame(width: Self.swimmerWidth, height: Self.swimmerWidth / WhaleArt.swimAspect)
            }
        }
        .accessibilityHidden(true)
    }

    private func frame(of frames: [UIImage], at date: Date) -> UIImage? {
        guard !frames.isEmpty else { return nil }
        if reduceMotion { return frames[0] }
        let index = Int(date.timeIntervalSinceReferenceDate / WhaleArt.swimFrameTime) % frames.count
        return frames[index]
    }
}
