import UIKit

/// Reports each touch that begins in its window and never recognizes one, so every touch reaches its view unchanged.
final class DockletTouchObserver: UIGestureRecognizer {
    /// Receives each touch as it begins.
    var onTouchBegan: ((UITouch) -> Void)?

    override init(target: Any?, action: Selector?) {
        super.init(target: target, action: action)
        cancelsTouchesInView = false
        delaysTouchesBegan = false
        delaysTouchesEnded = false
        delegate = self
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesBegan(touches, with: event)
        touches.forEach { onTouchBegan?($0) }
        state = .failed
    }
}

extension DockletTouchObserver: UIGestureRecognizerDelegate {
    func gestureRecognizer(
        _: UIGestureRecognizer,
        shouldRecognizeSimultaneouslyWith _: UIGestureRecognizer
    ) -> Bool {
        true
    }
}
