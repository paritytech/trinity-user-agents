import SwiftUI
import UIKit

/// Takes the slide's drag in UIKit rather than as a SwiftUI gesture, so it can
/// be ranked against the pans of the views that hold the card: the chat
/// bubble's swipe to reply and the message list's scroll. Those wait for this
/// pan to fail, which it does at once for a drag that starts mostly vertical,
/// so the list still scrolls from on top of the control.
struct SlideToConfirmDragSurface: UIViewRepresentable {
    let isInteractive: Bool
    /// Translation from where the drag began, along the reading direction.
    let onChange: (CGFloat) -> Void
    /// Whether the finger lifted, rather than the system cancelling the drag.
    let onEnd: (Bool) -> Void

    func makeUIView(context _: Context) -> SlideToConfirmDragView {
        SlideToConfirmDragView()
    }

    func updateUIView(_ view: SlideToConfirmDragView, context _: Context) {
        view.isInteractive = isInteractive
        view.onChange = onChange
        view.onEnd = onEnd
    }
}

final class SlideToConfirmDragView: UIView {
    let panRecognizer = UIPanGestureRecognizer()
    var isInteractive = false
    var onChange: ((CGFloat) -> Void)?
    var onEnd: ((Bool) -> Void)?

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .clear
        // The SwiftUI control above speaks for the slide to VoiceOver.
        accessibilityElementsHidden = true
        panRecognizer.delegate = self
        panRecognizer.addTarget(self, action: #selector(handlePan))
        addGestureRecognizer(panRecognizer)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) { fatalError() }

    /// Whether `other` has to wait for the slide's pan to fail before it may
    /// begin: any pan on a view that holds this one.
    func outranks(_ other: UIGestureRecognizer) -> Bool {
        guard other !== panRecognizer,
              other is UIPanGestureRecognizer,
              let otherView = other.view else {
            return false
        }
        return isDescendant(of: otherView)
    }

    /// Whether a drag with this translation and velocity belongs to the slide.
    static func claimsDrag(translation: CGPoint, velocity: CGPoint) -> Bool {
        let movement = translation == .zero ? velocity : translation
        return movement != .zero && abs(movement.x) >= abs(movement.y)
    }
}

extension SlideToConfirmDragView: UIGestureRecognizerDelegate {
    override func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
        guard gestureRecognizer === panRecognizer else {
            return super.gestureRecognizerShouldBegin(gestureRecognizer)
        }
        return isInteractive && Self.claimsDrag(
            translation: panRecognizer.translation(in: self),
            velocity: panRecognizer.velocity(in: self)
        )
    }

    func gestureRecognizer(
        _ gestureRecognizer: UIGestureRecognizer,
        shouldBeRequiredToFailBy otherGestureRecognizer: UIGestureRecognizer
    ) -> Bool {
        gestureRecognizer === panRecognizer && outranks(otherGestureRecognizer)
    }
}

private extension SlideToConfirmDragView {
    @objc
    func handlePan(_ recognizer: UIPanGestureRecognizer) {
        switch recognizer.state {
        case .began,
             .changed:
            onChange?(readingTranslation(recognizer.translation(in: self).x))
        case .ended:
            onEnd?(true)
        case .cancelled,
             .failed:
            onEnd?(false)
        default:
            break
        }
    }

    func readingTranslation(_ horizontal: CGFloat) -> CGFloat {
        effectiveUserInterfaceLayoutDirection == .rightToLeft ? -horizontal : horizontal
    }
}
