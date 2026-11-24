import UIKit
import UIKit.UIGestureRecognizerSubclass

/// How far a finger must move before a gesture node's pan starts, in points.
///
/// The system pan recogniser waits for about 10 points (about 1.7 mm) and its slop can't be set, so the node uses its
/// own recogniser with a lower threshold, about 1 mm. The app's
/// `GestureDragThreshold` default picks it at launch, so one build can run both:
/// absent → `defaultPoints`; a number → that many points; `system` → the system pan recogniser, as before.
/// (`simctl spawn <device> defaults write io.parity.polkadotapp.develop GestureDragThreshold -string system`)
enum GestureDragThreshold {
    static let defaultPoints: CGFloat = 5
    static let settingKey = "GestureDragThreshold"

    /// The threshold in points, or nil for the system pan recogniser.
    static func points(setting: String?) -> CGFloat? {
        guard let setting, !setting.isEmpty else { return defaultPoints }
        if setting == "system" { return nil }
        guard let value = Double(setting), value >= 0 else { return defaultPoints }
        return CGFloat(value)
    }

    static let current: CGFloat? = points(setting: UserDefaults.standard.string(forKey: settingKey))

    /// A finger that has moved `distance` from its touch-down has crossed the threshold.
    static func crosses(_ distance: CGPoint, threshold: CGFloat) -> Bool {
        (distance.x * distance.x + distance.y * distance.y).squareRoot() >= threshold
    }
}

/// What a gesture surface needs from its drag recogniser, whichever threshold it uses.
@MainActor
protocol DragRecognizing: UIGestureRecognizer {
    /// Where the finger first touched, in window points.
    var touchDown: CGPoint? { get }
    func translation(in view: UIView?) -> CGPoint
    func velocity(in view: UIView?) -> CGPoint
}

/// Remembers where the finger first touched, which a pan recogniser only reports once it has crossed its slop.
final class TouchDownPanGestureRecognizer: UIPanGestureRecognizer, DragRecognizing {
    private(set) var touchDown: CGPoint?

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        if touchDown == nil { touchDown = touches.first?.location(in: nil) }
        super.touchesBegan(touches, with: event)
    }

    override func reset() {
        super.reset()
        touchDown = nil
    }
}

/// A one-finger pan that begins once the finger is `threshold` points from its touch-down. Translation is measured
/// from where it began, as `UIPanGestureRecognizer`'s is, so the surface's catch-up works the same with either.
final class ThresholdPanGestureRecognizer: UIGestureRecognizer, DragRecognizing {
    let threshold: CGFloat
    /// Begins at the touch-down, with no movement, for a pan that takes the finger where it lands.
    var beginsAtTouchDown = false
    private(set) var touchDown: CGPoint?
    private var start = CGPoint.zero
    private var current = CGPoint.zero
    private var previous = CGPoint.zero
    private var previousTime: TimeInterval = 0
    private var currentTime: TimeInterval = 0

    init(threshold: CGFloat) {
        self.threshold = threshold
        super.init(target: nil, action: nil)
    }

    /// Before it begins, the movement from the touch-down (what decides the axis); after, from where it began.
    func translation(in _: UIView?) -> CGPoint {
        let from = state == .possible ? touchDown ?? current : start
        return CGPoint(x: current.x - from.x, y: current.y - from.y)
    }

    /// Points a second over the last two touches.
    func velocity(in _: UIView?) -> CGPoint {
        let elapsed = currentTime - previousTime
        guard elapsed > 0 else { return .zero }
        return CGPoint(x: (current.x - previous.x) / elapsed, y: (current.y - previous.y) / elapsed)
    }

    override func location(in view: UIView?) -> CGPoint {
        guard let view else { return current }
        return view.convert(current, from: nil)
    }

    override func touchesBegan(_ touches: Set<UITouch>, with _: UIEvent) {
        guard touchDown == nil, touches.count == 1, let touch = touches.first else {
            if state == .possible { state = .failed }
            return
        }
        touchDown = touch.location(in: nil)
        current = touchDown ?? .zero
        previous = current
        currentTime = touch.timestamp
        previousTime = currentTime
        if beginsAtTouchDown {
            start = current
            state = .began
        }
    }

    override func touchesMoved(_ touches: Set<UITouch>, with _: UIEvent) {
        guard let touch = touches.first, let touchDown else { return }
        previous = current
        previousTime = currentTime
        current = touch.location(in: nil)
        currentTime = touch.timestamp
        switch state {
        case .possible:
            let moved = CGPoint(x: current.x - touchDown.x, y: current.y - touchDown.y)
            if GestureDragThreshold.crosses(moved, threshold: threshold) {
                start = current
                state = .began
            }
        case .began,
             .changed:
            state = .changed
        default: break
        }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with _: UIEvent) {
        if let touch = touches.first { current = touch.location(in: nil) }
        state = state == .possible ? .failed : .ended
    }

    override func touchesCancelled(_: Set<UITouch>, with _: UIEvent) {
        state = state == .possible ? .failed : .cancelled
    }

    override func reset() {
        super.reset()
        touchDown = nil
        start = .zero
        current = .zero
        previous = .zero
    }
}
