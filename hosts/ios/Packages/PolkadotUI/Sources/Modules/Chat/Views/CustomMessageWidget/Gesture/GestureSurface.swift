import SwiftUI
import UIKit

struct GestureSurface: UIViewRepresentable {
    let props: CustomMessageWidgetNode.GestureProps
    let onPan: (CustomMessageWidgetNode.GesturePhase, CGPoint) -> Void
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

final class GestureSurfaceView: UIView {
    let panRecognizer = UIPanGestureRecognizer()
    let tapRecognizer = UITapGestureRecognizer()
    var axis: CustomMessageWidgetNode.PanAxis?
    var isInteractive = false
    var acceptsTap = false
    var onPan: ((CustomMessageWidgetNode.GesturePhase, CGPoint) -> Void)?
    var onTap: (() -> Void)?

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

    @objc func handlePan(_ recognizer: UIPanGestureRecognizer) {
        var translation = recognizer.translation(in: window)
        if effectiveUserInterfaceLayoutDirection == .rightToLeft { translation.x = -translation.x }
        switch recognizer.state {
        case .began: onPan?(.started, translation)
        case .changed: onPan?(.moved, translation)
        case .ended: onPan?(.ended, translation)
        case .cancelled,
             .failed: onPan?(.cancelled, translation)
        default: break
        }
    }
}
