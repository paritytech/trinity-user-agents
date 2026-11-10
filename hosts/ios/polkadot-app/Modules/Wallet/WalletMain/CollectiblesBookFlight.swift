import DesignSystem
import UIKit

/// Carries the Collectibles book from the Pocket's peek to the top of its screen. The book lifts out of the peek and
/// settles on the page. A screen with no peek to fly from fades in instead.
final class CollectiblesBookFlight: NSObject, UIViewControllerAnimatedTransitioning {
    private let screen: CollectiblesViewController
    private let origin: PocketOrigin?
    /// Read once, so the duration UIKit is told and the motion that runs agree.
    private let reduceMotion: Bool

    init(screen: CollectiblesViewController, origin: PocketOrigin?) {
        self.screen = screen
        self.origin = origin
        reduceMotion = UIAccessibility.isReduceMotionEnabled
    }

    func transitionDuration(using _: UIViewControllerContextTransitioning?) -> TimeInterval {
        PocketFlight.duration(reduceMotion: reduceMotion)
    }

    func animateTransition(using context: UIViewControllerContextTransitioning) {
        let container = context.containerView
        screen.view.frame = context.finalFrame(for: screen)
        container.addSubview(screen.view)
        screen.view.layoutIfNeeded()

        switch PocketFlight.motion(
            toSlot: slotOffset(of: screen, origin: origin, in: container),
            reduceMotion: reduceMotion
        ) {
        case let .fly(offset):
            screen.beginFlight(from: offset)

            let backdrop = backdropFade(of: screen, to: .bgSurfaceMain)
            let flight = flightAnimator(moving: screen, to: .zero)
            flight.addCompletion { _ in
                context.completeTransition(!context.transitionWasCancelled)
            }

            origin?.moveOpenItem(by: .zero, clearingPocket: true, animated: true)
            backdrop.startAnimation()
            flight.startAnimation()
        case .fade,
             .crossfade:
            PocketFlight.fade(
                context,
                screen: screen,
                origin: origin,
                opening: true,
                animator: PocketFlight.fallbackAnimator(reduceMotion: reduceMotion)
            ) {
                context.completeTransition(!context.transitionWasCancelled)
            }
        }
    }
}

/// Carries the Collectibles book from the top of its screen back to the Pocket's peek, then closes the screen.
/// A screen with no peek to fly to fades out instead.
final class CollectiblesBookReturn: NSObject, UIViewControllerAnimatedTransitioning {
    private let screen: CollectiblesViewController
    private let origin: PocketOrigin?
    /// Read once, so the duration UIKit is told and the motion that runs agree.
    private let reduceMotion: Bool

    init(screen: CollectiblesViewController, origin: PocketOrigin?) {
        self.screen = screen
        self.origin = origin
        reduceMotion = UIAccessibility.isReduceMotionEnabled
    }

    func transitionDuration(using _: UIViewControllerContextTransitioning?) -> TimeInterval {
        PocketFlight.duration(reduceMotion: reduceMotion)
    }

    func animateTransition(using context: UIViewControllerContextTransitioning) {
        let container = context.containerView
        if let wallet = context.viewController(forKey: .to) {
            container.insertSubview(wallet.view, belowSubview: screen.view)
            wallet.view.frame = context.finalFrame(for: wallet)
        }

        switch PocketFlight.motion(
            toSlot: slotOffset(of: screen, origin: origin, in: container),
            reduceMotion: reduceMotion
        ) {
        case let .fly(offset):
            screen.close { [self] in
                flyBack(to: offset, context)
            }
        case .fade,
             .crossfade:
            PocketFlight.fade(
                context,
                screen: screen,
                origin: origin,
                opening: false,
                animator: PocketFlight.fallbackAnimator(reduceMotion: reduceMotion)
            ) {
                context.completeTransition(!context.transitionWasCancelled)
            }
        }
    }

    private func flyBack(to offset: CGPoint, _ context: UIViewControllerContextTransitioning) {
        let backdrop = backdropFade(of: screen, to: .clear)
        let flight = flightAnimator(moving: screen, to: offset)
        flight.addCompletion { _ in
            context.completeTransition(!context.transitionWasCancelled)
        }

        origin?.moveOpenItem(by: .zero, clearingPocket: false, animated: true)
        backdrop.startAnimation()
        flight.startAnimation()
    }
}

/// The offset that moves a book resting in `container` onto the Pocket's peek, or nil when the Pocket has no peek.
/// The book's own frame is unchanged by the close that comes before a return, since the lift moves a subview.
@MainActor
private func slotOffset(
    of screen: CollectiblesViewController,
    origin: PocketOrigin?,
    in container: UIView
) -> CGPoint? {
    origin?.frame(of: .collectibles).map { slot in
        offsetToSlot(container.convert(slot, from: nil), fromLanding: screen.bookFrame(in: container))
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
