import SwiftUI
import UIKit

/// Blur at a fractional intensity, below what `.ultraThinMaterial` can reach.
///
/// `UIBlurEffect` has no intensity of its own, so the blur is installed as an
/// animation on a `UIViewPropertyAnimator` that is never started. Seeking the
/// animator to `fractionComplete` leaves the effect partly applied. The
/// animator must stay paused and retained for the effect to hold.
struct DSTabBarVariableBlur: UIViewRepresentable {
    var intensity: Double

    func makeUIView(context: Context) -> UIVisualEffectView {
        let view = UIVisualEffectView(effect: nil)
        context.coordinator.seek(view, to: intensity)
        return view
    }

    func updateUIView(_ view: UIVisualEffectView, context: Context) {
        context.coordinator.seek(view, to: intensity)
    }

    static func dismantleUIView(_: UIVisualEffectView, coordinator: Coordinator) {
        coordinator.cancel()
    }

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    final class Coordinator {
        private var animator: UIViewPropertyAnimator?

        func seek(_ view: UIVisualEffectView, to intensity: Double) {
            let animator = animator ?? makeAnimator(blurring: view)
            self.animator = animator
            // A fraction of exactly zero drops the effect rather than leaving a faint blur.
            animator.fractionComplete = min(max(intensity, 0.001), 1)
        }

        /// A running animator that outlives its view traps in UIKit, so it is
        /// stopped without finishing.
        func cancel() {
            animator?.stopAnimation(true)
            animator = nil
        }

        private func makeAnimator(blurring view: UIVisualEffectView) -> UIViewPropertyAnimator {
            let animator = UIViewPropertyAnimator(duration: 1, curve: .linear)
            animator.addAnimations { view.effect = UIBlurEffect(style: .systemUltraThinMaterial) }
            animator.pausesOnCompletion = true
            return animator
        }
    }
}
