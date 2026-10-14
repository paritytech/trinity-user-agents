import UIKit
import UIKit.UIGestureRecognizerSubclass

final class DSTabSelectionRecognizer: UIGestureRecognizer {
    private var trackedTouch: UITouch?
    private var startLocation: CGPoint = .zero
    private var previousSample: (location: CGPoint, timestamp: TimeInterval)?
    private var latestSample: (location: CGPoint, timestamp: TimeInterval)?
    private var endTimestamp: TimeInterval?

    func translation(in view: UIView?) -> CGPoint {
        guard let trackedTouch else {
            return .zero
        }
        let startInView = view?.convert(startLocation, from: nil) ?? startLocation
        let current = trackedTouch.location(in: view)
        return CGPoint(x: current.x - startInView.x, y: current.y - startInView.y)
    }

    func velocity(in view: UIView?) -> CGPoint {
        guard
            let previousSample,
            let latestSample,
            case let elapsed = latestSample.timestamp - previousSample.timestamp,
            elapsed > 0
        else {
            return .zero
        }

        if let endTimestamp, endTimestamp - latestSample.timestamp > DSTabBarMetrics.foldFlickMaxSampleAge {
            return .zero
        }

        let from = view?.convert(previousSample.location, from: nil) ?? previousSample.location
        let to = view?.convert(latestSample.location, from: nil) ?? latestSample.location

        return CGPoint(x: (to.x - from.x) / elapsed, y: (to.y - from.y) / elapsed)
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesBegan(touches, with: event)

        guard trackedTouch == nil, touches.count == 1, let touch = touches.first else {
            touches.forEach { ignore($0, for: event) }
            return
        }
        trackedTouch = touch
        startLocation = touch.location(in: nil)
        previousSample = (startLocation, touch.timestamp)
        latestSample = previousSample
        state = .began
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesMoved(touches, with: event)

        guard tracks(touches), let touch = trackedTouch else {
            return
        }
        previousSample = latestSample
        latestSample = (touch.location(in: nil), touch.timestamp)

        // A vertical drag that began on the capsule is a scroll attempt, not a selection.
        // Horizontal travel stays legitimate: it drives drag-across-tabs and is clamped.
        guard !hasExceededVerticalSlop else {
            state = .cancelled
            return
        }

        state = .changed
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesEnded(touches, with: event)

        guard tracks(touches) else {
            return
        }
        endTimestamp = trackedTouch?.timestamp
        state = .ended
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesCancelled(touches, with: event)

        guard tracks(touches) else {
            return
        }
        state = .cancelled
    }

    override func reset() {
        super.reset()

        trackedTouch = nil
        startLocation = .zero
        previousSample = nil
        latestSample = nil
        endTimestamp = nil
    }
}

private extension DSTabSelectionRecognizer {
    var hasExceededVerticalSlop: Bool {
        guard let latestSample else {
            return false
        }
        return abs(latestSample.location.y - startLocation.y) > DSTabBarMetrics.selectionCancelVerticalSlop
    }

    func tracks(_ touches: Set<UITouch>) -> Bool {
        guard let trackedTouch else {
            return false
        }
        return touches.contains(trackedTouch)
    }
}
