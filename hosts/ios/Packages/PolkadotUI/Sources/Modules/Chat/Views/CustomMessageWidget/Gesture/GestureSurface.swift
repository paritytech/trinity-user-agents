import SwiftUI
import UIKit
import UIKit.UIGestureRecognizerSubclass

/// A pan's translation from where it was recognised, and the slop: how far the finger had already moved from its
/// touch-down point by then, which the translation leaves out.
struct PanTranslation: Equatable {
    var recognized: CGPoint
    var slop: CGPoint
}

struct GestureSurface: UIViewRepresentable {
    let props: CustomMessageWidgetNode.GestureProps
    let onPan: (CustomMessageWidgetNode.GesturePhase, PanTranslation) -> Void
    let onTap: () -> Void

    func makeUIView(context _: Context) -> GestureSurfaceView {
        GestureSurfaceView()
    }

    func updateUIView(_ view: GestureSurfaceView, context _: Context) {
        view.isInteractive = props.isEnabled
        view.axis = props.pan?.axis
        view.acceptsTap = props.tapAction != nil
        view.onPan = onPan
        view.onTap = onTap
    }
}

/// Remembers where the finger first touched, which a pan recogniser only reports once it has crossed its slop.
final class TouchDownPanGestureRecognizer: UIPanGestureRecognizer {
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

final class GestureSurfaceView: UIView {
    let panRecognizer = TouchDownPanGestureRecognizer()
    let tapRecognizer = UITapGestureRecognizer()
    var axis: CustomMessageWidgetNode.PanAxis?
    var isInteractive = false
    var acceptsTap = false
    var onPan: ((CustomMessageWidgetNode.GesturePhase, PanTranslation) -> Void)?
    var onTap: (() -> Void)?
    private var slop = CGPoint.zero

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .clear
        accessibilityElementsHidden = true
        panRecognizer.delegate = self
        panRecognizer.maximumNumberOfTouches = 1
        panRecognizer.addTarget(self, action: #selector(handlePan))
        addGestureRecognizer(panRecognizer)
        tapRecognizer.delegate = self
        tapRecognizer.addTarget(self, action: #selector(handleTap))
        tapRecognizer.require(toFail: panRecognizer)
        addGestureRecognizer(tapRecognizer)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) { fatalError() }

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

    @objc func handlePan(_ recognizer: TouchDownPanGestureRecognizer) {
        let translation = recognizer.translation(in: window)
        if recognizer.state == .began {
            slop = Self.slop(
                touchDown: recognizer.touchDown, location: recognizer.location(in: window), translation: translation
            )
        }
        var sample = PanTranslation(recognized: translation, slop: slop)
        if effectiveUserInterfaceLayoutDirection == .rightToLeft {
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
