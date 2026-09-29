import SwiftUI

/// The sidebar slides over the note from the left, over a dimmed backdrop.
/// A swipe in from the screen's left edge opens it; a swipe back or a tap
/// on the backdrop closes it. With no navigation stack, nothing else
/// claims that edge.
struct SidebarOverlay: View {
    @Environment(AppModel.self) private var model
    @State private var drag: CGFloat = 0

    private var tokens: Tokens { model.library.tokens }
    private var isOpen: Bool { model.workspace.sidebarOpen }
    private static let edgeWidth: CGFloat = 20
    private static let closeDistance: CGFloat = 80

    var body: some View {
        GeometryReader { proxy in
            let width = min(proxy.size.width * 0.86, 360)
            ZStack(alignment: .leading) {
                if isOpen {
                    backdrop(width: width)
                    SidebarPanel()
                        .frame(width: width)
                        .background(
                            tokens.swiftUIColor(\.surface)
                                .shadow(color: tokens.swiftUIColor(\.shadow), radius: 16)
                                .ignoresSafeArea()
                        )
                        .offset(x: min(drag, 0))
                        .gesture(closeGesture)
                        .transition(.move(edge: .leading))
                } else {
                    edgeStrip(width: width)
                }
            }
        }
        .animation(.snappy(duration: 0.28), value: isOpen)
    }

    private func backdrop(width: CGFloat) -> some View {
        tokens.swiftUIColor(\.backdrop)
            .opacity(1 - min(-drag / width, 1))
            .ignoresSafeArea()
            .onTapGesture { model.workspace.sidebarOpen = false }
            .gesture(closeGesture)
            .transition(.opacity)
            .accessibilityLabel("Close the sidebar")
            .accessibilityAddTraits(.isButton)
    }

    /// A thin strip on the left edge that a swipe opens the sidebar from.
    private func edgeStrip(width: CGFloat) -> some View {
        Color.clear
            .frame(width: Self.edgeWidth)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 8)
                    .onEnded { value in
                        if value.translation.width > 40 { model.workspace.sidebarOpen = true }
                    }
            )
            .ignoresSafeArea()
            .accessibilityHidden(true)
    }

    private var closeGesture: some Gesture {
        DragGesture(minimumDistance: 12)
            .onChanged { drag = $0.translation.width }
            .onEnded { value in
                if value.translation.width < -Self.closeDistance { model.workspace.sidebarOpen = false }
                withAnimation(.snappy) { drag = 0 }
            }
    }
}
