import SwiftUI

/// Every open tab as a card, like Safari's tab view: tap one to show it,
/// close one with its button or by swiping it sideways, or start a new one.
struct TabOverview: View {
    @Environment(AppModel.self) private var model

    private var tokens: Tokens { model.library.tokens }
    private let columns = [GridItem(.flexible()), GridItem(.flexible())]

    var body: some View {
        VStack(spacing: 0) {
            ScrollView {
                LazyVGrid(columns: columns, spacing: tokens.spacing.xl) {
                    ForEach(Array(model.tabs.tabs.enumerated()), id: \.element.id) { index, tab in
                        TabCard(
                            tab: tab,
                            title: model.tabs.title(of: tab),
                            isActive: index == model.tabs.activeIndex,
                            tokens: tokens,
                            open: { show(index) },
                            close: { withAnimation(.snappy) { model.tabs.close(tab.id) } }
                        )
                    }
                }
                .padding(tokens.spacing.xl)
            }
            bottomBar
        }
        .background(tokens.swiftUIColor(\.surface).ignoresSafeArea())
    }

    private var bottomBar: some View {
        HStack {
            BarButton(symbol: "plus", label: "New tab", tokens: tokens) {
                model.tabs.newTab()
                model.workspace.overviewOpen = false
            }
            Spacer()
            Text(model.tabs.tabs.count == 1 ? "1 tab" : "\(model.tabs.tabs.count) tabs")
                .font(Font(tokens.uiFont(size: tokens.smallSize)))
                .foregroundStyle(tokens.swiftUIColor(\.textDetail))
            Spacer()
            Button("Done") { model.workspace.overviewOpen = false }
                .font(Font(tokens.uiFont(size: tokens.bodySize, bold: true)))
                .foregroundStyle(tokens.swiftUIColor(\.accent))
                .frame(minWidth: 44, minHeight: 44)
        }
        .padding(.horizontal, tokens.spacing.xl)
        .padding(.vertical, tokens.spacing.sm)
    }

    private func show(_ index: Int) {
        model.tabs.select(index)
        model.workspace.overviewOpen = false
    }
}

/// A tab's card: its title over the start of its note.
private struct TabCard: View {
    @Environment(AppModel.self) private var model
    let tab: BrowserTab
    let title: String
    let isActive: Bool
    let tokens: Tokens
    let open: () -> Void
    let close: () -> Void
    @State private var preview = ""
    @State private var swipe: CGFloat = 0
    @State private var swipeAxis: Axis?
    @State private var cardWidth: CGFloat = 0
    @State private var closing = false
    @GestureState private var touching = false

    private var radius: CGFloat { CGFloat(tokens.spacing.radiusLg) }
    /// The room between the card and the ring round the tab showing.
    private var ringGap: CGFloat { CGFloat(tokens.spacing.xs) }

    var body: some View {
        face
            .contentShape(RoundedRectangle(cornerRadius: radius))
            .onTapGesture(perform: open)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(title)
            .accessibilityAddTraits(isActive ? [.isButton, .isSelected] : .isButton)
            .accessibilityAction { open() }
            .accessibilityAction(named: "Close") { close() }
            .overlay(alignment: .topTrailing) { closeButton }
            .offset(x: swipe)
            .opacity(1 - swipeProgress)
            .zIndex(swipe == 0 ? 0 : 1)
            .onGeometryChange(for: CGFloat.self, of: \.size.width) { cardWidth = $0 }
            .simultaneousGesture(swipeAway)
            .onChange(of: touching) { _, stillTouching in
                if !stillTouching && !closing { springBack() }
            }
            .task(id: tab.path) { preview = readPreview() }
    }

