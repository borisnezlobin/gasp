import SwiftUI

/// The whale along the bottom of the tour, as far across as the tour has
/// come: where it has swum to is the progress.
struct SeaBand: View {
    /// How far along the tour is, from 0 to 1.
    let tide: Double
    let tokens: Tokens
    @Environment(\.colorScheme) private var scheme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// Where the whale's nose starts and stops across the screen, leaving
    /// it room behind it at the start.
    private static let noseRange = (start: 0.2, end: 1.0)
    /// How strongly the whale shows, so it marks progress without pulling
    /// the eye from the step.
    private static let swimmerOpacity = 0.55
    static let height: CGFloat = 56
    private static let swimmerWidth: CGFloat = 84
    private let steps = WelcomeStep.allCases.count

    var body: some View {
        GeometryReader { proxy in
            let margin = WelcomeMetrics.margin
            let span = max(proxy.size.width - margin * 2, 0)
            let along = Self.noseRange.start + (Self.noseRange.end - Self.noseRange.start) * tide
            let nose = margin + span * along
            swimmer
                .opacity(Self.swimmerOpacity)
                .offset(x: nose - Self.swimmerWidth, y: 8)
                .animation(reduceMotion ? nil : .easeInOut(duration: 1.1), value: tide)
        }
        .frame(height: Self.height)
        .clipped()
        .accessibilityElement()
        .accessibilityLabel("Step \(Int((tide * Double(steps - 1)).rounded()) + 1) of \(steps)")
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
