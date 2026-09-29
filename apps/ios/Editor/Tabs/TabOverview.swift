import SwiftUI

/// Every open tab as a card, like Safari's tab view: tap one to show it,
/// close one with its button, or start a new one.
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

    private var radius: CGFloat { CGFloat(tokens.spacing.radiusLg) }

    var body: some View {
        Button(action: open) {
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
            .padding(isActive ? 3 : 0)
            .background(
                RoundedRectangle(cornerRadius: radius + 3)
                    .fill(isActive ? tokens.swiftUIColor(\.accent) : .clear)
            )
        }
        .buttonStyle(.plain)
        .overlay(alignment: .topTrailing) { closeButton }
        .accessibilityLabel(isActive ? "\(title), showing" : title)
        .task(id: tab.path) { preview = readPreview() }
    }

    private var closeButton: some View {
        Button(action: close) {
            Image(systemName: "xmark")
                .font(.system(size: 11, weight: .bold))
                .foregroundStyle(tokens.swiftUIColor(\.icon))
                .frame(width: 24, height: 24)
                .background(Circle().fill(tokens.swiftUIColor(\.fill)))
                .frame(width: 44, height: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(isActive ? 3 : 0)
        .accessibilityLabel("Close \(title)")
    }

    /// Markdown a card leaves out, and what it keeps of it.
    private static let plainText: [(pattern: String, keep: String)] = [
        (#"!\[\[[^\]]*\]\]"#, ""),
        (#"\[\[(?:[^\]|]*\|)?([^\]]*)\]\]"#, "$1"),
        (#"\[([^\]]*)\]\([^)]*\)"#, "$1"),
        (#"(?m)^>\s?(\[![^\]]*\][+-]?\s?)?"#, ""),
        (#"(?m)^#{1,6}\s+"#, ""),
        (#"[*_=`]{1,2}"#, "")
    ]

    /// The note's first lines as they read, without their Markdown.
    private func readPreview() -> String {
        guard let path = tab.path, let text = try? model.library.vault?.readNote(path: path) else {
            return tab.path == nil ? "Search or pick a note." : ""
        }
        return Self.plainText.reduce(String(text.prefix(800))) { preview, rule in
            preview.replacingOccurrences(of: rule.pattern, with: rule.keep, options: .regularExpression)
        }
    }
}
