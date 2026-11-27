import UIKit

/// A close that fades, under a finger. The screen fades over the Pocket at rest, together with the backdrop that
/// covers the Pocket under it, and a release fades the rest of the way or back.
@MainActor
struct PocketFadeScrub {
    private let screen: UIViewController
    private let backdrop: UIView

    init(_ context: UIViewControllerContextTransitioning, screen: UIViewController, origin: PocketOrigin?) {
        self.screen = screen
        origin?.moveOpenItem(by: .zero, clearingPocket: false, animated: false)
        backdrop = PocketFlight.insertBackdrop(below: screen.view, in: context.containerView)
    }

    /// Draws the fade at `progress`, from 0, the screen up, to 1, the screen gone.
    func update(progress: CGFloat) {
        setAlpha(1 - progress)
    }

    /// Fades the rest of the way from `progress`, or back, in that share of the crossfade's time.
    func release(progress: CGFloat, commits: Bool, then completion: @escaping () -> Void) {
        let remaining = commits ? 1 - progress : progress
        let fade = UIViewPropertyAnimator(
            duration: PocketFlight.crossfadeDuration * Double(remaining),
            curve: .easeInOut
        )
        fade.addAnimations {
            setAlpha(commits ? 0 : 1)
        }
        fade.addCompletion { _ in
            backdrop.removeFromSuperview()
            completion()
        }
        fade.startAnimation()
    }
}

private extension PocketFadeScrub {
    func setAlpha(_ alpha: CGFloat) {
        screen.view.alpha = alpha
        backdrop.alpha = alpha
    }
}
