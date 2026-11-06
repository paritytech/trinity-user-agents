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

    /// Shows the peek again once the book has landed on it, coloured, then fades it back to grey.
    func setCollectiblesBookLanded()
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
        let offset = offsetToSlot(container.convert(slot, from: nil), fromLanding: landing)

        origin.setCollectiblesBookLifted()
        screen.beginFlight(from: offset)

        let backdrop = backdropFade(of: screen, to: .bgSurfaceMain)
        let flight = flightAnimator(moving: screen, to: .zero)
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

/// Carries the Collectibles book from the top of its screen back to the Pocket's peek, then closes the screen.
/// A screen with no peek to fly to fades out instead.
final class CollectiblesBookReturn: NSObject, UIViewControllerAnimatedTransitioning {
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
        if let wallet = context.viewController(forKey: .to) {
            container.insertSubview(wallet.view, belowSubview: screen.view)
            wallet.view.frame = context.finalFrame(for: wallet)
        }

        guard !UIAccessibility.isReduceMotionEnabled, let slot = origin.collectiblesBookFrame else {
            fadeOut(context)
            return
        }

        screen.close { [self] in
            flyBack(to: slot, context)
        }
    }

    private func flyBack(to slot: CGRect, _ context: UIViewControllerContextTransitioning) {
        let container = context.containerView
        let landing = screen.bookFrame(in: container)
        let offset = offsetToSlot(container.convert(slot, from: nil), fromLanding: landing)

        let backdrop = backdropFade(of: screen, to: .clear)
        let flight = flightAnimator(moving: screen, to: offset)
        flight.addCompletion { [self] _ in
            origin.setCollectiblesBookLanded()
            context.completeTransition(!context.transitionWasCancelled)
        }

        backdrop.startAnimation()
        flight.startAnimation()
    }

    /// Where the book cannot fly back, the screen fades out over the Wallet.
    private func fadeOut(_ context: UIViewControllerContextTransitioning) {
        let fade = UIViewPropertyAnimator(duration: CollectiblesBookTiming.backdropFade, curve: .linear) { [self] in
            screen.view.alpha = 0
        }
        fade.addCompletion { [self] _ in
            origin.setCollectiblesBookLanded()
            context.completeTransition(!context.transitionWasCancelled)
        }
        fade.startAnimation()
    }
}

/// The offset that moves a book resting at `landing` onto `slot`, both in the same coordinates.
private func offsetToSlot(_ slot: CGRect, fromLanding landing: CGRect) -> CGPoint {
    CGPoint(x: slot.minX - landing.minX, y: slot.minY - landing.minY)
}

/// The book's flight to `offset` from its place, on the spring the push and the return share.
private func flightAnimator(moving screen: CollectiblesViewController, to offset: CGPoint) -> UIViewPropertyAnimator {
    let spring = UISpringTimingParameters(
        mass: 1,
        stiffness: CollectiblesBookTiming.flightStiffness,
        damping: 2 * sqrt(CollectiblesBookTiming.flightStiffness),
        initialVelocity: .zero
    )
    let flight = UIViewPropertyAnimator(duration: 0, timingParameters: spring)
    flight.addAnimations {
        screen.moveBook(by: offset)
    }
    return flight
}

/// Fades the screen's backdrop to `color`, so the Wallet shows through or is covered again.
private func backdropFade(of screen: CollectiblesViewController, to color: UIColor) -> UIViewPropertyAnimator {
    UIViewPropertyAnimator(duration: CollectiblesBookTiming.backdropFade, curve: .linear) {
        screen.view.backgroundColor = color
    }
}
