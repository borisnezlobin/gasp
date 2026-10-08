import SwiftUI
import UIKit.UIGestureRecognizerSubclass

/// Reports a finger moving over the view without claiming it, so the
/// tour's pages still swipe while the finger plays with the water.
struct FingerTrail: UIViewRepresentable {
    let onFinger: (CGPoint, FingerPhase) -> Void

    func makeUIView(context: Context) -> UIView {
        let view = UIView()
        view.backgroundColor = .clear
        view.addGestureRecognizer(FingerFollower(onFinger: onFinger))
        return view
    }

    func updateUIView(_ view: UIView, context: Context) {
        for case let follower as FingerFollower in view.gestureRecognizers ?? [] {
            follower.onFinger = onFinger
        }
    }
}

/// Watches touches and never recognizes, so it neither delays nor blocks
/// any other gesture.
private final class FingerFollower: UIGestureRecognizer {
    var onFinger: (CGPoint, FingerPhase) -> Void

    init(onFinger: @escaping (CGPoint, FingerPhase) -> Void) {
        self.onFinger = onFinger
        super.init(target: nil, action: nil)
        cancelsTouchesInView = false
        delaysTouchesBegan = false
        delaysTouchesEnded = false
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        report(touches, .down)
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
        report(touches, .moved)
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        report(touches, .lifted)
        state = .failed
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
        report(touches, .lifted)
        state = .failed
    }

    private func report(_ touches: Set<UITouch>, _ phase: FingerPhase) {
        guard let touch = touches.first else { return }
        onFinger(touch.location(in: view), phase)
    }
}
