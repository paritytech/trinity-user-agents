import SwiftUI
import UIKit

/// A pan's translation from where it was recognised, and the slop: how far the finger had already moved from its
/// touch-down point by then, which the translation leaves out. `location` and `touchDown` are where the finger is
/// and first touched inside the surface, from its top-start corner and clamped to its bounds.
struct PanTranslation: Equatable {
    var recognized: CGPoint
    var slop: CGPoint
    var location: CGPoint = .zero
    var touchDown: CGPoint = .zero
}

struct GestureSurface: UIViewRepresentable {
    let props: CustomMessageWidgetNode.GestureProps
    let onPan: (CustomMessageWidgetNode.GesturePhase, PanTranslation) -> Void
    let onTap: () -> Void
    /// A finger touching down (true) or lifting (false), at its place in the surface, before any pan decides.
    var onPress: ((Bool, CGPoint) -> Void)?

    func makeUIView(context _: Context) -> GestureSurfaceView {
        GestureSurfaceView()
    }

    func updateUIView(_ view: GestureSurfaceView, context _: Context) {
        view.isInteractive = props.isEnabled
        view.axis = props.pan?.axis
        view.acceptsTap = props.tapAction != nil
        view.onPan = onPan
        view.onTap = onTap
        view.onPress = onPress
    }
}

final class GestureSurfaceView: UIView {
    /// Our own low-threshold recogniser, or the system's pan when `GestureDragThreshold` says `system`.
    let panRecognizer: any DragRecognizing = GestureSurfaceView.makeDragRecognizer(GestureDragThreshold.current)
    let tapRecognizer = UITapGestureRecognizer()
    var axis: CustomMessageWidgetNode.PanAxis?
    var isInteractive = false
    var acceptsTap = false
    var onPan: ((CustomMessageWidgetNode.GesturePhase, PanTranslation) -> Void)?
    var onTap: (() -> Void)?
    var onPress: ((Bool, CGPoint) -> Void)?
    private var slop = CGPoint.zero

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .clear
        accessibilityElementsHidden = true
        panRecognizer.delegate = self
        panRecognizer.addTarget(self, action: #selector(handlePan))
        addGestureRecognizer(panRecognizer)
        tapRecognizer.delegate = self
        tapRecognizer.addTarget(self, action: #selector(handleTap))
        tapRecognizer.require(toFail: panRecognizer)
        addGestureRecognizer(tapRecognizer)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) { fatalError() }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesBegan(touches, with: event)
        if isInteractive, let touch = touches.first { onPress?(true, touch.location(in: self)) }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesEnded(touches, with: event)
        if let touch = touches.first { onPress?(false, touch.location(in: self)) }
    }

    /// A pan that takes the touch cancels it here first; the lift is reported after the pan has had its say.
    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesCancelled(touches, with: event)
        guard let touch = touches.first else { return }
        let place = touch.location(in: self)
        DispatchQueue.main.async { [weak self] in self?.onPress?(false, place) }
    }

    static func claimsDrag(
        axis: CustomMessageWidgetNode.PanAxis,
        translation: CGPoint,
        velocity: CGPoint
    ) -> Bool {
        let movement = translation == .zero ? velocity : translation
        guard movement != .zero else { return false }
        switch axis {
        case .horizontal: return abs(movement.x) >= abs(movement.y)
        case .vertical: return abs(movement.y) >= abs(movement.x)
        case .free: return true
        }
    }

    /// One finger, starting after `threshold` points; nil threshold is the system pan recogniser and its own slop.
    static func makeDragRecognizer(_ threshold: CGFloat?) -> any DragRecognizing {
        guard let threshold else {
            let system = TouchDownPanGestureRecognizer()
            system.maximumNumberOfTouches = 1
            return system
        }
        return ThresholdPanGestureRecognizer(threshold: threshold)
    }

    /// A point of the surface measured from its top-start corner and clamped to its bounds.
    static func inside(_ point: CGPoint, _ size: CGSize, mirrored: Bool) -> CGPoint {
        let across = min(max(point.x, 0), size.width)
        return CGPoint(x: mirrored ? size.width - across : across, y: min(max(point.y, 0), size.height))
    }

    /// In window points; zero when the touch-down was not seen.
    static func slop(touchDown: CGPoint?, location: CGPoint, translation: CGPoint) -> CGPoint {
        guard let touchDown else { return .zero }
        return CGPoint(x: location.x - touchDown.x - translation.x, y: location.y - touchDown.y - translation.y)
    }
}

extension GestureSurfaceView: UIGestureRecognizerDelegate {
    override func gestureRecognizerShouldBegin(_ recognizer: UIGestureRecognizer) -> Bool {
        guard isInteractive else { return false }
        if recognizer === tapRecognizer { return acceptsTap }
        guard let axis else { return false }
        return Self.claimsDrag(
            axis: axis,
            translation: panRecognizer.translation(in: window),
            velocity: panRecognizer.velocity(in: window)
        )
    }

    func gestureRecognizer(
        _ recognizer: UIGestureRecognizer,
        shouldBeRequiredToFailBy other: UIGestureRecognizer
    ) -> Bool {
        guard recognizer === panRecognizer, other !== panRecognizer,
              other is UIPanGestureRecognizer, let otherView = other.view else { return false }
        return isDescendant(of: otherView)
    }
}

private extension GestureSurfaceView {
    @objc func handleTap() { onTap?() }

    @objc func handlePan() {
        let recognizer = panRecognizer
        let translation = recognizer.translation(in: window)
        if recognizer.state == .began {
            slop = Self.slop(
                touchDown: recognizer.touchDown, location: recognizer.location(in: window), translation: translation
            )
        }
        let location = recognizer.location(in: self)
        let touchDown = recognizer.touchDown.map { convert($0, from: nil) } ?? location
        let mirrored = effectiveUserInterfaceLayoutDirection == .rightToLeft
        var sample = PanTranslation(
            recognized: translation,
            slop: slop,
            location: Self.inside(location, bounds.size, mirrored: mirrored),
            touchDown: Self.inside(touchDown, bounds.size, mirrored: mirrored)
        )
        if mirrored {
            sample.recognized.x = -sample.recognized.x
            sample.slop.x = -sample.slop.x
        }
        switch recognizer.state {
        case .began: onPan?(.started, sample)
        case .changed: onPan?(.moved, sample)
        case .ended: onPan?(.ended, sample)
        case .cancelled,
             .failed: onPan?(.cancelled, sample)
        default: break
        }
    }
}
