import SwiftUI

/// The sidebar slides over the note from the left, over a dimmed backdrop.
/// A swipe in from the screen's left edge opens it; a swipe back or a tap
/// on the backdrop closes it. With no navigation stack, nothing else
/// claims that edge, and a tap there still reaches the note, such as a
/// heading's fold control in the margin.
struct SidebarOverlay: View {
    @Environment(AppModel.self) private var model
    @State private var drag: CGFloat = 0

    private var tokens: Tokens { model.library.tokens }
    private var isOpen: Bool { model.workspace.sidebarOpen }
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
                    EdgeSwipe { model.workspace.sidebarOpen = true }
                        .frame(width: 0, height: 0)
                        .allowsHitTesting(false)
                        .accessibilityHidden(true)
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

    private var closeGesture: some Gesture {
        DragGesture(minimumDistance: 12)
            .onChanged { drag = $0.translation.width }
            .onEnded { value in
                if value.translation.width < -Self.closeDistance { model.workspace.sidebarOpen = false }
                withAnimation(.snappy) { drag = 0 }
            }
    }
}

/// A swipe in from the screen's left edge, recognised on the window by
/// UIKit, so taps near the edge go to what's under them.
private struct EdgeSwipe: UIViewRepresentable {
    let swiped: () -> Void

    func makeUIView(context: Context) -> EdgeSwipeView {
        EdgeSwipeView(swiped: swiped)
    }

    func updateUIView(_ view: EdgeSwipeView, context: Context) {
        view.swiped = swiped
    }
}

final class EdgeSwipeView: UIView {
    var swiped: () -> Void
    private lazy var recognizer: UIScreenEdgePanGestureRecognizer = {
        let recognizer = UIScreenEdgePanGestureRecognizer(target: self, action: #selector(panned(_:)))
        recognizer.edges = .left
        return recognizer
    }()
    private var opened = false
    private static let openDistance: CGFloat = 40

    init(swiped: @escaping () -> Void) {
        self.swiped = swiped
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("Views aren't decoded")
    }

    override func willMove(toWindow newWindow: UIWindow?) {
        recognizer.view?.removeGestureRecognizer(recognizer)
        newWindow?.addGestureRecognizer(recognizer)
    }

    /// Opens once per swipe, as soon as it has come far enough.
    @objc private func panned(_ recognizer: UIScreenEdgePanGestureRecognizer) {
        if recognizer.state == .began { opened = false }
        guard !opened, recognizer.state == .changed || recognizer.state == .ended,
              recognizer.translation(in: recognizer.view).x > Self.openDistance else { return }
        opened = true
        swiped()
    }
}