    private var face: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.sm) {
            Text(title)
                .font(Font(tokens.textFont(size: tokens.bodySize, bold: true)))
                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                .lineLimit(2)
                .padding(.trailing, 28)
            Text(preview)
                .font(Font(tokens.textFont(size: tokens.smallSize * 0.85)))
                .foregroundStyle(tokens.swiftUIColor(\.textMuted))
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
        .padding(tokens.spacing.lg)
        .frame(height: 220)
        .background(
            RoundedRectangle(cornerRadius: radius)
                .fill(tokens.swiftUIColor(\.background))
                .shadow(color: tokens.swiftUIColor(\.shadow), radius: 6, y: 2)
        )
        .overlay {
            RoundedRectangle(cornerRadius: radius + ringGap)
                .stroke(isActive ? tokens.swiftUIColor(\.accent) : .clear, lineWidth: 2)
                .padding(-ringGap)
        }
    }

    /// How far across its own width the card has gone, which is how much
    /// it has faded.
    private var swipeProgress: CGFloat {
        guard cardWidth > 0 else { return 0 }
        return min(abs(swipe) / cardWidth, 1)
    }

    /// Past half the card's width, letting go closes the tab.
    private var passedClosePoint: Bool { swipeProgress > 0.5 }

    /// A sideways drag carries the card with the finger. Letting go past
    /// half its width, or flicking it, sends it off and closes the tab;
    /// anything less springs it back. A drag that starts mostly up or down
    /// is left to the scroll view.
    private var swipeAway: some Gesture {
        DragGesture(minimumDistance: CGFloat(tokens.spacing.lg))
            .updating($touching) { _, touching, _ in touching = true }
            .onChanged(follow)
            .onEnded(release)
    }

    private func follow(_ value: DragGesture.Value) {
        guard !closing else { return }
        if swipeAxis == nil {
            let translation = value.translation
            swipeAxis = abs(translation.width) > abs(translation.height) ? .horizontal : .vertical
        }
        guard swipeAxis == .horizontal else { return }
        let wasPastClosePoint = passedClosePoint
        swipe = value.translation.width
        if passedClosePoint != wasPastClosePoint { Haptics.tick() }
    }

    private func release(_ value: DragGesture.Value) {
        defer { swipeAxis = nil }
        guard swipeAxis == .horizontal, !closing else { return }
        let flung = abs(value.predictedEndTranslation.width) > cardWidth
        guard passedClosePoint || flung else { return springBack() }
        sendOff(towards: flung ? value.predictedEndTranslation.width : swipe)
    }

    private func sendOff(towards direction: CGFloat) {
        closing = true
        withAnimation(.tabSettling) {
            swipe = direction < 0 ? -cardWidth : cardWidth
        } completion: {
            close()
        }
    }

    private func springBack() {
        swipeAxis = nil
        withAnimation(.tabSettling) { swipe = 0 }
    }

    private var closeButton: some View {
        Button(action: close) {
            Image(systemName: "xmark")
                .font(tokens.symbolFont(0.7, weight: .bold))
                .foregroundStyle(tokens.swiftUIColor(\.icon))
                .frame(width: 24, height: 24)
                .background(Circle().fill(tokens.swiftUIColor(\.fill)))
                .frame(width: 44, height: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Close \(title)")
    }

    /// Markdown a card leaves out, and what it keeps of it.
    private static let plainText: [(pattern: String, keep: String)] = [
        (#"!\[\[[^\]]*\]\]"#, ""),
        (#"\[\[(?:[^\]|]*\|)?([^\]]*)\]\]"#, "$1"),
        (#"\[([^\]]*)\]\([^)]*\)"#, "$1"),
        (#"(?m)^>\s?(\[![^\]]*\][+-]?\s?)?"#, ""),
        (#"(?m)^#{1,6}\s+"#, ""),
        (#"(?m)^(\s*)(?:[-*+]|\d+[.)])\s+(?:\[.\]\s+)?"#, "$1"),
        (#"[*_=`]{1,2}"#, "")
    ]

    /// The note's first lines as they read, without their Markdown.
    private func readPreview() -> String {
        guard let path = tab.path, let text = try? model.library.vault?.readNote(path: path) else {
            return tab.path == nil ? "Search or pick a note." : ""
        }
        let plain = Self.plainText.reduce(String(text.prefix(800))) { preview, rule in
            preview.replacingOccurrences(of: rule.pattern, with: rule.keep, options: .regularExpression)
        }
        return withoutRepeatedTitle(plain.trimmingCharacters(in: .whitespacesAndNewlines))
    }

    /// A note often starts with its title as a heading, which the card
    /// already shows above.
    private func withoutRepeatedTitle(_ preview: String) -> String {
        let lines = preview.split(separator: "\n", maxSplits: 1, omittingEmptySubsequences: false)
        guard let first = lines.first, first.trimmingCharacters(in: .whitespaces) == title else { return preview }
        return lines.count > 1 ? lines[1].trimmingCharacters(in: .whitespacesAndNewlines) : ""
    }
}
