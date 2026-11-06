import DesignSystem
import UIKit

/// The Wallet a Collectibles screen opens from: where the Pocket's peek sits, and how it draws the peek while the
/// screen is up.
@MainActor
protocol CollectiblesOrigin: AnyObject {
    /// The peek's frame in window coordinates, or nil when the peek is not on screen to fly from.
    var collectiblesBookFrame: CGRect? { get }

    /// Hides the peek, since the book stands in its place while the screen is up.
    func setCollectiblesBookLifted()
}

/// Carries the Collectibles book from the Pocket's peek to the top of its screen. The book lifts out of the peek and
/// settles on the page. A screen with no peek to fly from fades in instead.
final class CollectiblesBookFlight: NSObject, UIViewControllerAnimatedTransitioning {
    private static let duration: TimeInterval = 0.5

    private let screen: CollectiblesViewController
    private let origin: CollectiblesOrigin

    init(screen: CollectiblesViewController, origin: CollectiblesOrigin) {
        self.screen = screen
        self.origin = origin
    }

    func transitionDuration(using _: UIViewControllerContextTransitioning?) -> TimeInterval {
        Self.duration
    }

    func animateTransition(using context: UIViewControllerContextTransitioning) {
        let container = context.containerView
        screen.view.frame = context.finalFrame(for: screen)
        container.addSubview(screen.view)
        screen.view.layoutIfNeeded()

        guard !UIAccessibility.isReduceMotionEnabled, let slot = origin.collectiblesBookFrame else {
            fadeIn(context)
            return
        }

        let landing = screen.bookFrame(in: container)
        let fromSlot = container.convert(slot, from: nil)
        let offset = CGPoint(x: fromSlot.minX - landing.minX, y: fromSlot.minY - landing.minY)

        origin.setCollectiblesBookLifted()
        screen.beginFlight(from: offset)

        let backdrop = UIViewPropertyAnimator(duration: CollectiblesBookTiming.backdropFade, curve: .linear) { [self] in
            screen.view.backgroundColor = .bgSurfaceMain
        }

        let spring = UISpringTimingParameters(
            mass: 1,
            stiffness: CollectiblesBookTiming.flightStiffness,
            damping: 2 * sqrt(CollectiblesBookTiming.flightStiffness),
            initialVelocity: .zero
        )
        let flight = UIViewPropertyAnimator(duration: 0, timingParameters: spring)
        flight.addAnimations { [self] in
            screen.moveBook(by: .zero)
        }
        flight.addCompletion { _ in
            context.completeTransition(!context.transitionWasCancelled)
        }

        backdrop.startAnimation()
        flight.startAnimation()
    }

    /// Where the book cannot fly, the screen fades in over the Wallet.
    private func fadeIn(_ context: UIViewControllerContextTransitioning) {
        screen.view.alpha = 0

        let fade = UIViewPropertyAnimator(duration: CollectiblesBookTiming.backdropFade, curve: .linear) { [self] in
            screen.view.alpha = 1
        }
        fade.addCompletion { _ in
            context.completeTransition(!context.transitionWasCancelled)
        }
        fade.startAnimation()
    }
}
