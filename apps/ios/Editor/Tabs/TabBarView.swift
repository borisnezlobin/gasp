import SwiftUI

/// The bar at the bottom: the sidebar on the left, the note showing in the
/// middle (swipe it sideways for the neighbouring tab, tap it for every
/// tab), and how many tabs are open on the right.
struct TabBarView: View {
    @Environment(AppModel.self) private var model
    @State private var drag: CGFloat = 0

    /// Room the bar takes at the bottom of the screen.
    static let clearance: CGFloat = 84
    private static let switchDistance: CGFloat = 60

    private var tokens: Tokens { model.library.tokens }

    var body: some View {
        HStack(spacing: tokens.spacing.md) {
            BarButton(symbol: "sidebar.leading", label: "Show the sidebar", tokens: tokens) {
                model.workspace.sidebarOpen = true
            }
            titlePill
            BarButton(symbol: "square.on.square", label: "Show all tabs", tokens: tokens) {
                model.workspace.overviewOpen = true
            }
            .overlay(alignment: .center) {
                Text("\(model.tabs.tabs.count)")
                    .font(Font(tokens.uiFont(size: tokens.smallSize * 0.85, bold: true)))
                    .foregroundStyle(tokens.swiftUIColor(\.icon))
                    .offset(x: 2, y: 2)
                    .allowsHitTesting(false)
            }
        }
        .padding(tokens.spacing.sm)
        .background(
            Capsule()
                .fill(tokens.swiftUIColor(\.popover))
                .padding(1)
                .background(Capsule().fill(tokens.swiftUIColor(\.ring)))
                .shadow(color: tokens.swiftUIColor(\.shadow), radius: 12, y: 4)
        )
        .padding(.horizontal, tokens.spacing.xl)
        .padding(.bottom, tokens.spacing.sm)
    }

    private var titlePill: some View {
        let tab = model.tabs.active
        return VStack(spacing: 0) {
            Text(model.tabs.title(of: tab))
                .font(Font(tokens.textFont(size: tokens.bodySize, bold: true)))
                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
            if let folder = folder(of: tab) {
                Text(folder)
                    .font(Font(tokens.uiFont(size: tokens.smallSize * 0.85)))
                    .foregroundStyle(tokens.swiftUIColor(\.textDetail))
            }
        }
        .lineLimit(1)
        .frame(maxWidth: .infinity, minHeight: 44)
        .contentShape(Rectangle())
        .offset(x: drag)
        .opacity(1 - min(abs(drag) / 200, 0.5))
        .onTapGesture { model.workspace.overviewOpen = true }
        .gesture(switchGesture)
        .accessibilityElement(children: .combine)
        .accessibilityHint("Swipe to move between tabs. Tap to see them all.")
        .accessibilityAddTraits(.isButton)
    }

    private var switchGesture: some Gesture {
        DragGesture(minimumDistance: 12)
            .onChanged { drag = $0.translation.width }
            .onEnded { value in
                let distance = value.translation.width
                if distance < -Self.switchDistance { model.tabs.selectNext() }
                if distance > Self.switchDistance { model.tabs.selectPrevious() }
                withAnimation(.snappy) { drag = 0 }
            }
    }

    private func folder(of tab: BrowserTab) -> String? {
        guard let path = tab.path else { return nil }
        let folder = (path as NSString).deletingLastPathComponent
        return folder.isEmpty ? nil : folder
    }
}

/// A round icon button in the bottom bar.
struct BarButton: View {
    let symbol: String
    let label: String
    let tokens: Tokens
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 18, weight: .regular))
                .foregroundStyle(tokens.swiftUIColor(\.icon))
                .frame(width: 44, height: 44)
                .contentShape(Circle())
        }
        .buttonStyle(PressFillStyle(tokens: tokens))
        .accessibilityLabel(label)
    }
}

/// A fill that shows while a button is held.
struct PressFillStyle: ButtonStyle {
    let tokens: Tokens

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .background(
                Circle().fill(configuration.isPressed ? tokens.swiftUIColor(\.fillStrong) : .clear)
            )
    }
}
