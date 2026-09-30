import SwiftUI

/// The sidebar slides over the note from the left, over a dimmed backdrop.
/// A rightward swipe that starts in the left quarter of the screen opens
/// it; a swipe back or a tap on the backdrop closes it. Taps there still
/// reach the note, such as a heading's fold control in the margin.
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
        .onChange(of: isOpen) { _, open in
            if open { Haptics.arrived() }
        }
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

/// A rightward swipe from the left quarter of the screen, recognised on the
/// window by UIKit, so taps there go to what's under them.
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
    private let gate = SwipeGate()
    private lazy var recognizer: UIPanGestureRecognizer = {
        let recognizer = UIPanGestureRecognizer(target: self, action: #selector(panned(_:)))
        recognizer.delegate = gate
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
    @objc private func panned(_ recognizer: UIPanGestureRecognizer) {
        if recognizer.state == .began { opened = false }
        guard !opened, recognizer.state == .changed || recognizer.state == .ended,
              recognizer.translation(in: recognizer.view).x > Self.openDistance else { return }
        opened = true
        swiped()
    }
}

/// Where a swipe that opens the sidebar may start.
private final class SwipeGate: NSObject, UIGestureRecognizerDelegate {
    /// How far in from the left edge a swipe may start, as a share of the
    /// screen's width.
    private static let reach: CGFloat = 0.25
    /// The bar along the bottom, whose title swipes between tabs.
    private static let bottomBarHeight: CGFloat = 96

    /// Starts only for a mostly sideways swipe to the right, from the left
    /// quarter, above the bottom bar, and not on something that scrolls
    /// sideways itself, such as a wide table.
    func gestureRecognizerShouldBegin(_ gesture: UIGestureRecognizer) -> Bool {
        guard let pan = gesture as? UIPanGestureRecognizer, let window = pan.view else { return false }
        let start = pan.location(in: window)
        let velocity = pan.velocity(in: window)
        guard velocity.x > 0, abs(velocity.x) > abs(velocity.y) * 1.5 else { return false }
        guard start.x <= window.bounds.width * Self.reach,
              start.y < window.bounds.height - Self.bottomBarHeight else { return false }
        return !scrollsSideways(window.hitTest(start, with: nil))
    }

    /// Runs alongside the note's own scrolling, which a sideways swipe
    /// barely moves, rather than waiting for it to give up.
    func gestureRecognizer(
        _ gesture: UIGestureRecognizer,
        shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer
    ) -> Bool {
        true
    }

    private func scrollsSideways(_ view: UIView?) -> Bool {
        var current = view
        while let candidate = current {
            if let scroll = candidate as? UIScrollView,
               scroll.contentSize.width > scroll.bounds.width + 1 {
                return true
            }
            current = candidate.superview
        }
        return false
    }
}
